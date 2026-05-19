//! WebSocket 传输层 — 完整集成
//!
//! 使用 tokio-tungstenite 建立连接，通过 mpsc channel 推送 WsEvent。
//! 集成：headers/protocols 握手、多端点竞速、自动重连、心跳采样、压缩配置。

use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(feature = "rustls-tls")]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use crate::types::ws::*;
use crate::ws::{build_ws_config, EndpointRacer, HeartbeatManager, ReconnectManager};
use catcher_core::CatcherError;
use catcher_dns::{build_stale_aware_resolver, DnsResolver};

// ── 类型别名 ──

/// 底层 WebSocket 流类型
pub(crate) type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

pub(crate) type SharedDnsResolver = Arc<DnsResolver>;

// ── TLS 初始化 ──

/// 确保 rustls CryptoProvider 已安装（仅需执行一次）。
/// rustls 0.23+ 不再内置默认 crypto backend，必须显式安装。
#[cfg(feature = "rustls-tls")]
static TLS_PROVIDER_INSTALLED: OnceLock<()> = OnceLock::new();

#[cfg(feature = "rustls-tls")]
fn ensure_tls_provider() {
    TLS_PROVIDER_INSTALLED.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(not(feature = "rustls-tls"))]
fn ensure_tls_provider() {}

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

#[derive(Debug)]
enum ResolvedAddrConnectError {
    Tcp {
        addr: SocketAddr,
        source: io::Error,
    },
    TcpOption {
        addr: SocketAddr,
        source: io::Error,
    },
    Handshake {
        addr: SocketAddr,
        source: tokio_tungstenite::tungstenite::Error,
    },
    HandshakeTimeout {
        addr: SocketAddr,
        timeout_ms: u64,
    },
}

impl fmt::Display for ResolvedAddrConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp { addr, source } => write!(f, "{addr}: TCP connect failed: {source}"),
            Self::TcpOption { addr, source } => {
                write!(f, "{addr}: set TCP_NODELAY failed: {source}")
            }
            Self::Handshake { addr, source } => write!(f, "{addr}: WS handshake failed: {source}"),
            Self::HandshakeTimeout { addr, timeout_ms } => {
                write!(f, "{addr}: WS handshake timeout after {timeout_ms}ms")
            }
        }
    }
}

// ── 底层连接 ──

/// 构建带 headers 和 protocols 的 tungstenite Request
fn build_request(
    url: &str,
    config: &WsClientConfig,
) -> Result<tokio_tungstenite::tungstenite::handshake::client::Request, CatcherError> {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    let mut request = url
        .into_client_request()
        .map_err(|e| CatcherError::Internal(format!("invalid WS URL: {e}")))?;

    let headers = request.headers_mut();

    // 自定义 headers
    for (k, v) in &config.headers {
        let name = tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| CatcherError::Internal(format!("invalid header name '{k}': {e}")))?;
        let value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(v)
            .map_err(|e| CatcherError::Internal(format!("invalid header value for '{k}': {e}")))?;
        headers.append(name, value);
    }

    // WebSocket 子协议
    if !config.protocols.is_empty() {
        let value = tokio_tungstenite::tungstenite::http::HeaderValue::from_str(
            &config.protocols.join(", "),
        )
        .map_err(|e| CatcherError::Internal(format!("invalid protocols: {e}")))?;
        headers.insert(
            tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL,
            value,
        );
    }

    Ok(request)
}

