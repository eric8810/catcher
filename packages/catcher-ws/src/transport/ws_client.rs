//! WebSocket 传输层 — 完整集成
//!
//! 使用 yawc 建立连接，通过 mpsc channel 推送 WsEvent。
//! 集成：headers/protocols 握手、多端点竞速、自动重连、心跳采样、压缩配置。

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use tokio::sync::mpsc;
use url::Url;
use yawc::{Frame, OpCode};

use crate::types::ws::*;
use crate::ws::{
    build_ws_config, decode_application_compression_frame, encode_application_compression_frame,
    EndpointRacer, HeartbeatManager, ReconnectManager, APPLICATION_COMPRESSION_MAGIC,
};
use catcher_core::CatcherError;
use catcher_dns::reqwest_resolver::build_reqwest_resolver;

// ── 类型别名 ──

/// 底层 WebSocket 流类型
pub(crate) type WsStream = yawc::HttpWebSocket;

// ── 命令（WsHandle → 内部任务）──

enum WsCommand {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
    NetworkChanged,
}

// ── 公开类型 ──

/// WebSocket 传输层（静态入口）
pub struct WsTransport;

/// WebSocket 连接句柄 — 跨重连保持有效，用于发送消息
#[derive(Clone)]
pub struct WsHandle {
    url: String,
    cmd_tx: mpsc::UnboundedSender<WsCommand>,
}

impl WsHandle {
    /// 发送文本消息
    pub fn send_text(&self, text: &str) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::Text(text.to_string()))
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006,
                reason: "connection closed".into(),
            })
    }

    /// 发送二进制消息
    pub fn send_binary(&self, data: &[u8]) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::Binary(data.to_vec()))
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006,
                reason: "connection closed".into(),
            })
    }

    /// 关闭连接
    pub fn close(&self, code: u16, reason: &str) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::Close {
                code,
                reason: reason.to_string(),
            })
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006,
                reason: "connection closed".into(),
            })
    }

    /// 通知库网络环境已发生变化（WiFi 切换、VPN 换节点、蜂窝/WiFi 切换等）。
    ///
    /// 旧连接在网络切换后通常处于半开状态，被动等待心跳超时需要 10-30 秒。
    /// 调用此方法立即：
    /// 1. 断开当前连接（不等心跳超时）
    /// 2. 清空 DNS 缓存（新网络下解析结果可能不同）
    /// 3. 重置重连退避计数，跳过退避延迟立即重连
    /// 4. 多端点配置时重新竞速（新网络下最优端点可能不同）
    ///
    /// 重连期间正在等待退避的也会被打断并立即重试。
    pub fn network_changed(&self) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::NetworkChanged)
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006,
                reason: "connection closed".into(),
            })
    }

    /// 返回连接的 URL（初始连接的 URL）
    pub fn url(&self) -> &str {
        &self.url
    }
}

// ── 内部状态 ──

struct HeartbeatState {
    mgr: HeartbeatManager,
    waiting_for_pong: bool,
    ping_sent_at: Option<Instant>,
}

enum LoopOutcome {
    CleanClose,
    Disconnected { code: u16, reason: String },
    HeartbeatTimeout,
    NetworkChanged,
}

fn application_compression_algorithm_name(
    algorithm: ApplicationCompressionAlgorithm,
) -> &'static str {
    match algorithm {
        ApplicationCompressionAlgorithm::Gzip => "gzip",
        ApplicationCompressionAlgorithm::Zstd => "zstd",
    }
}

fn encode_text_message(text: &str, config: &WsClientConfig) -> Result<Frame, CatcherError> {
    if config.msgpack {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            if let Ok(bin) = rmp_serde::to_vec(&value) {
                return encode_binary_message(&bin, config);
            }
        }
    }

    if !config.per_message_deflate {
        if let Some(ref compression) = config.application_compression {
            if let Some(frame) =
                encode_application_compression_frame(text.as_bytes(), false, compression)?
            {
                return Ok(Frame::binary(frame));
            }
        }
    }
    Ok(Frame::text(text.as_bytes().to_vec()))
}

fn encode_binary_message(data: &[u8], config: &WsClientConfig) -> Result<Frame, CatcherError> {
    if !config.per_message_deflate {
        if let Some(ref compression) = config.application_compression {
            if let Some(frame) = encode_application_compression_frame(data, true, compression)? {
                return Ok(Frame::binary(frame));
            }
        }
    }
    Ok(Frame::binary(data.to_vec()))
}

fn decode_binary_message(
    data: &[u8],
    config: &WsClientConfig,
) -> Result<(Vec<u8>, bool), CatcherError> {
    let (payload, is_binary) =
        match decode_application_compression_frame(data, config.max_payload_bytes)? {
            Some(frame) => (frame.data, frame.is_binary),
            None => (data.to_vec(), true),
        };

    if config.msgpack && is_binary {
        if let Ok(value) = rmp_serde::from_slice::<serde_json::Value>(&payload) {
            if let Ok(json) = serde_json::to_string(&value) {
                return Ok((json.into_bytes(), false));
            }
        }
    }

    Ok((payload, is_binary))
}

// ── 底层连接 ──

fn parse_ws_url(url: &str) -> Result<Url, CatcherError> {
    let parsed =
        Url::parse(url).map_err(|e| CatcherError::Internal(format!("invalid WS URL: {e}")))?;
    match parsed.scheme() {
        "ws" | "wss" => Ok(parsed),
        scheme => Err(CatcherError::InvalidConfig(format!(
            "unsupported WS URL scheme: {scheme}",
        ))),
    }
}

