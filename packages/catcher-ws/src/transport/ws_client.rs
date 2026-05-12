use tokio::sync::mpsc;

use catcher_core::CatcherError;
use crate::types::ws::*;

use futures_util::{SinkExt, StreamExt};

/// WebSocket 传输层
///
/// 使用 tokio-tungstenite 建立连接，通过 mpsc channel 推送 WsEvent。
/// 重连、心跳、多端点竞速等高级功能由 src/ws/ 层提供。
pub struct WsTransport;

/// WebSocket 连接句柄 — 用于发送消息
#[derive(Clone)]
pub struct WsHandle {
    url: String,
    cmd_tx: mpsc::UnboundedSender<WsCommand>,
}

enum WsCommand {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
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

    /// 返回连接的 URL
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl WsTransport {
    /// 建立 WebSocket 连接
    ///
    /// 返回 (WsHandle, UnboundedReceiver<WsEvent>)。
    /// handle 用于发送消息（同步），receiver 用于接收事件流（异步）。
    pub async fn connect(
        url: &str,
        config: &WsClientConfig,
    ) -> Result<(WsHandle, mpsc::UnboundedReceiver<WsEvent>), CatcherError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

        let url_owned = url.to_string();

        // Build WebSocket config
        let ws_config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(config.max_payload_bytes as usize),
            max_frame_size: Some(config.max_payload_bytes as usize),
            ..Default::default()
        };

        // Connect
        let (ws_stream, _response) = tokio_tungstenite::connect_async_with_config(
            url_owned.as_str(),
            Some(ws_config),
            true, // disable_nagle
        )
        .await
        .map_err(|e| CatcherError::Internal(format!("ws connect failed: {e}")))?;

        // Use a select loop to handle both sending and receiving
        let url_clone = url_owned.clone();
        let tx_read = event_tx.clone();
        tokio::spawn(async move {
            // Split into writer and reader parts
            let (mut writer, mut reader) = ws_stream.split();

            // Emit Connected event
            let _ = tx_read.send(WsEvent::Connected {
                url: url_clone,
                latency_ms: 0,
            });

            loop {
                tokio::select! {
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(WsCommand::Text(t)) => {
                                let msg = tokio_tungstenite::tungstenite::Message::Text(t);
                                if writer.send(msg).await.is_err() { break; }
                            }
                            Some(WsCommand::Binary(d)) => {
                                let msg = tokio_tungstenite::tungstenite::Message::Binary(d);
                                if writer.send(msg).await.is_err() { break; }
                            }
                            Some(WsCommand::Close { code, reason }) => {
                                let msg = tokio_tungstenite::tungstenite::Message::Close(Some(
                                    tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(code),
                                        reason: std::borrow::Cow::Owned(reason),
                                    },
                                ));
                                let _ = writer.send(msg).await;
                                let _ = writer.close().await;
                                break;
                            }
                            None => break, // channel closed
                        }
                    }
                    msg = reader.next() => {
                        match msg {
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t))) => {
                                let _ = tx_read.send(WsEvent::Message {
                                    data: t.as_bytes().to_vec(),
                                    is_binary: false,
                                });
                            }
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(d))) => {
                                let _ = tx_read.send(WsEvent::Message {
                                    data: d,
                                    is_binary: true,
                                });
                            }
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(_))) => {}
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {
                                let _ = tx_read.send(WsEvent::HeartbeatRtt { rtt_ms: 0 });
                            }
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Close(frame))) => {
                                let (code, reason) = frame
                                    .map(|f| {
                                        let c: u16 = f.code.into();
                                        (c, f.reason.into_owned())
                                    })
                                    .unwrap_or((1006, "abnormal".into()));
                                let _ = tx_read.send(WsEvent::Disconnected { code, reason });
                                break;
                            }
                            Some(Ok(tokio_tungstenite::tungstenite::Message::Frame(_))) => {}
                            Some(Err(e)) => {
                                let _ = tx_read.send(WsEvent::Error {
                                    message: e.to_string(),
                                });
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        Ok((
            WsHandle {
                url: url_owned,
                cmd_tx,
            },
            event_rx,
        ))
    }
}
