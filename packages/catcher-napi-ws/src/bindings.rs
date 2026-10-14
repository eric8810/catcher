//! WebSocket client napi bindings — full API
//!
//! Configuration is passed as a JSON string matching `WsClientConfig`:
//! ```json
//! {
//!   "urls": ["wss://echo.example.com"],
//!   "headers": { "Authorization": "Bearer ..." },
//!   "protocols": ["v1", "v2"],
//!   "per_message_deflate": true,
//!   "dns": { "mode": "catcher", "cache_ttl_secs": 300 },
//!   "msgpack": true,
//!   "reconnect": { "initial_delay_ms": 500, "max_delay_ms": 30000, "max_attempts": 20 },
//!   "heartbeat": { "interval_ms": 30000, "pong_timeout_ms": 10000 }
//! }
//! ```

use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{Env, JsFunction, JsObject};
use napi_derive::napi;
use std::sync::{Arc, Mutex};

use catcher_ws::types::ws::WsClientConfig;
use catcher_ws::{WsHandle, WsShutdown, WsTransport};
use tokio::sync::watch;

/// JavaScript-facing WebSocket client
#[napi]
pub struct JsWsClient {
    handle: Arc<Mutex<Option<Arc<WsHandle>>>>,
    shutdown: WsShutdown,
    forwarded: watch::Receiver<bool>,
}

#[napi]
impl JsWsClient {
    /// Create a WebSocket client from a JSON config string.
    ///
    /// `onEvent` receives events as JSON strings.
    /// Event shapes (JSON):
    ///   {"type":"Connected","url":"...","latency_ms":5}
    ///   {"type":"Disconnected","code":1000,"reason":"..."}
    ///   {"type":"Message","data_base64":"...","is_binary":false}
    ///   {"type":"Error","message":"..."}
    ///   {"type":"Reconnecting","attempt":1,"delay_ms":500}
    ///   {"type":"HeartbeatRtt","rtt_ms":12}
    #[napi(constructor)]
    pub fn new(
        config_json: String,
        #[napi(ts_arg_type = "(eventJson: string) => void")] on_event: Option<JsFunction>,
    ) -> napi::Result<Self> {
        let config: WsClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        if config.urls.is_empty() {
            return Err(napi::Error::from_reason("urls cannot be empty"));
        }

        let handle = Arc::new(Mutex::new(None::<Arc<WsHandle>>));
        let handle_clone = handle.clone();
        let shutdown = WsShutdown::default();
        let background_shutdown = shutdown.clone();
        let (forwarded_tx, forwarded) = watch::channel(false);
        let config_clone = config.clone();

        // Build threadsafe callback
        let tsfn: Option<ThreadsafeFunction<String, ErrorStrategy::CalleeHandled>> =
            if let Some(js_fn) = on_event {
                let t = js_fn
                    .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                        Ok(vec![ctx.value])
                    })?;
                Some(t)
            } else {
                None
            };

        // Connect in background
        tokio::spawn(async move {
            match WsTransport::connect_with_shutdown(&config_clone, background_shutdown.clone())
                .await
            {
                Ok((h, mut rx)) => {
                    let ws_handle = Arc::new(h);
                    *handle_clone.lock().unwrap() = Some(ws_handle);

                    loop {
                        let event = tokio::select! {
                            biased;
                            _ = background_shutdown.cancelled() => break,
                            event = rx.recv() => match event { Some(event) => event, None => break },
                        };
                        if background_shutdown.is_cancelled() {
                            break;
                        }
                        let json = event.to_ffi_json();
                        if let Some(ref t) = tsfn {
                            let _ = t.call(Ok(json), ThreadsafeFunctionCallMode::Blocking);
                        }
                    }
                }
                Err(e) => {
                    if !background_shutdown.is_cancelled() {
                        if let Some(ref t) = tsfn {
                            let json = serde_json::json!({
                                "type": "Error",
                                "message": e.to_string()
                            })
                            .to_string();
                            let _ = t.call(Ok(json), ThreadsafeFunctionCallMode::Blocking);
                        }
                    }
                }
            }
            // 等待网络任务退出后再完成转发，保证 closeAndWait 的本地停止语义。
            background_shutdown.wait_closed().await;
            forwarded_tx.send_replace(true);
        });

        Ok(Self {
            handle,
            shutdown,
            forwarded,
        })
    }

    /// Send a text message
    #[napi]
    pub fn send(&self, data: String) -> napi::Result<()> {
        if let Some(ref h) = *self.handle.lock().unwrap() {
            h.send_text(&data)
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        } else {
            Err(napi::Error::from_reason("WebSocket not connected"))
        }
    }

    /// Send a binary message
    #[napi]
    pub fn send_binary(&self, data: napi::bindgen_prelude::Buffer) -> napi::Result<()> {
        if let Some(ref h) = *self.handle.lock().unwrap() {
            h.send_binary(data.as_ref())
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        } else {
            Err(napi::Error::from_reason("WebSocket not connected"))
        }
    }

    /// Notify the client that the network environment changed
    /// (WiFi switch, VPN node change, cellular handover, ...).
    ///
    /// Immediately drops the (likely half-open) connection, clears the DNS
    /// cache, resets reconnect backoff and reconnects right away — instead of
    /// waiting 10-30s for heartbeat timeout. With multiple endpoints the race
    /// is re-run, since the best endpoint may differ on the new network.
    #[napi]
    pub fn network_changed(&self) -> napi::Result<()> {
        if let Some(ref h) = *self.handle.lock().unwrap() {
            h.network_changed()
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        } else {
            Err(napi::Error::from_reason("WebSocket not connected"))
        }
    }

    /// 请求终止连接、首次握手和重连，丢弃尚未发送的缓存；可重复调用。
    /// 默认 code=1000、reason="normal"，已连接时尽力发送 Close 帧。
    /// 返回仅表示已保存终止请求；使用 close_and_wait 确认本地停止。
    #[napi]
    pub fn close(&self, code: Option<u16>, reason: Option<String>) -> napi::Result<()> {
        // 即使首次握手未结束，关闭意图也由 shutdown 保存。
        if let Some(ref h) = *self.handle.lock().unwrap() {
            h.close(
                code.unwrap_or(1000),
                &reason.unwrap_or_else(|| "normal".into()),
            )
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        }
        self.shutdown.cancel();
        Ok(())
    }

    /// 同步保存终止请求，然后返回等待 WS 连接管理器与事件转发退出的 Promise。
    /// 不等待共享 DNS 后台刷新、已排入 Node.js 队列的回调或对端确认。
    #[napi(ts_return_type = "Promise<void>")]
    pub fn close_and_wait(
        &self,
        env: Env,
        code: Option<u16>,
        reason: Option<String>,
    ) -> napi::Result<JsObject> {
        self.close(code, reason)?;
        let mut forwarded = self.forwarded.clone();
        env.spawn_future(async move {
            let _ = forwarded.wait_for(|value| *value).await;
            Ok(())
        })
    }
}

