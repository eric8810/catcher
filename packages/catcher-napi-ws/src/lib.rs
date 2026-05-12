//! WebSocket client napi bindings

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
    #[napi(constructor)]
    pub fn new(
        config_json: String,
        #[napi(ts_arg_type = "(eventJson: string) => void")] on_event: Option<JsFunction>,
    ) -> napi::Result<Self> {
        let config: WsClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let urls = config.urls.clone();
        let first_url = urls
            .first()
            .cloned()
            .unwrap_or_else(|| "ws://localhost".into());

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
            match WsTransport::connect(&first_url, &config_clone).await {
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

    /// Close the WebSocket connection
    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        if let Some(ref h) = *self.handle.lock().unwrap() {
            h.close(1000, "normal")
                .map_err(|e| napi::Error::from_reason(e.to_string()))
        } else {
            Ok(())
        }
    }
}
