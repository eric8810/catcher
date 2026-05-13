//! WebSocket C ABI — create / send / close / destroy

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex};

use crate::transport::ws_client::{WsHandle, WsTransport};
use crate::types::ws::WsClientConfig;

use catcher_core::{EventCallback, FfiResult, FfiString};

static WS_HANDLES: Mutex<Option<HashMap<usize, Arc<WsHandle>>>> = Mutex::new(None);
static WS_NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn ws_handles() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Arc<WsHandle>>>> {
    WS_HANDLES.lock().unwrap()
}

/// Safely read an FfiString as a Rust String. Returns default on null/invalid.
fn ffi_string_to_string(s: FfiString, default: &str) -> String {
    if s.data.is_null() || s.len == 0 {
        return default.to_string();
    }
    unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(s.data as *const u8, s.len))
            .unwrap_or(default)
            .to_string()
    }
}

/// Build a JSON error string safely (no format! injection).
fn error_json(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

/// Invoke an FFI event callback with ownership-transferred CStrings.
fn invoke_event_callback(
    cb: EventCallback,
    event_name: &str,
    json: String,
    user_data: usize,
) {
    let c_event = CString::new(event_name.replace('\0', "")).unwrap_or_default();
    let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
    let json_len = c_json.as_bytes().len();

    cb(
        c_event.into_raw(),
        c_json.into_raw() as *const u8,
        json_len,
        user_data as *mut c_void,
    );
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_create(
    config_json: *const c_char,
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void {
    if config_json.is_null() {
        return std::ptr::null_mut();
    }
    let json = CStr::from_ptr(config_json);
    let config: WsClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let urls = config.urls.clone();
    let first_url = match urls.first().cloned() {
        Some(u) => u,
        None => {
            // Return error via callback instead of silently connecting to localhost
            let json = error_json("urls cannot be empty");
            invoke_event_callback(event_callback, "ws_error", json, user_data as usize);
            return std::ptr::null_mut();
        }
    };

    let id = WS_NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let cb = event_callback;
    let ud = user_data as usize;

    tokio::task::spawn(async move {
        match WsTransport::connect(&first_url, &config).await {
            Ok((handle, mut rx)) => {
                let ws_handle = Arc::new(handle);
                ws_handles()
                    .get_or_insert_with(HashMap::new)
                    .insert(id, ws_handle);

                while let Some(event) = rx.recv().await {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    invoke_event_callback(cb, "ws_event", json, ud);
                }
            }
            Err(e) => {
                let json = error_json(&e.to_string());
                invoke_event_callback(cb, "ws_error", json, ud);
            }
        }
    });

    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_send_text(handle: *mut c_void, message: FfiString) -> FfiResult {
    if handle.is_null() {
        return FfiResult::error(1, "null handle");
    }
    let id = *(handle as *const usize);
    let text = ffi_string_to_string(message, "");
    let handles = ws_handles();
    if let Some(ref map) = *handles {
        if let Some(h) = map.get(&id) {
            match h.send_text(&text) {
                Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
                Err(e) => FfiResult::error(1, &e.to_string()),
            }
        } else {
            FfiResult::error(1, "handle not found")
        }
    } else {
        FfiResult::error(1, "no handles")
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_send_binary(
    handle: *mut c_void,
    data: *const u8,
    len: usize,
) -> FfiResult {
    if handle.is_null() {
        return FfiResult::error(1, "null handle");
    }
    if data.is_null() {
        return FfiResult::error(1, "null data pointer");
    }
    let id = *(handle as *const usize);
    let bytes = std::slice::from_raw_parts(data, len);
    let handles = ws_handles();
    if let Some(ref map) = *handles {
        if let Some(h) = map.get(&id) {
            match h.send_binary(bytes) {
                Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
                Err(e) => FfiResult::error(1, &e.to_string()),
            }
        } else {
            FfiResult::error(1, "handle not found")
        }
    } else {
        FfiResult::error(1, "no handles")
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_close(handle: *mut c_void, code: u16, reason: FfiString) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    let reason_str = ffi_string_to_string(reason, "normal");
    let handles = ws_handles();
    if let Some(ref map) = *handles {
        if let Some(h) = map.get(&id) {
            let _ = h.close(code, &reason_str);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    ws_handles().as_mut().map(|m| m.remove(&id));
    drop(Box::from_raw(handle as *mut usize));
}

/// Free an FfiResult returned by WS FFI functions.
///
/// Dart must call this after every WS FFI call that returns FfiResult.
/// Takes ownership — Drop impl handles freeing error_message CString.
#[no_mangle]
pub extern "C" fn catcher_free_result(result: FfiResult) {
    drop(result);
}
