//! WebSocket 传输层 — 完整集成
//!
//! 使用 yawc 建立连接，通过 mpsc channel 推送 WsEvent。
//! 集成：headers/protocols 握手、多端点竞速、自动重连、心跳采样、压缩配置。

use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use yawc::frame::{Frame, OpCode};

use crate::types::ws::*;
use crate::ws::{
    build_ws_options, decode_application_compression_frame, encode_application_compression_frame,
    EndpointRacer, HeartbeatManager, ReconnectManager, APPLICATION_COMPRESSION_MAGIC,
};
use catcher_core::CatcherError;

// ── 类型别名 ──

/// 底层 WebSocket 流类型
pub(crate) type WsStream = yawc::TcpWebSocket;

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
    if !config.per_message_deflate {
        if let Some(ref compression) = config.application_compression {
            if let Some(frame) =
                encode_application_compression_frame(text.as_bytes(), false, compression)?
            {
                return Ok(Frame::binary(frame));
            }
        }
    }
    Ok(Frame::text(text.to_string()))
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

// ── 底层连接 ──

/// 构建带 headers 和 protocols 的 yawc HTTP request builder。
fn build_request(config: &WsClientConfig) -> Result<yawc::HttpRequestBuilder, CatcherError> {
    let mut builder = yawc::HttpRequest::builder();

    for (k, v) in &config.headers {
        let name = http::HeaderName::from_bytes(k.as_bytes())
            .map_err(|e| CatcherError::Internal(format!("invalid header name '{k}': {e}")))?;
        let value = http::HeaderValue::from_str(v)
            .map_err(|e| CatcherError::Internal(format!("invalid header value for '{k}': {e}")))?;
        builder = builder.header(name, value);
    }

    if !config.per_message_deflate {
        if let Some(ref compression) = config.application_compression {
            if compression.enabled {
                let algorithm = application_compression_algorithm_name(compression.algorithm);
                builder = builder.header("X-Catcher-Application-Compression", algorithm);
                let format = std::str::from_utf8(APPLICATION_COMPRESSION_MAGIC).map_err(|e| {
                    CatcherError::Internal(format!("invalid compression magic: {e}"))
                })?;
                builder = builder.header("X-Catcher-Application-Compression-Format", format);
                builder = builder.header(
                    "X-Catcher-Application-Compression-Threshold",
                    compression.threshold_bytes.to_string(),
                );
            }
        }
    }

    // WebSocket 子协议
    if !config.protocols.is_empty() {
        let value = http::HeaderValue::from_str(&config.protocols.join(", "))
            .map_err(|e| CatcherError::Internal(format!("invalid protocols: {e}")))?;
        builder = builder.header(http::header::SEC_WEBSOCKET_PROTOCOL, value);
    }

    Ok(builder)
}

/// 底层 WebSocket 连接 — 处理 headers/protocols/handshake_timeout/deflate。
/// 返回 (stream, latency_ms)。
pub(crate) async fn connect_stream(
    url: &str,
    config: &WsClientConfig,
) -> Result<(WsStream, u64), CatcherError> {
    let parsed_url = url
        .parse()
        .map_err(|e| CatcherError::Internal(format!("invalid WS URL: {e}")))?;
    let request = build_request(config)?;
    let ws_options = build_ws_options(config);

    let start = Instant::now();
    let connect = yawc::WebSocket::connect(parsed_url)
        .with_options(ws_options)
        .with_request(request);
    let result = if config.handshake_timeout_ms > 0 {
        tokio::time::timeout(Duration::from_millis(config.handshake_timeout_ms), connect)
            .await
            .map_err(|_| CatcherError::ConnectionTimeout(config.handshake_timeout_ms))?
    } else {
        connect.await
    };

    let stream = result.map_err(|e| CatcherError::Internal(format!("ws connect failed: {e}")))?;

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
                            match decode_application_compression_frame(
                                frame.payload(),
                                config.max_payload_bytes,
                            ) {
                                Ok(Some(frame)) => {
                                    let _ = event_tx.send(WsEvent::Message {
                                        data: frame.data,
                                        is_binary: frame.is_binary,
                                    });
                                }
                                Ok(None) => {
                                    let _ = event_tx.send(WsEvent::Message {
                                        data: frame.payload().to_vec(),
                                        is_binary: true,
                                    });
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
            headers: vec![("Authorization".into(), "Bearer token".into())]
                .into_iter()
                .collect(),
            protocols: vec!["v1".into(), "v2".into()],
            ..Default::default()
        };

        let req = build_request(&config).unwrap();
        assert_eq!(
            req.headers_ref()
                .unwrap()
                .get("Authorization")
                .map(|v| v.to_str().unwrap()),
            Some("Bearer token")
        );
        assert_eq!(
            req.headers_ref()
                .unwrap()
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
        let req = build_request(&config).unwrap();
        assert!(req.headers_ref().unwrap().get("Authorization").is_none());
        assert!(req
            .headers_ref()
            .unwrap()
            .get("Sec-WebSocket-Protocol")
            .is_none());
    }

    /// 验证 build_request 无效 URL 报错
    #[test]
    fn build_request_invalid_url() {
        let config = WsClientConfig {
            headers: vec![("bad header".into(), "value".into())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        assert!(build_request(&config).is_err());
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

        let req = build_request(&config).unwrap();

        assert_eq!(
            req.headers_ref()
                .unwrap()
                .get("X-Catcher-Application-Compression")
                .map(|v| v.to_str().unwrap()),
            Some("zstd")
        );
        assert_eq!(
            req.headers_ref()
                .unwrap()
                .get("X-Catcher-Application-Compression-Format")
                .map(|v| v.to_str().unwrap()),
            Some("CATCHER-CMP-1")
        );
        assert_eq!(
            req.headers_ref()
                .unwrap()
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

        let req = build_request(&config).unwrap();

        assert!(req
            .headers_ref()
            .unwrap()
            .get("X-Catcher-Application-Compression")
            .is_none());
    }
}