/// 构建 WebSocket 握手请求头。
fn build_handshake_headers(config: &WsClientConfig) -> Result<HeaderMap, CatcherError> {
    let mut headers = HeaderMap::new();
    for (k, v) in &config.headers {
        let name = HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| CatcherError::Internal(format!("invalid header name '{k}': {e}")))?;
        let value = HeaderValue::from_str(v)
            .map_err(|e| CatcherError::Internal(format!("invalid header value for '{k}': {e}")))?;
        headers.insert(name, value);
    }

    if !config.per_message_deflate {
        if let Some(ref compression) = config.application_compression {
            if compression.enabled {
                let algorithm = application_compression_algorithm_name(compression.algorithm);
                let value = HeaderValue::from_str(algorithm).map_err(|e| {
                    CatcherError::Internal(format!("invalid compression header: {e}"))
                })?;
                headers.insert("X-Catcher-Application-Compression", value);

                let format = std::str::from_utf8(APPLICATION_COMPRESSION_MAGIC).map_err(|e| {
                    CatcherError::Internal(format!("invalid compression magic: {e}"))
                })?;
                let value = HeaderValue::from_str(format).map_err(|e| {
                    CatcherError::Internal(format!("invalid compression format header: {e}"))
                })?;
                headers.insert("X-Catcher-Application-Compression-Format", value);

                let value = HeaderValue::from_str(&compression.threshold_bytes.to_string())
                    .map_err(|e| {
                        CatcherError::Internal(format!("invalid compression threshold header: {e}"))
                    })?;
                headers.insert("X-Catcher-Application-Compression-Threshold", value);
            }
        }
    }

    if !config.protocols.is_empty() {
        let value = HeaderValue::from_str(&config.protocols.join(", "))
            .map_err(|e| CatcherError::Internal(format!("invalid protocols: {e}")))?;
        headers.insert("Sec-WebSocket-Protocol", value);
    }

    Ok(headers)
}

/// 构建带 headers 和 protocols 的 yawc RequestBuilder。
#[cfg(test)]
fn build_request_builder(
    config: &WsClientConfig,
) -> Result<yawc::HttpRequestBuilder, CatcherError> {
    let mut builder = yawc::HttpRequest::builder();
    for (name, value) in build_handshake_headers(config)?.iter() {
        builder = builder.header(name.clone(), value.clone());
    }
    Ok(builder)
}

/// 构建完整请求供配置测试使用；实际握手由 yawc 生成标准 Upgrade 请求。
#[cfg(test)]
fn build_request(url: &str, config: &WsClientConfig) -> Result<yawc::HttpRequest, CatcherError> {
    parse_ws_url(url)?;
    build_request_builder(config)?
        .uri(url)
        .body(())
        .map_err(|e| CatcherError::Internal(format!("invalid WS request: {e}")))
}

#[cfg(test)]
fn request_host_port(request: &yawc::HttpRequest) -> Result<(String, u16), CatcherError> {
    let uri = request.uri();
    let host = uri
        .host()
        .ok_or_else(|| CatcherError::InvalidConfig("WS URL missing host".into()))?
        .to_string();
    let port = uri
        .port_u16()
        .or_else(|| match uri.scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or_else(|| CatcherError::InvalidConfig("unsupported WS URL scheme".into()))?;
    Ok((host, port))
}

fn map_tls_version(version: TlsVersion) -> reqwest::tls::Version {
    match version {
        TlsVersion::Tls1_0 => reqwest::tls::Version::TLS_1_0,
        TlsVersion::Tls1_1 => reqwest::tls::Version::TLS_1_1,
        TlsVersion::Tls1_2 => reqwest::tls::Version::TLS_1_2,
        TlsVersion::Tls1_3 => reqwest::tls::Version::TLS_1_3,
    }
}

fn build_reqwest_tls_config(
    mut builder: reqwest::ClientBuilder,
    config: &TlsConfig,
) -> Result<reqwest::ClientBuilder, CatcherError> {
    if config
        .pin_sha256
        .as_ref()
        .is_some_and(|pins| !pins.is_empty())
    {
        return Err(CatcherError::InvalidConfig(
            "ws tls.pin_sha256 is not supported yet".into(),
        ));
    }

    if !config.reject_unauthorized {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if let Some(ref pem) = config.ca_cert_pem {
        let cert = reqwest::Certificate::from_pem(pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse WS CA cert PEM: {e}")))?;
        builder = builder.add_root_certificate(cert);
    }

    if let Some(ref path) = config.ca_cert_path {
        let pem_bytes = std::fs::read(path)
            .map_err(|e| CatcherError::TlsError(format!("read WS CA cert file {path}: {e}")))?;
        let cert = reqwest::Certificate::from_pem(&pem_bytes)
            .map_err(|e| CatcherError::TlsError(format!("parse WS CA cert file {path}: {e}")))?;
        builder = builder.add_root_certificate(cert);
    }

    if let (Some(ref cert_pem), Some(ref key_pem)) =
        (&config.client_cert_pem, &config.client_key_pem)
    {
        let identity_pem = format!("{cert_pem}\n{key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse WS client identity PEM: {e}")))?;
        builder = builder.identity(identity);
    }

    if let (Some(ref cert_path), Some(ref key_path)) =
        (&config.client_cert_path, &config.client_key_path)
    {
        let cert_pem = std::fs::read_to_string(cert_path)
            .map_err(|e| CatcherError::TlsError(format!("read WS client cert {cert_path}: {e}")))?;
        let key_pem = std::fs::read_to_string(key_path)
            .map_err(|e| CatcherError::TlsError(format!("read WS client key {key_path}: {e}")))?;
        let identity_pem = format!("{cert_pem}\n{key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes()).map_err(|e| {
            CatcherError::TlsError(format!("parse WS client identity from files: {e}"))
        })?;
        builder = builder.identity(identity);
    }

    if config.client_identity_pfx.is_some() {
        return Err(CatcherError::InvalidConfig(
            "ws tls.client_identity_pfx requires native-tls and is not supported by catcher-ws"
                .into(),
        ));
    }

    if let Some(min) = config.min_tls_version {
        builder = builder.min_tls_version(map_tls_version(min));
    }
    if let Some(max) = config.max_tls_version {
        builder = builder.max_tls_version(map_tls_version(max));
    }

    // SNI 主机名覆写：reqwest 0.13 的 `tls_sni()` 只接受布尔开关，无法覆写 SNI 主机名。
    // 与其静默忽略，不如显式报错，避免调用方误以为生效。详见 docs/issues/028。
    if config.tls_sni_override.is_some() {
        return Err(CatcherError::InvalidConfig(
            "ws tls.tls_sni_override is not supported: reqwest cannot override the SNI hostname"
                .into(),
        ));
    }

    Ok(builder)
}