fn request_host_port(
    request: &tokio_tungstenite::tungstenite::handshake::client::Request,
) -> Result<(String, u16), CatcherError> {
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

fn build_dns_resolver(config: &WsClientConfig) -> Result<SharedDnsResolver, CatcherError> {
    let dns_config = config.dns.clone().unwrap_or_default();
    build_stale_aware_resolver(&dns_config)
}

async fn connect_with_dns(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
    ws_config: tokio_tungstenite::tungstenite::protocol::WebSocketConfig,
    resolver: &SharedDnsResolver,
    handshake_timeout_ms: u64,
) -> Result<
    (
        WsStream,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    CatcherError,
> {
    let (host, port) = request_host_port(&request)?;
    let addrs = resolver.resolve_socket_addrs(&host, port).await?;
    connect_with_resolved_addrs(request, ws_config, &host, port, addrs, handshake_timeout_ms).await
}

async fn connect_with_resolved_addrs(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
    ws_config: tokio_tungstenite::tungstenite::protocol::WebSocketConfig,
    host: &str,
    port: u16,
    addrs: Vec<SocketAddr>,
    handshake_timeout_ms: u64,
) -> Result<
    (
        WsStream,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    CatcherError,
> {
    #[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
    {
        if request.uri().scheme_str() == Some("wss") {
            return Err(CatcherError::InvalidConfig(
                "wss requires rustls-tls or native-tls feature".into(),
            ));
        }
    }

    let mut last_error: Option<ResolvedAddrConnectError> = None;

    for addr in addrs {
        let attempt = connect_to_resolved_addr(request.clone(), ws_config, addr);
        let result = if handshake_timeout_ms > 0 {
            match tokio::time::timeout(Duration::from_millis(handshake_timeout_ms), attempt).await {
                Ok(result) => result,
                Err(_) => {
                    last_error = Some(ResolvedAddrConnectError::HandshakeTimeout {
                        addr,
                        timeout_ms: handshake_timeout_ms,
                    });
                    continue;
                }
            }
        } else {
            attempt.await
        };

        match result {
            Ok(pair) => return Ok(pair),
            Err(e) => {
                last_error = Some(e);
            }
        }
    }

    if let Some(ResolvedAddrConnectError::HandshakeTimeout { timeout_ms, .. }) = last_error {
        return Err(CatcherError::WsHandshakeTimeout(timeout_ms));
    }

    Err(CatcherError::Internal(format!(
        "ws connect failed: no resolved address for {host}:{port} succeeded{}",
        last_error.map(|e| format!(": {e}")).unwrap_or_default(),
    )))
}

async fn connect_to_resolved_addr(
    request: tokio_tungstenite::tungstenite::handshake::client::Request,
    ws_config: tokio_tungstenite::tungstenite::protocol::WebSocketConfig,
    addr: SocketAddr,
) -> Result<
    (
        WsStream,
        tokio_tungstenite::tungstenite::handshake::client::Response,
    ),
    ResolvedAddrConnectError,
> {
    let socket = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(|source| ResolvedAddrConnectError::Tcp { addr, source })?;
    socket
        .set_nodelay(true)
        .map_err(|source| ResolvedAddrConnectError::TcpOption { addr, source })?;

    #[cfg(any(feature = "rustls-tls", feature = "native-tls"))]
    {
        tokio_tungstenite::client_async_tls_with_config(request, socket, Some(ws_config), None)
            .await
            .map_err(|source| ResolvedAddrConnectError::Handshake { addr, source })
    }

    #[cfg(not(any(feature = "rustls-tls", feature = "native-tls")))]
    {
        tokio_tungstenite::client_async_with_config(
            request,
            tokio_tungstenite::MaybeTlsStream::Plain(socket),
            Some(ws_config),
        )
        .await
        .map_err(|source| ResolvedAddrConnectError::Handshake { addr, source })
    }
}

/// 底层 WebSocket 连接 — 处理 headers/protocols/handshake_timeout/deflate。
/// 返回 (stream, latency_ms)。
pub(crate) async fn connect_stream_with_resolver(
    url: &str,
    config: &WsClientConfig,
    resolver: &SharedDnsResolver,
) -> Result<(WsStream, u64), CatcherError> {
    ensure_tls_provider();

    let request = build_request(url, config)?;
    let ws_config = build_ws_config(config);

    let start = Instant::now();
    let result = connect_with_dns(request, ws_config, resolver, config.handshake_timeout_ms).await;

    let (stream, _response) = result?;

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
    /// - `per_message_deflate`: 压缩（受 tungstenite 0.24 限制）
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
        let dns_resolver = build_dns_resolver(config)?;

        // 初始连接 — 多端点竞速或单 URL
        let (connected_url, initial_stream, latency_ms) = if config.urls.len() > 1
            || config.race_count > 1
        {
            let racer = EndpointRacer::new(config.urls.clone(), config.race_count);
            let (url, stream, lat) = racer.race(config, &dns_resolver).await?;
            (url, stream, lat)
        } else {
            let url = config.urls.first().unwrap().clone();
            let (stream, lat) = connect_stream_with_resolver(&url, config, &dns_resolver).await?;
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
                dns_resolver,
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
    dns_resolver: SharedDnsResolver,
    event_tx: mpsc::UnboundedSender<WsEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<WsCommand>,
) {
    let current_url = initial_url;
    let mut stream_opt = Some(initial_stream);
    let mut reconnect_mgr = config
        .reconnect
        .as_ref()
        .map(|c| ReconnectManager::new(c.clone()));

    let first_latency = initial_latency_ms;
    let mut first_connect = true;

    loop {
        // 发送 Connected 事件
        {
            let lat = if first_connect {
                first_latency
            } else {
                0 // 重连时无精确延迟测量
            };
            let _ = event_tx.send(WsEvent::Connected {
                url: current_url.clone(),
                latency_ms: lat,
            });
        }
        first_connect = false;

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
                            let msg = if config.msgpack {
                                // msgpack: JSON text → msgpack binary frame
                                match serde_json::from_str::<serde_json::Value>(&t)
                                    .map_err(|e| e.to_string())
                                    .and_then(|v| rmp_serde::to_vec(&v).map_err(|e| e.to_string()))
                                {
                                    Ok(bin) => tokio_tungstenite::tungstenite::Message::Binary(bin.into()),
                                    Err(_) => tokio_tungstenite::tungstenite::Message::Text(t.into()),
                                }
                            } else {
                                tokio_tungstenite::tungstenite::Message::Text(t.into())
                            };
                            if writer.send(msg).await.is_err() {
                                break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send failed".into(),
                                };
                            }
                        }
                        Some(WsCommand::Binary(d)) => {
                            let msg = tokio_tungstenite::tungstenite::Message::Binary(d.into());
                            if writer.send(msg).await.is_err() {
                                break LoopOutcome::Disconnected {
                                    code: 1006,
                                    reason: "send failed".into(),
                                };
                            }
                        }
                        Some(WsCommand::Close { code, reason }) => {
                            let msg = tokio_tungstenite::tungstenite::Message::Close(Some(
                                tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                    code: code.into(),
                                    reason: reason.into(),
                                },
                            ));
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
                            state.mgr.on_missed_pong();
                            if state.mgr.is_missed_pongs_exceeded() {
                                break LoopOutcome::HeartbeatTimeout;
                            }
                        }
                        state.waiting_for_pong = true;
                        state.ping_sent_at = Some(Instant::now());
                        let _ = writer.send(
                            tokio_tungstenite::tungstenite::Message::Ping(Vec::new().into()),
                        ).await;
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
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) => {
                            let _ = event_tx.send(WsEvent::Message {
                                data: t.as_bytes().to_vec(),
                                is_binary: false,
                            });
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(d))) => {
                            if config.msgpack {
                                // msgpack: decode binary frame → JSON text
                                match rmp_serde::from_slice::<serde_json::Value>(&d)
                                    .map_err(|e| e.to_string())
                                    .and_then(|v| serde_json::to_string(&v).map_err(|e| e.to_string()))
                                {
                                    Ok(json) => {
                                        let _ = event_tx.send(WsEvent::Message {
                                            data: json.into_bytes(),
                                            is_binary: false,
                                        });
                                    }
                                    Err(_) => {
                                        let _ = event_tx.send(WsEvent::Message {
                                            data: d.to_vec(),
                                            is_binary: true,
                                        });
                                    }
                                }
                            } else {
                                let _ = event_tx.send(WsEvent::Message {
                                    data: d.to_vec(),
                                    is_binary: true,
                                });
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(_))) => {}
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {
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
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Close(frame))) => {
                            let (code, reason) = frame
                                .map(|f| {
                                    let c: u16 = f.code.into();
                                    (c, f.reason.to_string())
                                })
                                .unwrap_or((1006, "abnormal".into()));
                            break LoopOutcome::Disconnected { code, reason };
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Frame(_))) => {}
                        Some(Err(e)) => {
                            break LoopOutcome::Disconnected {
                                code: 1006,
                                reason: e.to_string(),
                            };
                        }
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

            while let Some(delay) = mgr.on_disconnect() {
                let attempt = mgr.attempt();
                let _ = event_tx.send(WsEvent::Reconnecting {
                    attempt,
                    delay_ms: delay,
                });

                tokio::time::sleep(Duration::from_millis(delay)).await;

                // 检查用户是否已经发出 close 命令
                if cmd_rx.try_recv().is_ok() {
                    break;
                }

                match connect_stream_with_resolver(&current_url, config, &dns_resolver).await {
                    Ok((stream, _lat)) => {
                        stream_opt = Some(stream);
                        mgr.on_connected();
                        reconnected = true;
                        break;
                    }
                    Err(_) => continue,
                }
            }

            if reconnected {
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

    /// 验证 WS 连接使用 DNS host_mapping，而不是系统 DNS
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
        let resolver = build_dns_resolver(&config).unwrap();

        let result = connect_stream_with_resolver(&config.urls[0], &config, &resolver).await;
        assert!(result.is_ok());
        drop(result);
        let _ = server.await.unwrap();
    }

    /// 验证未配置 DNS 时，默认路径仍能连接本机 IP
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
        let resolver = build_dns_resolver(&config).unwrap();

        let result = connect_stream_with_resolver(&config.urls[0], &config, &resolver).await;
        assert!(result.is_ok());
        drop(result);
        let _ = server.await.unwrap();
    }

    /// 验证第一个 IP 握手失败后，会继续尝试下一个 IP。
    #[tokio::test]
    async fn connect_with_dns_retries_next_ip_after_handshake_failure() {
        use tokio::io::AsyncWriteExt;

        let bad_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bad_addr = bad_listener.local_addr().unwrap();
        let bad_server = tokio::spawn(async move {
            let (mut stream, _) = bad_listener.accept().await.unwrap();
            let _ = stream
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n")
                .await;
        });

        let good_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let good_addr = good_listener.local_addr().unwrap();
        let good_server = tokio::spawn(async move {
            let (stream, _) = good_listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://retry.test:{}/ws", good_addr.port())],
            handshake_timeout_ms: 1_000,
            ..Default::default()
        };
        let request = build_request(&config.urls[0], &config).unwrap();
        let ws_config = build_ws_config(&config);
        let result = connect_with_resolved_addrs(
            request,
            ws_config,
            "retry.test",
            good_addr.port(),
            vec![bad_addr, good_addr],
            1_000,
        )
        .await;

        assert!(result.is_ok());
        drop(result);
        bad_server.await.unwrap();
        let _ = good_server.await.unwrap();
    }

    /// 验证第一个 IP 握手卡住时，也会在超时后继续尝试下一个 IP。
    #[tokio::test]
    async fn connect_with_dns_retries_next_ip_after_handshake_timeout() {
        let bad_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let bad_addr = bad_listener.local_addr().unwrap();
        let bad_server = tokio::spawn(async move {
            let (_stream, _) = bad_listener.accept().await.unwrap();
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        });

        let good_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let good_addr = good_listener.local_addr().unwrap();
        let good_server = tokio::spawn(async move {
            let (stream, _) = good_listener.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });

        let config = WsClientConfig {
            urls: vec![format!("ws://retry-timeout.test:{}/ws", good_addr.port())],
            handshake_timeout_ms: 50,
            ..Default::default()
        };
        let request = build_request(&config.urls[0], &config).unwrap();
        let ws_config = build_ws_config(&config);
        let result = connect_with_resolved_addrs(
            request,
            ws_config,
            "retry-timeout.test",
            good_addr.port(),
            vec![bad_addr, good_addr],
            config.handshake_timeout_ms,
        )
        .await;

        assert!(result.is_ok());
        drop(result);
        bad_server.abort();
        let _ = good_server.await.unwrap();
    }
}
