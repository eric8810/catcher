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

    if config.tls_sni_override.is_some() {
        builder = builder.tls_sni(true);
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

    if let Some(ref dns_config) = config.dns {
        if dns_config.use_catcher_resolver() {
            let resolver = build_reqwest_resolver(dns_config)?;
            builder = builder.dns_resolver(resolver);
        }
    }

    if let Some(ref proxy_config) = config.proxy {
        let mut proxy = reqwest::Proxy::all(&proxy_config.url)
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
    reqwest_client: reqwest::Client,
    event_tx: mpsc::UnboundedSender<WsEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<WsCommand>,
) {
    let current_url = initial_url;
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
                            if writer.send(msg).await.is_err() {
                                break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send failed".into(),
                                };
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
                            if writer.send(msg).await.is_err() {
                                break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send failed".into(),
                                };
                            }
                        }
                        Some(WsCommand::Close { code, reason }) => {
                            let msg = Frame::close(yawc::close::CloseCode::from(code), reason);
                            let _ = writer.send(msg).await;
                            let _ = writer.close().await;
                            break LoopOutcome::CleanClose;
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
                        let _ = writer.send(Frame::ping(Vec::new())).await;
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
                                let _ = writer
                                    .send(Frame::close(
                                        yawc::close::CloseCode::from(code),
                                        &reason,
                                    ))
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
        match outcome {
            LoopOutcome::CleanClose => break,

            LoopOutcome::HeartbeatTimeout => {
                let _ = event_tx.send(WsEvent::Disconnected {
                    code: 1006,
                    reason: "heartbeat timeout".into(),
                });
            }
            LoopOutcome::Disconnected { code, reason } => {
                let _ = event_tx.send(WsEvent::Disconnected { code, reason });
            }
        }

        // ── 尝试重连 ──
        if let Some(ref mut mgr) = reconnect_mgr {
            let mut reconnected = false;
            let mut buffered_commands: Vec<WsCommand> = Vec::new();
            let mut reconnect_latency_ms: u64 = 0;

            while let Some(delay) = mgr.on_disconnect() {
                let attempt = mgr.attempt();
                let _ = event_tx.send(WsEvent::Reconnecting {
                    attempt,
                    delay_ms: delay,
                });

                tokio::time::sleep(Duration::from_millis(delay)).await;

                // 排空待处理命令：Close 中止重连，其余缓存等待重放
                let mut abort = false;
                loop {
                    match cmd_rx.try_recv() {
                        Ok(WsCommand::Close { .. }) => {
                            abort = true;
                            break;
                        }
                        Ok(cmd) => buffered_commands.push(cmd),
                        Err(mpsc::error::TryRecvError::Empty) => break,
                        Err(mpsc::error::TryRecvError::Disconnected) => {
                            abort = true;
                            break;
                        }
                    }
                }
                if abort {
                    break;
                }

                match connect_stream_with_client(&current_url, config, &reqwest_client).await {
                    Ok((stream, lat)) => {
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
                // 重连后回放缓存的命令（Close 已被过滤，不会到达这里）
                if !buffered_commands.is_empty() {
                    if let Some(mut stream) = stream_opt.take() {
                        for cmd in buffered_commands {
                            match cmd {
                                WsCommand::Text(t) => {
                                    if let Ok(msg) = encode_text_message(&t, config) {
                                        let _ = futures_util::SinkExt::send(&mut stream, msg).await;
                                    }
                                }
                                WsCommand::Binary(d) => {
                                    if let Ok(msg) = encode_binary_message(&d, config) {
                                        let _ = futures_util::SinkExt::send(&mut stream, msg).await;
                                    }
                                }
                                WsCommand::Close { .. } => {}
                            }
                        }
                        stream_opt = Some(stream);
                    }
                }
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
    fn config_accepts_proxy_tls_dns_and_network_path() {
        let json = r#"{
            "urls": ["wss://example.com/ws"],
            "proxy": {
                "url": "socks5h://127.0.0.1:7890",
                "auth": {"username": "u", "password": "p"},
                "no_proxy": ["localhost"]
            },
            "tls": {"reject_unauthorized": false, "min_tls_version": "Tls1_2"},
            "dns": {"mode": "native"},
            "network_path_id": "vpn-on"
        }"#;

        let config: WsClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.proxy.as_ref().map(|proxy| proxy.url.as_str()),
            Some("socks5h://127.0.0.1:7890")
        );
        assert!(!config.tls.reject_unauthorized);
        assert_eq!(config.tls.min_tls_version, Some(TlsVersion::Tls1_2));
        assert!(!config.dns.as_ref().unwrap().use_catcher_resolver());
        assert_eq!(config.network_path_id.as_deref(), Some("vpn-on"));
    }
}
