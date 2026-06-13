//! WebSocket C ABI — create / send / close / destroy
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;

use crate::transport::ws_client::{WsHandle, WsTransport};
use crate::types::ws::WsClientConfig;

use catcher_core::{EventCallback, FfiResult, FfiString, HandleRegistry};

static WS_REGISTRY: HandleRegistry<WsHandle> = HandleRegistry::new();

/// Global tokio runtime for WS async operations (spawning, etc.)
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-ws FFI")
    })
}

/// Safely read an FfiString as a Rust String. Returns default on null/invalid.
fn ffi_string_to_string(s: FfiString, default: &str) -> String {
    s.to_string_lossy(default)
}

/// Build a JSON error string safely (no format! injection).
fn error_json(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

/// Invoke an FFI event callback with ownership-transferred CStrings.
fn invoke_event_callback(cb: EventCallback, event_name: &str, json: String, user_data: usize) {
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

/// 创建 WS 客户端。返回的句柄是注册表 id 直接编码为指针值（id ≥ 1，
/// 与 null 不冲突），**不指向任何内存** — destroy 之后用旧句柄调用任何
/// API 都安全返回 "handle not found"，而不是 use-after-free。
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
    if config.urls.is_empty() {
        // Return error via callback instead of silently connecting to localhost
        let json = error_json("urls cannot be empty");
        invoke_event_callback(event_callback, "ws_error", json, user_data as usize);
        return std::ptr::null_mut();
    }

    let id = WS_REGISTRY.next_id();
    let cb = event_callback;
    let ud = user_data as usize;

    runtime().spawn(async move {
        match WsTransport::connect(&config).await {
            Ok((handle, mut rx)) => {
                WS_REGISTRY.insert_with_id(id, Arc::new(handle));

                while let Some(event) = rx.recv().await {
                    let json = event.to_ffi_json();
                    invoke_event_callback(cb, "ws_event", json, ud);
                }
            }
            Err(e) => {
                let json = error_json(&e.to_string());
                invoke_event_callback(cb, "ws_error", json, ud);
            }
        }
    });

    id as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_send_text(
    handle: *mut c_void,
    message: FfiString,
) -> FfiResult {
    if handle.is_null() {
        return FfiResult::error(1, "null handle");
    }
    let id = handle as usize;
    let text = ffi_string_to_string(message, "");
    match WS_REGISTRY.get(id) {
        Some(h) => match h.send_text(&text) {
            Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
            Err(e) => FfiResult::error(1, &e.to_string()),
        },
        None => FfiResult::error(1, "handle not found"),
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
    let id = handle as usize;
    let bytes = std::slice::from_raw_parts(data, len);
    match WS_REGISTRY.get(id) {
        Some(h) => match h.send_binary(bytes) {
            Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
            Err(e) => FfiResult::error(1, &e.to_string()),
        },
        None => FfiResult::error(1, "handle not found"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_close(handle: *mut c_void, code: u16, reason: FfiString) {
    if handle.is_null() {
        return;
    }
    let id = handle as usize;
    let reason_str = ffi_string_to_string(reason, "normal");
    if let Some(h) = WS_REGISTRY.get(id) {
        let _ = h.close(code, &reason_str);
    }
}

/// 通知 WS 客户端网络环境已变化（WiFi 切换 / VPN 换节点等）。
/// 立即断开当前连接、清空 DNS 缓存、跳过退避延迟重连。
#[no_mangle]
pub unsafe extern "C" fn catcher_ws_network_changed(handle: *mut c_void) -> FfiResult {
    if handle.is_null() {
        return FfiResult::error(1, "null handle");
    }
    let id = handle as usize;
    match WS_REGISTRY.get(id) {
        Some(h) => match h.network_changed() {
            Ok(()) => FfiResult::ok(std::ptr::null_mut(), 0),
            Err(e) => FfiResult::error(1, &e.to_string()),
        },
        None => FfiResult::error(1, "handle not found"),
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_ws_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let id = handle as usize;
    WS_REGISTRY.remove(id);
}

// Note: catcher_free_result is provided by catcher-core (ffi_types.rs).
// Do not re-define here to avoid duplicate symbol errors at link time.
