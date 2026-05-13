//! WebSocket C ABI — create / send / close / destroy
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, Mutex};

use crate::transport::ws_client::{WsHandle, WsTransport};
use crate::types::ws::WsClientConfig;

use catcher_core::{EventCallback, FfiResult, FfiString};

static WS_HANDLES: Mutex<Option<HashMap<usize, Arc<WsHandle>>>> = Mutex::new(None);
static WS_NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn ws_handles() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Arc<WsHandle>>>> {
    WS_HANDLES.lock().unwrap()
}

#[no_mangle]
pub extern "C" fn catcher_ws_create(
    config_json: *const c_char,
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void {
    if config_json.is_null() {
        return std::ptr::null_mut();
    }
    let json = unsafe { CStr::from_ptr(config_json) };
    let config: WsClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let urls = config.urls.clone();
    let first_url = urls
        .first()
        .cloned()
        .unwrap_or_else(|| "ws://localhost".into());

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

                // Forward events directly (not behind lock)
                while let Some(event) = rx.recv().await {
                    let json = serde_json::to_string(&event).unwrap_or_default();
                    let c_event = std::ffi::CString::new("ws_event").unwrap();
                    cb(
                        c_event.as_ptr(),
                        json.as_ptr(),
                        json.len(),
                        ud as *mut c_void,
                    );
                }
            }
            Err(e) => {
                let json = format!("{{\"error\":\"{e}\"}}");
                let c_event = std::ffi::CString::new("ws_error").unwrap();
                cb(
                    c_event.as_ptr(),
                    json.as_ptr(),
                    json.len(),
                    ud as *mut c_void,
                );
            }
        }
    });

    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub extern "C" fn catcher_ws_send_text(handle: *mut c_void, message: FfiString) -> FfiResult {
    if handle.is_null() {
        return FfiResult::error(1, "null handle");
    }
    let id = unsafe { *(handle as *const usize) };
    let text = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(
            message.data as *const u8,
            message.len,
        ))
        .unwrap_or("")
    };
    let handles = ws_handles();
    if let Some(ref map) = *handles {
        if let Some(h) = map.get(&id) {
            match h.send_text(text) {
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
pub extern "C" fn catcher_ws_send_binary(
    handle: *mut c_void,
    data: *const u8,
    len: usize,
) -> FfiResult {
    if handle.is_null() {
        return FfiResult::error(1, "null handle");
    }
    let id = unsafe { *(handle as *const usize) };
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
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
pub extern "C" fn catcher_ws_close(handle: *mut c_void, code: u16, reason: FfiString) {
    if handle.is_null() {
        return;
    }
    let id = unsafe { *(handle as *const usize) };
    let reason_str = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(
            reason.data as *const u8,
            reason.len,
        ))
        .unwrap_or("normal")
    };
    let handles = ws_handles();
    if let Some(ref map) = *handles {
        if let Some(h) = map.get(&id) {
            let _ = h.close(code, reason_str);
        }
    }
}

#[no_mangle]
pub extern "C" fn catcher_ws_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let id = unsafe { *(handle as *const usize) };
    ws_handles().as_mut().map(|m| m.remove(&id));
    unsafe {
        drop(Box::from_raw(handle as *mut usize));
    }
}

/// Free an FfiResult returned by WS FFI functions.
///
/// Dart must call this after every WS FFI call that returns FfiResult.
/// Takes ownership of the FfiResult — the Drop impl handles freeing
/// the error_message CString automatically.
#[no_mangle]
pub extern "C" fn catcher_free_result(result: FfiResult) {
    // Just take ownership and let Drop handle cleanup.
    // Do NOT manually free error_message here — Drop impl already does it,
    // and doing both would be a double-free.
    drop(result);
}