pub(crate) fn build_reqwest_client(
    config: &WsClientConfig,
) -> Result<reqwest::Client, CatcherError> {
    let mut builder = reqwest::Client::builder();

    if config.handshake_timeout_ms > 0 {
        builder = builder.connect_timeout(Duration::from_millis(config.handshake_timeout_ms));
    }

    builder = build_reqwest_tls_config(builder, &config.tls)?;

    // 仅在显式 Catcher DNS 模式下注入自定义 resolver。与 HTTP 路径一致，正确性依赖
    // reqwest 的内部行为（issue #031）：走代理时 reqwest 不调用此 resolver，目标域名
    // 交给代理远端解析。护栏为 proxy_dns_behavior_test（随 cargo test --workspace 运行）。
    // **升级 reqwest 必须重跑该测试。**
    if let Some(ref dns_config) = config.dns {
        if dns_config.use_catcher_resolver() {
            let resolver = build_reqwest_resolver(dns_config)?;
            builder = builder.dns_resolver(resolver);
        }
    }

    if let Some(ref proxy_config) = config.proxy {
        let proxy_url = proxy_config.transport_url();
        let mut proxy = reqwest::Proxy::all(proxy_url.as_ref())
            .map_err(|e| CatcherError::InvalidConfig(format!("invalid WS proxy URL: {e}")))?;
        if let Some(ref auth) = proxy_config.auth {
            proxy = proxy.basic_auth(&auth.username, &auth.password);
        }
        if !proxy_config.no_proxy.is_empty() {
            let no_proxy = reqwest::NoProxy::from_string(&proxy_config.no_proxy.join(","));
            proxy = proxy.no_proxy(no_proxy);
        }
        builder = builder.proxy(proxy);
    }

    let headers = build_handshake_headers(config)?;
    if !headers.is_empty() {
        builder = builder.default_headers(headers);
    }

    builder
        .build()
        .map_err(|e| CatcherError::Internal(format!("WS reqwest build error: {e}")))
}

async fn connect_with_reqwest(
    url: Url,
    config: &WsClientConfig,
    ws_config: yawc::Options,
    client: &reqwest::Client,
) -> Result<WsStream, CatcherError> {
    let attempt = yawc::WebSocket::reqwest(url, client.clone(), ws_config);
    if config.handshake_timeout_ms > 0 {
        match tokio::time::timeout(Duration::from_millis(config.handshake_timeout_ms), attempt)
            .await
        {
            Ok(result) => result,
            Err(_) => {
                return Err(CatcherError::WsHandshakeTimeout(
                    config.handshake_timeout_ms,
                ))
            }
        }
    } else {
        attempt.await
    }
    .map_err(|e| CatcherError::Internal(format!("ws handshake failed: {e}")))
}

/// 底层 WebSocket 连接 — 处理 headers/protocols/handshake_timeout/deflate。
/// 返回 (stream, latency_ms)。
pub(crate) async fn connect_stream_with_client(
    url: &str,
    config: &WsClientConfig,
    client: &reqwest::Client,
) -> Result<(WsStream, u64), CatcherError> {
    let url = parse_ws_url(url)?;
    let ws_config = build_ws_config(config);

    let start = Instant::now();
    let stream = connect_with_reqwest(url, config, ws_config, client).await?;

    let latency_ms = start.elapsed().as_millis() as u64;
    Ok((stream, latency_ms))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SendStatus {
    Sent,
    Failed,
    TimedOut,
}

/// 带超时的帧发送。
///
/// 网络切换后连接变成半开时，发送会阻塞到 TCP 重传超时（分钟级），
/// 期间事件循环无法处理任何命令（包括 network_changed / close）。
/// 超时判定断线，让循环尽快进入重连流程。`timeout_ms == 0` 不限制。
async fn send_frame_with_timeout<S>(writer: &mut S, frame: Frame, timeout_ms: u64) -> SendStatus
where
    S: futures_util::Sink<Frame> + Unpin,
{
    if timeout_ms == 0 {
        return match writer.send(frame).await {
            Ok(()) => SendStatus::Sent,
            Err(_) => SendStatus::Failed,
        };
    }
    match tokio::time::timeout(Duration::from_millis(timeout_ms), writer.send(frame)).await {
        Ok(Ok(())) => SendStatus::Sent,
        Ok(Err(_)) => SendStatus::Failed,
        Err(_) => SendStatus::TimedOut,
    }
}

/// 重连成功后重放缓存的发送命令（Close/NetworkChanged 已在缓存时被过滤）。
async fn replay_buffered_commands<S>(
    stream: &mut S,
    commands: &[WsCommand],
    config: &WsClientConfig,
) -> SendStatus
where
    S: futures_util::Sink<Frame> + Unpin,
{
    for cmd in commands {
        match cmd {
            WsCommand::Text(t) => {
                if let Ok(msg) = encode_text_message(t, config) {
                    let status = send_frame_with_timeout(stream, msg, config.send_timeout_ms).await;
                    if status != SendStatus::Sent {
                        return status;
                    }
                }
            }
            WsCommand::Binary(d) => {
                if let Ok(msg) = encode_binary_message(d, config) {
                    let status = send_frame_with_timeout(stream, msg, config.send_timeout_ms).await;
                    if status != SendStatus::Sent {
                        return status;
                    }
                }
            }
            WsCommand::Close { .. } | WsCommand::NetworkChanged => {}
        }
    }
    SendStatus::Sent
}

fn replay_disconnect_reason(status: SendStatus) -> Option<&'static str> {
    match status {
        SendStatus::Sent => None,
        SendStatus::Failed => Some("replay send failed"),
        SendStatus::TimedOut => Some("replay send timeout"),
    }
}

/// 全量重连：多端点配置时重新竞速，单端点直接连接。
/// 网络环境变化后调用 — 新网络下最优端点可能与之前不同。
async fn connect_any_endpoint(
    config: &WsClientConfig,
    client: &reqwest::Client,
) -> Result<(String, WsStream, u64), CatcherError> {
    if config.urls.len() > 1 || config.race_count > 1 {
        // race_count 只限制初始连接的竞速宽度（EndpointRacer 内部 take(race_count)）。
        // 网络变化后旧的最优端点可能已不可达，必须让每个端点都有机会。
        let race_count = (config.urls.len() as u32).max(config.race_count);
        let racer = EndpointRacer::new(config.urls.clone(), race_count);
        racer.race(config, client).await
    } else {
        let url = config
            .urls
            .first()
            .cloned()
            .ok_or_else(|| CatcherError::InvalidConfig("no WS URLs configured".into()))?;
        let (stream, lat) = connect_stream_with_client(&url, config, client).await?;
        Ok((url, stream, lat))
    }
}

