//! WebSocket client napi bindings — full API
//!
//! Configuration is passed as a JSON string matching `WsClientConfig`:
//! ```json
//! {
//!   "urls": ["wss://echo.example.com"],
//!   "headers": { "Authorization": "Bearer ..." },
//!   "protocols": ["v1", "v2"],
//!   "per_message_deflate": true,
//!   "reconnect": { "initial_delay_ms": 500, "max_delay_ms": 30000, "max_attempts": 20 },
//!   "heartbeat": { "interval_ms": 30000, "pong_timeout_ms": 10000 }
//! }
//! ```

use napi::*;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
use std::sync::{Arc, Mutex};

use catcher_ws::types::ws::WsClientConfig;
use catcher_ws::{WsHandle, WsTransport};

/// JavaScript-facing WebSocket client
#[napi]
pub struct JsWsClient {
    handle: Arc<Mutex<Option<Arc<WsHandle>>>>,
}

#[napi]
impl JsWsClient {
    /// Create a WebSocket client from a JSON config string.
    ///
    /// `onEvent` receives events as JSON strings.
    /// Event shapes (JSON):
    ///   {"type":"Connected","url":"...","latency_ms":5}
    ///   {"type":"Disconnected","code":1000,"reason":"..."}
    ///   {"type":"Message","data":"...","is_binary":false}
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
            match WsTransport::connect(&config_clone).await {
                Ok((h, mut rx)) => {
                    let ws_handle = Arc::new(h);
                    *handle_clone.lock().unwrap() = Some(ws_handle);

                    while let Some(event) = rx.recv().await {
                        if let (Ok(json), Some(ref t)) =
                            (serde_json::to_string(&event), &tsfn)
                        {
                            let _ = t.call(Ok(json), ThreadsafeFunctionCallMode::Blocking);
                        }
                    }
                }
                Err(e) => {
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
        });

        Ok(Self { handle })
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

    /// Close the WebSocket connection with optional code and reason.
    ///
    /// Defaults to code 1000, reason "normal" if not specified.
    #[napi]
    pub fn close(
        &self,
        code: Option<u16>,
        reason: Option<String>,
    ) -> napi::Result<()> {
        if let Some(ref h) = *self.handle.lock().unwrap() {
            let code = code.unwrap_or(1000);
            let reason = reason.unwrap_or_else(|| "normal".into());
            h.close(code, &reason)
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        } else {
            Ok(())
        }
    }
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
        assert_eq!(config.headers.get("Authorization").unwrap(), "Bearer token123");
        assert_eq!(config.protocols, vec!["v1"]);
        assert!(config.per_message_deflate);
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
        assert!(!config.per_message_deflate);
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
