//! SSE C ABI — connect / stream / close / destroy
//!
//! Provides 6 C ABI symbols bridging catcher-http's SseClient (persistent + auto-reconnect)
//! and SseStream (one-shot POST SSE) to FFI consumers.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;

use catcher_core::ffi_helpers::{self, CancellationGuard};
use catcher_core::types::sse::{SseClientConfig, SseMethod};
use catcher_core::{EventCallback, FfiString, HandleRegistry};

use crate::sse::client::{SseClient, SseReadyState};
use crate::sse::SseStream;

// ── Handle registry for SseClient ──
use tokio::sync::Mutex as TokioMutex;

static SSE_REGISTRY: HandleRegistry<TokioMutex<SseClient>> = HandleRegistry::new();
static SSE_GUARD: std::sync::OnceLock<CancellationGuard> = std::sync::OnceLock::new();
fn sse_guard() -> &'static CancellationGuard { SSE_GUARD.get_or_init(CancellationGuard::new) }

fn sse_runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher SSE FFI")
    })
}

fn build_sse_event_json(event_type: &str, data: &str) -> String {
    serde_json::json!({ "type": event_type, "data": data }).to_string()
}

fn invoke_sse_callback(callback: EventCallback, json: String, user_data: usize) {
    ffi_helpers::invoke_callback(callback, "sse_event", json, user_data)
}

fn invoke_sse_callback_if_active(id: usize, callback: EventCallback, json: String, user_data: usize) -> bool {
    ffi_helpers::invoke_callback_if_active(sse_guard(), id, callback, "sse_event", json, user_data)
}

// ── SseStream — one-shot POST SSE (OpenAI/Anthropic streaming API) ──

/// One-shot SSE stream. Consumes the SSE response and invokes the callback
/// for each content line, then sends a "done" event.
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_stream(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    headers_json: *const c_char,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let ud = user_data as usize;

    let method_str = ffi_helpers::ffi_str(method, "GET");
    let url_str = ffi_helpers::ffi_str(url, "/");
    let body_data = unsafe { ffi_helpers::read_body_bytes(body, body_len) };
    let headers = unsafe { ffi_helpers::parse_headers_json(headers_json) };

    let sse_method = match method_str.to_uppercase().as_str() {
        "GET" => SseMethod::GET,
        _ => SseMethod::POST,
    };

    let config = SseClientConfig {
        url: url_str,
        method: sse_method,
        headers,
        body: if body_data.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(&body_data).to_string())
        },
        reconnect: None,
        timeout_ms: 30_000,
        circuit_breaker: None,
    };

    let callback_ptr = callback;

    sse_runtime().spawn(async move {
        use tokio_stream::StreamExt;
        match SseStream::connect(config).await {
            Ok(mut stream) => {
                // Send "open" event first
                let open_json = build_sse_event_json("open", "");
                invoke_sse_callback(callback_ptr, open_json, ud);

                while let Some(line_result) = stream.next().await {
                    match line_result {
                        Ok(line) => {
                            let json = build_sse_event_json("data", &line);
                            invoke_sse_callback(callback_ptr, json, ud);
                        }
                        Err(e) => {
                            let err_json = build_sse_event_json("error", &e.to_string());
                            invoke_sse_callback(callback_ptr, err_json, ud);
                            return;
                        }
                    }
                }
                // Stream exhausted — send done
                let done_json = build_sse_event_json("close", "");
                invoke_sse_callback(callback_ptr, done_json, ud);
            }
            Err(e) => {
                let err_json = build_sse_event_json("error", &e.to_string());
                invoke_sse_callback(callback_ptr, err_json, ud);
            }
        }
    });
}

// ── SseClient — persistent SSE with auto-reconnect ──