// ── 高级连接 ──

impl WsTransport {
    /// 建立 WebSocket 连接，集成全部 config 功能：
    ///
    /// - `urls` + `race_count`: 多端点竞速
    /// - `headers`: 自定义握手 headers
    /// - `protocols`: WebSocket 子协议
    /// - `per_message_deflate`: RFC 7692 permessage-deflate 压缩
    /// - `handshake_timeout_ms`: 握手超时
    /// - `reconnect`: 自动重连 + 指数退避
    /// - `heartbeat`: 定时 ping + 超时检测
    ///
    /// 返回 (WsHandle, 事件接收器)。
    /// WsHandle 在重连期间保持有效，发送的消息会在重连后自动发送。
    pub async fn connect(
        config: &WsClientConfig,
    ) -> Result<(WsHandle, mpsc::UnboundedReceiver<WsEvent>), CatcherError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

        if config.urls.is_empty() {
            return Err(CatcherError::InvalidConfig("no WS URLs configured".into()));
        }
        let reqwest_client = build_reqwest_client(config)?;

        // 初始连接 — 多端点竞速或单 URL
        let (connected_url, initial_stream, latency_ms) = if config.urls.len() > 1
            || config.race_count > 1
        {
            let racer = EndpointRacer::new(config.urls.clone(), config.race_count);
            let (url, stream, lat) = racer.race(config, &reqwest_client).await?;
            (url, stream, lat)
        } else {
            let url = config.urls.first().unwrap().clone();
            let (stream, lat) = connect_stream_with_client(&url, config, &reqwest_client).await?;
            (url, stream, lat)
        };

        let handle_url = connected_url.clone();
        let mgr_config = config.clone();

        // 启动连接管理器任务
        tokio::spawn(async move {
            connection_manager(
                connected_url,
                initial_stream,
                latency_ms,
                &mgr_config,
                reqwest_client,
                event_tx,
                cmd_rx,
            )
            .await;
        });

        Ok((
            WsHandle {
                url: handle_url,
                cmd_tx,
            },
            event_rx,
        ))
    }
}

// ── 连接管理器 ──