impl Drop for JsWsClient {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

// ── Codec: expose Rust rmp-serde pack/unpack for benchmarking ──

/// Encode a JS value to msgpack bytes (Rust rmp-serde).
#[napi]
pub fn pack(value: serde_json::Value) -> napi::Result<napi::bindgen_prelude::Buffer> {
    let bytes =
        catcher_ws::codec::pack(&value).map_err(|e| napi::Error::from_reason(e.to_string()))?;
    Ok(bytes.into())
}

/// Decode msgpack bytes to a JSON string (Rust rmp-serde).
#[napi]
pub fn unpack(data: napi::bindgen_prelude::Buffer) -> napi::Result<String> {
    let value: serde_json::Value = catcher_ws::codec::unpack(data.as_ref())
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;
    serde_json::to_string(&value).map_err(|e| napi::Error::from_reason(e.to_string()))
}

// ── Rust unit tests ──

#[cfg(test)]
mod tests {
    use catcher_ws::types::ws::WsClientConfig;

    /// Verify WsClientConfig parses headers, protocols, per_message_deflate, reconnect, heartbeat
    #[test]
    fn ws_config_full_parse() {
        let json = r#"{
            "urls": ["wss://example.com/ws"],
            "headers": {"Authorization": "Bearer token123"},
            "protocols": ["v1"],
            "per_message_deflate": true,
            "dns": {
                "mode": "catcher",
                "cache_size": 256,
                "cache_ttl_secs": 60,
                "negative_ttl_secs": 30,
                "stale_ttl_secs": 600,
                "stale_on_error": true,
                "host_mapping": {"example.com": "127.0.0.1"}
            },
            "msgpack": true,
            "reconnect": {
                "initial_delay_ms": 200,
                "max_delay_ms": 5000,
                "max_attempts": 3
            },
            "heartbeat": {
                "interval_ms": 10000,
                "pong_timeout_ms": 5000
            }
        }"#;
        let config: WsClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.urls, vec!["wss://example.com/ws"]);
        assert_eq!(
            config.headers.get("Authorization").unwrap(),
            "Bearer token123"
        );
        assert_eq!(config.protocols, vec!["v1"]);
        assert!(config.per_message_deflate);
        assert!(config.msgpack);
        let dns = config.dns.unwrap();
        assert_eq!(dns.cache_size, 256);
        assert_eq!(dns.cache_ttl_secs, 60);
        assert_eq!(dns.negative_ttl_secs, 30);
        assert_eq!(dns.stale_ttl_secs, 600);
        assert_eq!(dns.host_mapping.get("example.com").unwrap(), "127.0.0.1");
        let rc = config.reconnect.unwrap();
        assert_eq!(rc.initial_delay_ms, 200);
        assert_eq!(rc.max_attempts, 3);
        let hb = config.heartbeat.unwrap();
        assert_eq!(hb.interval_ms, 10000);
        assert_eq!(hb.pong_timeout_ms, 5000);
    }

    /// Verify WsClientConfig defaults when fields omitted
    #[test]
    fn ws_config_defaults() {
        let json = r#"{"urls": ["ws://localhost"]}"#;
        let config: WsClientConfig = serde_json::from_str(json).unwrap();
        assert!(config.headers.is_empty());
        assert!(config.protocols.is_empty());
        assert!(config.per_message_deflate);
        assert!(!config.msgpack);
        assert!(config.dns.is_none());
        assert!(config.reconnect.is_none());
        assert!(config.heartbeat.is_none());
    }

    /// Verify WsEvent serializes to tagged JSON
    #[test]
    fn ws_event_serialization() {
        use catcher_ws::types::ws::WsEvent;

        let evt = WsEvent::Connected {
            url: "wss://example.com".into(),
            latency_ms: 5,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"Connected\""));
        assert!(json.contains("\"latency_ms\":5"));

        let evt = WsEvent::Message {
            data: vec![72, 101, 108, 108, 111],
            is_binary: false,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"Message\""));

        let evt = WsEvent::Reconnecting {
            attempt: 2,
            delay_ms: 1000,
        };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"Reconnecting\""));

        let evt = WsEvent::HeartbeatRtt { rtt_ms: 42 };
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"type\":\"HeartbeatRtt\""));
    }

    /// Verify close params — default and custom
    #[test]
    #[allow(clippy::unnecessary_literal_unwrap)]
    fn close_params_default_and_custom() {
        // Test that the param extraction logic is correct
        let code: Option<u16> = None;
        let reason: Option<String> = None;
        assert_eq!(code.unwrap_or(1000), 1000);
        assert_eq!(reason.unwrap_or_else(|| "normal".into()), "normal");

        let code: Option<u16> = Some(1001);
        let reason: Option<String> = Some("going away".into());
        assert_eq!(code.unwrap_or(1000), 1001);
        assert_eq!(reason.unwrap_or_else(|| "normal".into()), "going away");
    }
}

/// 仅测试特性：控制真实原生 Sink 的 flush 闸门。
#[cfg(feature = "lifecycle-test-hooks")]
#[napi]
pub fn test_set_flush_gate(paused: bool) {
    catcher_ws::transport::test_gate::set_paused(paused);
}

/// 仅测试特性：读取 Pending 与取消后的 Sink 调用计数。
#[cfg(feature = "lifecycle-test-hooks")]
#[napi]
pub fn test_flush_gate_state() -> napi::Result<serde_json::Value> {
    serde_json::to_value(catcher_ws::transport::test_gate::snapshot())
        .map_err(|error| napi::Error::from_reason(error.to_string()))
}

/// 仅测试特性：在同步关闭请求返回后开始记录后续 Sink 调用。
#[cfg(feature = "lifecycle-test-hooks")]
#[napi]
pub fn test_mark_close_requested() {
    catcher_ws::transport::test_gate::mark_close_requested();
}

/// 仅测试特性：释放 flush 并保留取消后的调用计数。
#[cfg(feature = "lifecycle-test-hooks")]
#[napi]
pub fn test_release_flush_gate() {
    catcher_ws::transport::test_gate::release_flush();
}
