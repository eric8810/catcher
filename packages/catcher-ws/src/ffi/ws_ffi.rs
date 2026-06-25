//! WebSocket C ABI — create / send / close / destroy
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, OnceLock};

use crate::transport::ws_client::{WsHandle, WsTransport};
use crate::types::ws::WsClientConfig;

use catcher_core::ffi_helpers::{self, CancellationGuard};
use catcher_core::{EventCallback, FfiResult, FfiString, HandleRegistry};

static WS_REGISTRY: HandleRegistry<WsHandle> = HandleRegistry::new();
static WS_GUARD: OnceLock<CancellationGuard> = OnceLock::new();
fn ws_guard() -> &'static CancellationGuard { WS_GUARD.get_or_init(CancellationGuard::new) }

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("ws runtime"))
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_create(
    config_json: *const c_char, event_callback: EventCallback, user_data: *mut c_void,
) -> *mut c_void {
    if config_json.is_null() { return std::ptr::null_mut(); }
    let json = CStr::from_ptr(config_json);
    let config: WsClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c, Err(_) => return std::ptr::null_mut(),
    };
    if config.urls.is_empty() {
        ffi_helpers::invoke_callback(event_callback, "ws_error", ffi_helpers::error_json("urls cannot be empty"), user_data as usize);
        return std::ptr::null_mut();
    }
    let id = WS_REGISTRY.next_id();
    let cb = event_callback;
    let ud = user_data as usize;
    runtime().spawn(async move {
        match WsTransport::connect(&config).await {
            Ok((handle, mut rx)) => {
                if ws_guard().is_cancelled(id) { let _ = handle.close(1000, "destroy"); return; }
                WS_REGISTRY.insert_with_id(id, Arc::new(handle));
                while let Some(event) = rx.recv().await {
                    let json = event.to_ffi_json();
                    if !ffi_helpers::invoke_callback_if_active(ws_guard(), id, cb, "ws_event", json, ud) { break; }
                }
                WS_REGISTRY.remove(id);
            }
            Err(e) => {
                let _ = ffi_helpers::invoke_callback_if_active(ws_guard(), id, cb, "ws_error", ffi_helpers::error_json(&e.to_string()), ud);
            }
        }
    });
    id as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_send_text(handle: *mut c_void, message: FfiString) -> FfiResult {
    if handle.is_null() { return FfiResult::error(1, "null handle"); }
    let id = handle as usize;
    match WS_REGISTRY.get(id) {
        Some(h) => match h.send_text(&ffi_helpers::ffi_str(message, "")) {
            Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
            Err(e) => FfiResult::error(1, &e.to_string()),
        },
        None => FfiResult::error(1, "handle not found"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_send_binary(handle: *mut c_void, data: *const u8, len: usize) -> FfiResult {
    if handle.is_null() || data.is_null() { return FfiResult::error(1, "null pointer"); }
    let id = handle as usize;
    match WS_REGISTRY.get(id) {
        Some(h) => match h.send_binary(std::slice::from_raw_parts(data, len)) {
            Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
            Err(e) => FfiResult::error(1, &e.to_string()),
        },
        None => FfiResult::error(1, "handle not found"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_close(handle: *mut c_void, code: u16, reason: FfiString) {
    if handle.is_null() { return; }
    if let Some(h) = WS_REGISTRY.get(handle as usize) {
        let _ = h.close(code, &ffi_helpers::ffi_str(reason, "normal"));
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_network_changed(handle: *mut c_void) -> FfiResult {
    if handle.is_null() { return FfiResult::error(1, "null handle"); }
    match WS_REGISTRY.get(handle as usize) {
        Some(h) => match h.network_changed() { Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0), Err(e) => FfiResult::error(1, &e.to_string()) },
        None => FfiResult::error(1, "handle not found"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_destroy(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = handle as usize;
    ws_guard().mark(id);
    if let Some(h) = WS_REGISTRY.remove(id) { let _ = h.close(1000, "destroy"); }
}
