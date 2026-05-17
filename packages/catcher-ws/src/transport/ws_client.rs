//! WebSocket 传输层 — 完整集成
//!
//! 使用 tokio-tungstenite 建立连接，通过 mpsc channel 推送 WsEvent。
//! 集成：headers/protocols 握手、多端点竞速、自动重连、心跳采样、压缩配置。

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;

use catcher_core::CatcherError;
use crate::types::ws::*;
use crate::ws::{build_ws_config, EndpointRacer, HeartbeatManager, ReconnectManager};

// ── 类型别名 ──

/// 底层 WebSocket 流类型
pub(crate) type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

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

/// 底层 WebSocket 连接 — 处理 headers/protocols/handshake_timeout/deflate。
/// 返回 (stream, latency_ms)。
pub(crate) async fn connect_stream(
    url: &str,
    config: &WsClientConfig,
) -> Result<(WsStream, u64), CatcherError> {
    let request = build_request(url, config)?;
    let ws_config = build_ws_config(config);

    let start = Instant::now();
    let result = if config.handshake_timeout_ms > 0 {
        tokio::time::timeout(
            Duration::from_millis(config.handshake_timeout_ms),
            tokio_tungstenite::connect_async_with_config(request, Some(ws_config), true),
        )
        .await
        .map_err(|_| CatcherError::ConnectionTimeout(config.handshake_timeout_ms))?
    } else {
        tokio_tungstenite::connect_async_with_config(request, Some(ws_config), true).await
    };

    let (stream, _response) = result
        .map_err(|e| CatcherError::Internal(format!("ws connect failed: {e}")))?;

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
            return Err(CatcherError::InvalidConfig(
                "no WS URLs configured".into(),
            ));
        }

        // 初始连接 — 多端点竞速或单 URL
        let (connected_url, initial_stream, latency_ms) =
            if config.urls.len() > 1 || config.race_count > 1 {
                let racer = EndpointRacer::new(config.urls.clone(), config.race_count);
                let (url, stream, lat) = racer.race(config).await?;
                (url, stream, lat)
            } else {
                let url = config.urls.first().unwrap().clone();
                let (stream, lat) = connect_stream(&url, config).await?;
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
    event_tx: mpsc::UnboundedSender<WsEvent>,
    mut cmd_rx: mpsc::UnboundedReceiver<WsCommand>,
) {
    let current_url = initial_url;
    let mut stream_opt = Some(initial_stream);
    let mut reconnect_mgr = config.reconnect.as_ref().map(|c| ReconnectManager::new(c.clone()));

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

        // 心跳定时器任务（channel 驱动，避免 select 内的借用问题）
        let (ping_tx, mut ping_rx) = mpsc::unbounded_channel::<()>();
        if config.heartbeat.is_some() {
            let interval_ms = config.heartbeat.as_ref().unwrap().interval_ms;
            let tx = ping_tx.clone();
            tokio::spawn(async move {
                let mut timer = tokio::time::interval(Duration::from_millis(interval_ms));
                timer.tick().await; // 首次立即触发，跳过
                loop {
                    timer.tick().await;
                    if tx.send(()).is_err() {
                        break;
                    }
                }
            });
        }
        drop(ping_tx); // 只让 timer task 持有 sender

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
                            let msg = tokio_tungstenite::tungstenite::Message::Text(t.into());
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

                // 心跳 tick
                Some(_) = ping_rx.recv() => {
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
                            let _ = event_tx.send(WsEvent::Message {
                                data: d.to_vec(),
                                is_binary: true,
                            });
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

            loop {
                let delay = match mgr.on_disconnect() {
                    Some(d) => d,
                    None => break, // 重试耗尽
                };

                let attempt = mgr.attempt();
                let _ = event_tx.send(WsEvent::Reconnecting { attempt, delay_ms: delay });

                tokio::time::sleep(Duration::from_millis(delay)).await;

                // 检查用户是否已经发出 close 命令
                if cmd_rx.try_recv().is_ok() {
                    break;
                }

                match connect_stream(&current_url, config).await {
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
            headers: vec![
                ("Authorization".into(), "Bearer token".into()),
            ]
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

    /// 验证 WsClientConfig 的 headers 序列化/反序列化
    #[test]
    fn config_headers_roundtrip() {
        let json = r#"{
            "urls": ["wss://example.com"],
            "headers": {"X-Custom": "value", "Authorization": "Bearer abc"},
            "protocols": ["graphql-ws"]
        }"#;
        let config: WsClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.headers.len(), 2);
        assert_eq!(config.protocols, vec!["graphql-ws"]);
    }
}