/// Create a persistent SSE client with auto-reconnect.
/// Returns an opaque handle (pointer to id), or null on failure.
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_connect(
    config_json: *const c_char,
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void {
    if config_json.is_null() {
        return std::ptr::null_mut();
    }
    let json = CStr::from_ptr(config_json);
    let config: SseClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };

    let ud = user_data as usize;
    let callback_ptr = event_callback;

    let handle = std::thread::spawn(move || {
        // 使用独立辅助线程的 runtime，避免在 tokio 上下文内 block_on 导致 panic
        static AUX_RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        let rt = AUX_RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create aux runtime for SSE connect")
        });
        rt.block_on(async move {
            match SseClient::connect(config).await {
                Ok(client) => {
                    // Send open event
                    let open_json = build_sse_event_json("open", "");
                    invoke_sse_callback(callback_ptr, open_json, ud);

                    // Store client for later operations (close, ready_state, etc.)
                    let client_arc = Arc::new(TokioMutex::new(client));
                    let id = SSE_REGISTRY.insert(client_arc.clone());

                    // Spawn background task to forward SSE lines to callback
                    sse_runtime().spawn(async move {
                        loop {
                            // destroy 后停止转发，避免回调已释放的 user_data（issue #034）。
                            if sse_guard().is_cancelled(id) {
                                break;
                            }
                            let mut c = client_arc.lock().await;
                            let line_result = c.next_line().await;
                            drop(c); // release lock before invoking callback
                            match line_result {
                                Some(Ok(line)) => {
                                    let json = build_sse_event_json("data", &line);
                                    if !invoke_sse_callback_if_active(id, callback_ptr, json, ud) {
                                        break;
                                    }
                                }
                                Some(Err(e)) => {
                                    let err_json = build_sse_event_json("error", &e.to_string());
                                    if !invoke_sse_callback_if_active(id, callback_ptr, err_json, ud)
                                    {
                                        break;
                                    }
                                }
                                None => {
                                    let done_json = build_sse_event_json("close", "");
                                    let _ = invoke_sse_callback_if_active(
                                        id,
                                        callback_ptr,
                                        done_json,
                                        ud,
                                    );
                                    break;
                                }
                            }
                        }
                    });

                    id
                }
                Err(_) => 0usize,
            }
        })
    });

    match handle.join() {
        Ok(ptr) if ptr != 0 => ptr as *mut c_void,
        _ => std::ptr::null_mut(),
    }
}

/// Get the ready state of an SSE client.
/// Returns: 0=Connecting, 1=Open, 2=Closed. Returns -1 if handle is invalid.
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_ready_state(sse_handle: *mut c_void) -> i32 {
    if sse_handle.is_null() {
        return -1;
    }
    let id = sse_handle as usize;
    SSE_REGISTRY
        .get(id)
        .map(|client| match client.blocking_lock().ready_state() {
            SseReadyState::Connecting => 0,
            SseReadyState::Open => 1,
            SseReadyState::Closed => 2,
        })
        .unwrap_or(-1)
}

/// Get the last event ID of an SSE client.
/// Returns a C string (caller must free via catcher_free_data), or null if handle is invalid.
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_last_event_id(sse_handle: *mut c_void) -> *mut c_char {
    if sse_handle.is_null() {
        return std::ptr::null_mut();
    }
    let id = sse_handle as usize;
    SSE_REGISTRY
        .get(id)
        .map(|client| {
            let last_id = client.blocking_lock().last_event_id();
            CString::new(last_id).unwrap_or_default().into_raw()
        })
        .unwrap_or(std::ptr::null_mut())
}

/// Close an SSE client (stops reconnection).
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_close(sse_handle: *mut c_void) {
    if sse_handle.is_null() {
        return;
    }
    let id = sse_handle as usize;
    if let Some(client) = SSE_REGISTRY.get(id) {
        client.blocking_lock().close();
    }
}

/// Destroy an SSE client handle.
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_destroy(sse_handle: *mut c_void) {
    if sse_handle.is_null() {
        return;
    }
    let id = sse_handle as usize;
    // 标记取消并关闭，阻止后台转发循环在 destroy 后再回调宿主 user_data（issue #034）。
    sse_guard().mark(id);
    if let Some(client) = SSE_REGISTRY.get(id) {
        client.blocking_lock().close();
    }
    SSE_REGISTRY.remove(id);
}
