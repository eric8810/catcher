use napi::*;
use napi::threadsafe_function::{
    ThreadSafeCallContext, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;

use catcher_http::{SseClient, SseStream};
use catcher_core::types::sse::SseClientConfig;

use crate::helpers::Tsfn;

/// JavaScript-facing SSE stream handle (one-shot, no auto-reconnect).
/// Call `close()` to abort the stream.
#[napi]
pub struct JsSseStream {
    pub(crate) cancel_tx: Option<tokio::sync::mpsc::UnboundedSender<()>>,
}

#[napi]
impl JsSseStream {
    /// Close the SSE stream — aborts the background read loop.
    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        if let Some(ref tx) = self.cancel_tx {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// JavaScript-facing SSE client handle (auto-reconnect).
/// Call `close()` to stop the client and auto-reconnection.
#[napi]
pub struct JsSseClient {
    pub(crate) cancel_tx: Option<tokio::sync::mpsc::UnboundedSender<()>>,
}

#[napi]
impl JsSseClient {
    /// Close the SSE client — stops auto-reconnect.
    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        if let Some(ref tx) = self.cancel_tx {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// Create a one-shot SSE stream (no auto-reconnect).
/// The `onEvent` callback receives JSON strings.
/// Returns a `JsSseStream` handle. Call `.close()` to stop.
#[napi]
pub fn sse_stream(
    config_json: String,
    #[napi(ts_arg_type = "(eventJson: string) => void")] on_event: JsFunction,
) -> napi::Result<JsSseStream> {
    let config: SseClientConfig = serde_json::from_str(&config_json)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let tsfn: Tsfn = on_event
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            Ok(vec![ctx.value])
        })?;

    let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    tokio::spawn(async move {
        match SseStream::connect(config).await {
            Ok(mut stream) => {
                use tokio_stream::StreamExt;
                loop {
                    tokio::select! {
                        result = stream.next() => {
                            match result {
                                Some(Ok(line)) => {
                                    let _ = tsfn.call(
                                        Ok(sse_line_json(&line)),
                                        ThreadsafeFunctionCallMode::NonBlocking,
                                    );
                                }
                                Some(Err(e)) => {
                                    let _ = tsfn.call(
                                        Ok(sse_error_json(&e.to_string())),
                                        ThreadsafeFunctionCallMode::NonBlocking,
                                    );
                                }
                                None => break,
                            }
                        }
                        _ = cancel_rx.recv() => break,
                    }
                }
                let _ = tsfn.call(
                    Ok(sse_end_json()),
                    ThreadsafeFunctionCallMode::NonBlocking,
                );
            }
            Err(e) => {
                let _ = tsfn.call(
                    Ok(sse_error_json(&e.to_string())),
                    ThreadsafeFunctionCallMode::NonBlocking,
                );
            }
        }
    });

    Ok(JsSseStream {
        cancel_tx: Some(cancel_tx),
    })
}

/// Create a long-lived SSE client with auto-reconnect.
#[napi]
pub fn sse_client(
    config_json: String,
    #[napi(ts_arg_type = "(eventJson: string) => void")] on_event: JsFunction,
) -> napi::Result<JsSseClient> {
    let config: SseClientConfig = serde_json::from_str(&config_json)
        .map_err(|e| napi::Error::from_reason(e.to_string()))?;

    let tsfn: Tsfn = on_event
        .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
            Ok(vec![ctx.value])
        })?;

    let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    tokio::spawn(async move {
        match SseClient::connect(config).await {
            Ok(mut client) => {
                loop {
                    tokio::select! {
                        result = client.next_line() => {
                            match result {
                                Some(Ok(line)) => {
                                    let _ = tsfn.call(
                                        Ok(sse_line_json(&line)),
                                        ThreadsafeFunctionCallMode::NonBlocking,
                                    );
                                }
                                Some(Err(e)) => {
                                    let _ = tsfn.call(
                                        Ok(sse_error_json(&e.to_string())),
                                        ThreadsafeFunctionCallMode::NonBlocking,
                                    );
                                }
                                None => break,
                            }
                        }
                        _ = cancel_rx.recv() => {
                            client.close();
                            break;
                        }
                    }
                }
                let _ = tsfn.call(
                    Ok(sse_end_json()),
                    ThreadsafeFunctionCallMode::NonBlocking,
                );
            }
            Err(e) => {
                let _ = tsfn.call(
                    Ok(sse_error_json(&e.to_string())),
                    ThreadsafeFunctionCallMode::NonBlocking,
                );
            }
        }
    });

    Ok(JsSseClient {
        cancel_tx: Some(cancel_tx),
    })
}

// ── SSE Helpers ──

pub(crate) fn sse_line_json(line: &str) -> String {
    serde_json::json!({"type": "Line", "data": line}).to_string()
}

pub(crate) fn sse_error_json(msg: &str) -> String {
    serde_json::json!({"type": "Error", "message": msg}).to_string()
}

pub(crate) fn sse_end_json() -> String {
    serde_json::json!({"type": "End"}).to_string()
}