async fn connection_manager(
    initial_url: String,
    initial_stream: WsStream,
    initial_latency_ms: u64,
    config: &WsClientConfig,
    mut reqwest_client: reqwest::Client,
    event_tx: mpsc::UnboundedSender<WsEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<WsCommand>,
) {
    let mut current_url = initial_url;
    let mut stream_opt = Some(initial_stream);
    let mut reconnect_mgr = config
        .reconnect
        .as_ref()
        .map(|c| ReconnectManager::new(c.clone()));

    let mut first_latency = initial_latency_ms;

    loop {
        // 发送 Connected 事件
        {
            let lat = first_latency;
            let _ = event_tx.send(WsEvent::Connected {
                url: current_url.clone(),
                latency_ms: lat,
            });
        }
        // 设置心跳
        let mut hb_state = config.heartbeat.as_ref().map(|hb_config| HeartbeatState {
            mgr: HeartbeatManager::new(hb_config.clone()),
            waiting_for_pong: false,
            ping_sent_at: None,
        });

        // 心跳定时器 — 使用 sleep_until 实现动态间隔，每次 ping 前查询 HeartbeatManager::interval_ms()
        let ping_sleep = if let Some(ref mut state) = hb_state {
            let initial_ms = state.mgr.interval_ms();
            tokio::time::sleep(Duration::from_millis(initial_ms))
        } else {
            // 无心跳配置，创建一个永远不会触发的 sleep
            tokio::time::sleep(Duration::MAX)
        };
        tokio::pin!(ping_sleep);

        // 拆分读写
        let (mut writer, mut reader) = stream_opt
            .take()
            .expect("stream_opt must be Some at start of loop")
            .split();

        // ── Select loop ──
        let outcome = loop {
            tokio::select! {
                biased;

                // 用户命令
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(WsCommand::Text(t)) => {
                            let msg = match encode_text_message(&t, config) {
                                Ok(msg) => msg,
                                Err(e) => {
                                    let _ = event_tx.send(WsEvent::Error {
                                        message: e.to_string(),
                                    });
                                    continue;
                                }
                            };
                            match send_frame_with_timeout(&mut writer, msg, config.send_timeout_ms).await {
                                SendStatus::Sent => {}
                                SendStatus::Failed => break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send failed".into(),
                                },
                                SendStatus::TimedOut => break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send timeout".into(),
                                },
                            }
                        }
                        Some(WsCommand::Binary(d)) => {
                            let msg = match encode_binary_message(&d, config) {
                                Ok(msg) => msg,
                                Err(e) => {
                                    let _ = event_tx.send(WsEvent::Error {
                                        message: e.to_string(),
                                    });
                                    continue;
                                }
                            };
                            match send_frame_with_timeout(&mut writer, msg, config.send_timeout_ms).await {
                                SendStatus::Sent => {}
                                SendStatus::Failed => break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send failed".into(),
                                },
                                SendStatus::TimedOut => break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send timeout".into(),
                                },
                            }
                        }
                        Some(WsCommand::Close { code, reason }) => {
                            let msg = Frame::close(yawc::close::CloseCode::from(code), reason);
                            let _ = send_frame_with_timeout(&mut writer, msg, config.send_timeout_ms).await;
                            if config.send_timeout_ms == 0 {
                                let _ = writer.close().await;
                            } else {
                                let _ = tokio::time::timeout(
                                    Duration::from_millis(config.send_timeout_ms),
                                    writer.close(),
                                )
                                .await;
                            }
                            break LoopOutcome::CleanClose;
                        }
                        Some(WsCommand::NetworkChanged) => {
                            // 旧网络上的连接大概率已半开，直接丢弃，不发 Close 帧
                            // （在死网络上发送会阻塞到超时）
                            break LoopOutcome::NetworkChanged;
                        }
                        None => break LoopOutcome::CleanClose,
                    }
                }

                // 心跳 tick — 动态间隔
                _ = &mut ping_sleep, if hb_state.is_some() => {
                    if let Some(ref mut state) = hb_state {
                        if state.waiting_for_pong {
                            // pong_timeout_ms 快速超时检测
                            if state.mgr.is_timed_out() {
                                break LoopOutcome::HeartbeatTimeout;
                            }
                            state.mgr.on_missed_pong();
                            if state.mgr.is_missed_pongs_exceeded() {
                                break LoopOutcome::HeartbeatTimeout;
                            }
                        }
                        state.waiting_for_pong = true;
                        state.ping_sent_at = Some(Instant::now());
                        if matches!(
                            send_frame_with_timeout(
                                &mut writer,
                                Frame::ping(Vec::new()),
                                config.send_timeout_ms,
                            )
                            .await,
                            SendStatus::TimedOut
                        ) {
                            // 半开连接：ping 都发不出去，不必再等 pong 超时
                            break LoopOutcome::Disconnected {
                                code: 1006,
                                reason: "ping send timeout".into(),
                            };
                        }
                        // 根据自适应间隔重设下一次 ping 时间
                        let next_ms = state.mgr.interval_ms();
                        ping_sleep.as_mut().reset(
                            tokio::time::Instant::now() + Duration::from_millis(next_ms),
                        );
                    }
                }

                    // 收到的消息
                    msg = reader.next() => {
                        match msg {
                            Some(frame) if frame.opcode() == OpCode::Text => {
                                let _ = event_tx.send(WsEvent::Message {
                                    data: frame.payload().to_vec(),
                                    is_binary: false,
                                });
                            }
                            Some(frame) if frame.opcode() == OpCode::Binary => {
                                match decode_binary_message(frame.payload(), config) {
                                    Ok((data, is_binary)) => {
                                        let _ = event_tx.send(WsEvent::Message { data, is_binary });
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(WsEvent::Error {
                                            message: e.to_string(),
                                        });
                                    }
                                }
                            }
                            Some(frame) if frame.opcode() == OpCode::Ping => {}
                            Some(frame) if frame.opcode() == OpCode::Pong => {
                                if let Some(ref mut state) = hb_state {
                                    let rtt_ms = state.ping_sent_at
                                        .take()
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                state.mgr.on_pong(rtt_ms);
                                state.waiting_for_pong = false;
                                    let _ = event_tx.send(WsEvent::HeartbeatRtt { rtt_ms });
                                }
                            }
                            Some(frame) if frame.opcode() == OpCode::Close => {
                                let code = frame.close_code().map(u16::from).unwrap_or(1006);
                                let reason = frame
                                    .close_reason()
                                    .ok()
                                    .flatten()
                                    .unwrap_or("abnormal")
                                    .to_string();
                                // RFC 6455 §5.5.1: 收到 Close 帧必须回一个 Close 帧
                                let _ = send_frame_with_timeout(
                                    &mut writer,
                                    Frame::close(yawc::close::CloseCode::from(code), &reason),
                                    config.send_timeout_ms,
                                )
                                .await;
                                break LoopOutcome::Disconnected { code, reason };
                            }
                            Some(_) => {}
                            None => {
                                break LoopOutcome::Disconnected {
                                    code: 1006,
                                reason: "stream ended".into(),
                            };
                        }
                    }
                }
            }
        };

        // ── 处理 select loop 结果 ──
        let network_changed = matches!(outcome, LoopOutcome::NetworkChanged);
        match outcome {
            LoopOutcome::CleanClose => break,

            LoopOutcome::HeartbeatTimeout => {
                let _ = event_tx.send(WsEvent::Disconnected {
                    code: 1006,
                    reason: "heartbeat timeout".into(),
                });
            }
            LoopOutcome::NetworkChanged => {
                let _ = event_tx.send(WsEvent::Disconnected {
                    code: 1006,
                    reason: "network changed".into(),
                });
            }
            LoopOutcome::Disconnected { code, reason } => {
                let _ = event_tx.send(WsEvent::Disconnected { code, reason });
            }
        }

        // 跨重连阶段缓存的待重放命令（Text/Binary）
        let mut buffered_commands: Vec<WsCommand> = Vec::new();

        // ── 网络变化：清 DNS 缓存 + 跳过退避立即重连（即使未配置 reconnect）──
        if network_changed {
            // 合并 OS 回调风暴：一次网络切换常触发多个连续回调（如 Android 的
            // onAvailable / onCapabilitiesChanged），排空积压的 NetworkChanged
            // 只重连一次；其余命令缓存等待重放，Close 中止
            let mut close_requested = false;
            loop {
                match cmd_rx.try_recv() {
                    Ok(WsCommand::NetworkChanged) => {}
                    Ok(WsCommand::Close { .. }) => {
                        close_requested = true;
                        break;
                    }
                    Ok(cmd) => buffered_commands.push(cmd),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        close_requested = true;
                        break;
                    }
                }
            }
            if close_requested {
                break;
            }

            // 重建 reqwest client：丢弃旧连接池，并重新应用 proxy/TLS/DNS 配置。
            // 显式启用 Catcher DNS 时，也会随新 client 重建 resolver。
            match build_reqwest_client(config) {
                Ok(client) => reqwest_client = client,
                Err(e) => {
                    let _ = event_tx.send(WsEvent::Error {
                        message: format!("network changed client rebuild failed: {e}"),
                    });
                }
            }
            if let Some(ref mut mgr) = reconnect_mgr {
                mgr.reset();
            }
            let _ = event_tx.send(WsEvent::Reconnecting {
                attempt: 0,
                delay_ms: 0,
            });
            match connect_any_endpoint(config, &reqwest_client).await {
                Ok((url, mut stream, lat)) => {
                    let replay_status =
                        replay_buffered_commands(&mut stream, &buffered_commands, config).await;
                    if let Some(reason) = replay_disconnect_reason(replay_status) {
                        let _ = event_tx.send(WsEvent::Disconnected {
                            code: 1006,
                            reason: reason.into(),
                        });
                    } else {
                        current_url = url;
                        stream_opt = Some(stream);
                        first_latency = lat;
                        if let Some(ref mut mgr) = reconnect_mgr {
                            mgr.on_connected();
                        }
                        continue;
                    }
                }
                Err(e) => {
                    // 无退避重连兜底时这是终态：补一个 Error 事件，
                    // 避免消费者停留在 Reconnecting 状态等待一个不会来的结果
                    if reconnect_mgr.is_none() {
                        let _ = event_tx.send(WsEvent::Error {
                            message: format!("network changed reconnect failed: {e}"),
                        });
                    }
                    // 落入常规退避重连流程
                }
            }
        }

        // ── 尝试重连 ──
        if let Some(ref mut mgr) = reconnect_mgr {
            let mut reconnected = false;
            let mut reconnect_latency_ms: u64 = 0;

            // 网络变化触发的重连：重新竞速全部端点（而非固守 current_url）。
            // 立即重连失败落入本循环时必须继承这个意图，否则多端点配置下
            // 会固守旧网络的获胜端点直到耗尽重试
            let mut race_endpoints = network_changed;

            while let Some(delay) = mgr.on_disconnect() {
                let attempt = mgr.attempt();
                let _ = event_tx.send(WsEvent::Reconnecting {
                    attempt,
                    delay_ms: delay,
                });

                // 退避等待 — 期间监听命令：Close 中止；NetworkChanged 打断
                // 剩余延迟、重置退避计数立即重试；其余缓存等待重放
                let mut abort = false;
                let mut skip_backoff = false;
                {
                    let backoff_sleep = tokio::time::sleep(Duration::from_millis(delay));
                    tokio::pin!(backoff_sleep);
                    loop {
                        tokio::select! {
                            _ = &mut backoff_sleep => break,
                            cmd = cmd_rx.recv() => match cmd {
                                Some(WsCommand::Close { .. }) | None => {
                                    abort = true;
                                    break;
                                }
                                Some(WsCommand::NetworkChanged) => {
                                    skip_backoff = true;
                                    break;
                                }
                                Some(cmd) => buffered_commands.push(cmd),
                            }
                        }
                    }
                }

                // 排空剩余待处理命令
                if !abort {
                    loop {
                        match cmd_rx.try_recv() {
                            Ok(WsCommand::Close { .. }) => {
                                abort = true;
                                break;
                            }
                            Ok(WsCommand::NetworkChanged) => skip_backoff = true,
                            Ok(cmd) => buffered_commands.push(cmd),
                            Err(mpsc::error::TryRecvError::Empty) => break,
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                abort = true;
                                break;
                            }
                        }
                    }
                }
                if abort {
                    break;
                }
                if skip_backoff {
                    match build_reqwest_client(config) {
                        Ok(client) => reqwest_client = client,
                        Err(e) => {
                            let _ = event_tx.send(WsEvent::Error {
                                message: format!("network changed client rebuild failed: {e}"),
                            });
                        }
                    }
                    mgr.reset();
                    race_endpoints = true;
                }

                let connect_result = if race_endpoints {
                    connect_any_endpoint(config, &reqwest_client)
                        .await
                        .map(|(url, stream, lat)| {
                            current_url = url;
                            (stream, lat)
                        })
                } else {
                    connect_stream_with_client(&current_url, config, &reqwest_client).await
                };

                match connect_result {
                    Ok((mut stream, lat)) => {
                        let replay_status =
                            replay_buffered_commands(&mut stream, &buffered_commands, config).await;
                        if let Some(reason) = replay_disconnect_reason(replay_status) {
                            let _ = event_tx.send(WsEvent::Disconnected {
                                code: 1006,
                                reason: reason.into(),
                            });
                            continue;
                        }
                        stream_opt = Some(stream);
                        reconnect_latency_ms = lat;
                        mgr.on_connected();
                        reconnected = true;
                        break;
                    }
                    Err(_) => continue,
                }
            }

            if reconnected {
                // 更新延迟供下一轮 Connected 事件使用
                first_latency = reconnect_latency_ms;
                continue; // 回到外层循环 → 新的 select loop
            }
        }

        // 无重连配置或耗尽 — 退出
        break;
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 build_request 成功构建带 headers 和 protocols 的请求
    #[test]
    fn build_request_with_headers_and_protocols() {
        let config = WsClientConfig {
            urls: vec!["wss://example.com/ws".into()],
            headers: vec![("Authorization".into(), "Bearer token".into())]
                .into_iter()
                .collect(),
            protocols: vec!["v1".into(), "v2".into()],
            ..Default::default()
        };

        let req = build_request("wss://example.com/ws", &config).unwrap();
        assert_eq!(
            req.headers()
                .get("Authorization")
                .map(|v| v.to_str().unwrap()),
            Some("Bearer token")
        );
        assert_eq!(
            req.headers()
                .get("Sec-WebSocket-Protocol")
                .map(|v| v.to_str().unwrap()),
            Some("v1, v2")
        );
    }

    /// 验证 build_request 空 headers/protocols 不报错
    #[test]
    fn build_request_minimal() {
        let config = WsClientConfig {
            urls: vec!["ws://localhost".into()],
            ..Default::default()
        };
        let req = build_request("ws://localhost", &config).unwrap();
        assert!(req.headers().get("Authorization").is_none());
        assert!(req.headers().get("Sec-WebSocket-Protocol").is_none());
    }

    /// 验证 build_request 无效 URL 报错
    #[test]
    fn build_request_invalid_url() {
        let config = WsClientConfig::default();
        assert!(build_request("not a url :///", &config).is_err());
    }

    /// 验证 WS URL 能正确提取 host 和默认端口
    #[test]
    fn request_host_port_defaults() {
        let config = WsClientConfig::default();
        let req = build_request("wss://example.com/ws", &config).unwrap();
        let (host, port) = request_host_port(&req).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 443);

        let req = build_request("ws://example.com:8080/ws", &config).unwrap();
        let (host, port) = request_host_port(&req).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 8080);
    }

    /// 验证 WsClientConfig 的 headers 序列化/反序列化
    #[test]
    fn config_headers_roundtrip() {
        let json = r#"{
            "urls": ["wss://example.com"],
            "headers": {"X-Custom": "value", "Authorization": "Bearer abc"},
            "protocols": ["graphql-ws"],
            "msgpack": true,
            "dns": {
                "mode": "catcher",
                "cache_size": 128,
                "cache_ttl_secs": 30,
                "host_mapping": {"example.com": "127.0.0.1"}
            }
        }"#;
        let config: WsClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.headers.len(), 2);
        assert_eq!(config.protocols, vec!["graphql-ws"]);
        assert!(config.msgpack);
        let dns = config.dns.unwrap();
        assert_eq!(dns.cache_size, 128);
        assert_eq!(dns.cache_ttl_secs, 30);
        assert_eq!(
            dns.host_mapping.get("example.com"),
            Some(&"127.0.0.1".to_string())
        );
    }

    /// 验证显式配置 DNS 时，WS 连接使用 host_mapping。
    #[tokio::test]
    async fn connect_stream_uses_dns_host_mapping() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://ws.test:{port}")],
            dns: Some(DnsConfig {
                mode: DnsMode::Catcher,
                host_mapping: [("ws.test".to_string(), "127.0.0.1".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            }),
            handshake_timeout_ms: 1_000,
            ..Default::default()
        };
        let client = build_reqwest_client(&config).unwrap();

        let result = connect_stream_with_client(&config.urls[0], &config, &client).await;
        assert!(result.is_ok());
        drop(result);
        let _ = server.await.unwrap();
    }

    /// 验证未配置 DNS 时，默认路径仍能连接本机 IP。
    #[tokio::test]
    async fn connect_stream_without_dns_config_still_connects_ip() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://127.0.0.1:{port}")],
            handshake_timeout_ms: 1_000,
            ..Default::default()
        };
        let client = build_reqwest_client(&config).unwrap();

        let result = connect_stream_with_client(&config.urls[0], &config, &client).await;
        assert!(result.is_ok());
        drop(result);
        let _ = server.await.unwrap();
    }

    /// 永远不就绪的 Sink — 模拟半开连接上发送阻塞
    struct PendingSink;

    impl futures_util::Sink<Frame> for PendingSink {
        type Error = std::io::Error;

        fn poll_ready(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Pending
        }

        fn start_send(self: std::pin::Pin<&mut Self>, _item: Frame) -> Result<(), Self::Error> {
            Ok(())
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Pending
        }

        fn poll_close(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), Self::Error>> {
            std::task::Poll::Pending
        }
    }

    /// 半开连接上发送阻塞时应在 send_timeout_ms 内返回 TimedOut，
    /// 而不是阻塞整个事件循环
    #[tokio::test]
    async fn send_frame_times_out_on_stuck_sink() {
        let mut sink = PendingSink;
        let start = Instant::now();
        let status = send_frame_with_timeout(&mut sink, Frame::text(b"x".to_vec()), 50).await;
        assert!(matches!(status, SendStatus::TimedOut));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 正常 Sink 上发送应返回 Sent（含 timeout=0 禁用路径）
    #[tokio::test]
    async fn send_frame_succeeds_on_ready_sink() {
        let mut sink = futures_util::sink::drain();
        let status = send_frame_with_timeout(&mut sink, Frame::text(b"a".to_vec()), 1_000).await;
        assert!(matches!(status, SendStatus::Sent));

        let status = send_frame_with_timeout(&mut sink, Frame::text(b"b".to_vec()), 0).await;
        assert!(matches!(status, SendStatus::Sent));
    }

    /// 重放缓存消息时也必须遵守 send_timeout_ms。
    #[tokio::test]
    async fn replay_buffered_commands_times_out_on_stuck_sink() {
        let mut sink = PendingSink;
        let config = WsClientConfig {
            send_timeout_ms: 50,
            ..Default::default()
        };
        let commands = vec![WsCommand::Text("hello".to_string())];

        let start = Instant::now();
        let status = replay_buffered_commands(&mut sink, &commands, &config).await;

        assert!(matches!(status, SendStatus::TimedOut));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// network_changed() 应立即断开并重连，事件序列：
    /// Connected → Disconnected("network changed") → Reconnecting(0) → Connected
    #[tokio::test]
    async fn network_changed_triggers_immediate_reconnect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        // 服务器接受两次连接（初始 + 网络变化后的重连）
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                // 保持连接存活，直到对端断开
                tokio::spawn(async move {
                    let mut ws = ws;
                    while let Some(Ok(_)) = ws.next().await {}
                });
            }
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://127.0.0.1:{port}")],
            handshake_timeout_ms: 1_000,
            reconnect: Some(ReconnectConfig {
                initial_delay_ms: 60_000, // 故意设很大：验证重连没有走退避延迟
                max_delay_ms: 60_000,
                backoff_multiplier: 2.0,
                max_attempts: 5,
            }),
            ..Default::default()
        };

        let (handle, mut rx) = WsTransport::connect(&config).await.unwrap();

        // 初始 Connected
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first, WsEvent::Connected { .. }));

        handle.network_changed().unwrap();

        // Disconnected("network changed")
        let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        match ev {
            WsEvent::Disconnected { reason, .. } => assert_eq!(reason, "network changed"),
            other => panic!("expected Disconnected, got {other:?}"),
        }

        // Reconnecting(attempt=0, delay=0) — 立即重连，无退避
        let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        match ev {
            WsEvent::Reconnecting { attempt, delay_ms } => {
                assert_eq!(attempt, 0);
                assert_eq!(delay_ms, 0);
            }
            other => panic!("expected Reconnecting, got {other:?}"),
        }

        // 2 秒内重新 Connected（initial_delay 是 60 秒，证明跳过了退避）
        let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(ev, WsEvent::Connected { .. }));

        let _ = handle.close(1000, "done");
        let _ = server.await;
    }

    /// network_changed() 应打断正在进行的退避等待并立即重连，
    /// 且退避期间发送的消息在重连后被重放
    #[tokio::test]
    async fn network_changed_interrupts_backoff_and_replays_buffered() {
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // 第一段：接受一次连接后立刻断开 → 客户端进入 60s 退避
        let first = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            drop(ws); // 直接断开
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://{addr}")],
            handshake_timeout_ms: 1_000,
            reconnect: Some(ReconnectConfig {
                initial_delay_ms: 60_000, // 不打断的话测试必然超时
                max_delay_ms: 60_000,
                backoff_multiplier: 2.0,
                max_attempts: 5,
            }),
            ..Default::default()
        };

        let (handle, mut rx) = WsTransport::connect(&config).await.unwrap();
        first.await.unwrap();

        // 等待进入退避（Connected → Disconnected → Reconnecting(1, 60000)）
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap();
            if let WsEvent::Reconnecting { attempt, delay_ms } = ev {
                assert_eq!(attempt, 1);
                assert_eq!(delay_ms, 60_000);
                break;
            }
        }

        // 退避期间发送消息（应被缓存）并重新拉起服务器
        handle.send_text("buffered during backoff").unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let second = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            // 读取重放的消息
            loop {
                match ws.next().await {
                    Some(Ok(Message::Text(t))) => return t.to_string(),
                    Some(Ok(_)) => continue,
                    other => panic!("expected replayed text, got {other:?}"),
                }
            }
        });

        handle.network_changed().unwrap();

        // 2 秒内重连成功（60s 退避被打断）
        loop {
            let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .expect("reconnect should not wait out the 60s backoff")
                .unwrap();
            if matches!(ev, WsEvent::Connected { .. }) {
                break;
            }
        }

        // 缓存的消息被重放到新连接
        let replayed = tokio::time::timeout(Duration::from_secs(2), second)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replayed, "buffered during backoff");

        let _ = handle.close(1000, "done");
    }

    /// 多端点配置下 network_changed() 应重新竞速全部端点：
    /// 原端点不可达时切换到其他端点（即使 race_count=1）
    #[tokio::test]
    async fn network_changed_re_races_all_endpoints() {
        let listener_a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_a = listener_a.local_addr().unwrap();
        let listener_b = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr_b = listener_b.local_addr().unwrap();

        // A：接受初始连接后保持
        let server_a = tokio::spawn(async move {
            let (stream, _) = listener_a.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            tokio::spawn(async move {
                let mut ws = ws;
                while let Some(Ok(_)) = ws.next().await {}
            });
            listener_a // 持有 listener，稍后 drop 模拟端点不可达
        });

        // B：等待网络变化后的重连
        let server_b = tokio::spawn(async move {
            let (stream, _) = listener_b.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            tokio::spawn(async move {
                let mut ws = ws;
                while let Some(Ok(_)) = ws.next().await {}
            });
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://{addr_a}"), format!("ws://{addr_b}")],
            handshake_timeout_ms: 1_000,
            race_count: 1, // 默认值：初始竞速只取第一个端点
            ..Default::default()
        };

        let (handle, mut rx) = WsTransport::connect(&config).await.unwrap();
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        match first {
            WsEvent::Connected { ref url, .. } => assert!(url.contains(&addr_a.to_string())),
            other => panic!("expected Connected, got {other:?}"),
        }

        // A 端点下线，通知网络变化 → 重竞速应连上 B
        let listener_a = server_a.await.unwrap();
        drop(listener_a);
        handle.network_changed().unwrap();

        loop {
            let ev = tokio::time::timeout(Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .unwrap();
            if let WsEvent::Connected { url, .. } = ev {
                assert!(
                    url.contains(&addr_b.to_string()),
                    "should fail over to endpoint B, got {url}"
                );
                break;
            }
        }

        let _ = handle.close(1000, "done");
        let _ = server_b.await;
    }

    /// 未配置 reconnect 时 network_changed() 也应执行一次立即重连
    #[tokio::test]
    async fn network_changed_reconnects_without_reconnect_config() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                tokio::spawn(async move {
                    let mut ws = ws;
                    while let Some(Ok(_)) = ws.next().await {}
                });
            }
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://127.0.0.1:{port}")],
            handshake_timeout_ms: 1_000,
            reconnect: None,
            ..Default::default()
        };

        let (handle, mut rx) = WsTransport::connect(&config).await.unwrap();
        let first = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first, WsEvent::Connected { .. }));

        handle.network_changed().unwrap();

        // Disconnected → Reconnecting → Connected
        let mut got_connected = false;
        for _ in 0..3 {
            let ev = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap()
                .unwrap();
            if matches!(ev, WsEvent::Connected { .. }) {
                got_connected = true;
                break;
            }
        }
        assert!(
            got_connected,
            "should reconnect even without reconnect config"
        );

        let _ = handle.close(1000, "done");
        let _ = server.await;
    }
    /// 验证 build_request 自动声明应用层压缩能力
    #[test]
    fn build_request_adds_application_compression_headers() {
        let config = WsClientConfig {
            urls: vec!["wss://example.com/ws".into()],
            per_message_deflate: false,
            application_compression: Some(ApplicationCompressionConfig {
                enabled: true,
                algorithm: ApplicationCompressionAlgorithm::Zstd,
                threshold_bytes: 2048,
            }),
            ..Default::default()
        };

        let req = build_request("wss://example.com/ws", &config).unwrap();

        assert_eq!(
            req.headers()
                .get("X-Catcher-Application-Compression")
                .map(|v| v.to_str().unwrap()),
            Some("zstd")
        );
        assert_eq!(
            req.headers()
                .get("X-Catcher-Application-Compression-Format")
                .map(|v| v.to_str().unwrap()),
            Some("CATCHER-CMP-1")
        );
        assert_eq!(
            req.headers()
                .get("X-Catcher-Application-Compression-Threshold")
                .map(|v| v.to_str().unwrap()),
            Some("2048")
        );
    }

    /// permessage-deflate 优先于应用层压缩，避免双重压缩。
    #[test]
    fn build_request_omits_application_compression_when_permessage_deflate_enabled() {
        let config = WsClientConfig {
            urls: vec!["wss://example.com/ws".into()],
            per_message_deflate: true,
            application_compression: Some(ApplicationCompressionConfig::default()),
            ..Default::default()
        };

        let req = build_request("wss://example.com/ws", &config).unwrap();

        assert!(req
            .headers()
            .get("X-Catcher-Application-Compression")
            .is_none());
    }

    #[test]
    fn config_accepts_proxy_tls_and_dns() {
        let json = r#"{
            "urls": ["wss://example.com/ws"],
            "proxy": {
                "url": "socks5h://127.0.0.1:7890",
                "auth": {"username": "u", "password": "p"},
                "no_proxy": ["localhost"]
            },
            "tls": {"reject_unauthorized": false, "min_tls_version": "Tls1_2"},
            "dns": {"mode": "native"}
        }"#;

        let config: WsClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.proxy.as_ref().map(|proxy| proxy.url.as_str()),
            Some("socks5h://127.0.0.1:7890")
        );
        assert!(!config.tls.reject_unauthorized);
        assert_eq!(config.tls.min_tls_version, Some(TlsVersion::Tls1_2));
        assert!(!config.dns.as_ref().unwrap().use_catcher_resolver());
    }
}
