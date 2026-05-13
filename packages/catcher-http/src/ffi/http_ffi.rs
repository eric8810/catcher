//! HTTP C ABI — create / get / post / execute / destroy

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex};

use crate::transport::http_client::HttpTransport;
use crate::types::http::{HttpClientConfig, HttpMethod, HttpRequest};

use catcher_core::{EventCallback, FfiString};

static HANDLES: Mutex<Option<HashMap<usize, Arc<HttpTransport>>>> = Mutex::new(None);
static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn handles() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Arc<HttpTransport>>>> {
    HANDLES.lock().unwrap()
}

/// Invoke an FFI event callback with ownership-transferred CStrings.
///
/// Both strings are leaked via `CString::into_raw()`. The Dart side MUST
/// call `catcher_free_event_data()` after reading to reclaim memory.
fn invoke_http_callback(
    callback: EventCallback,
    event_name: &str,
    json: String,
    user_data: usize,
) {
    let c_event = CString::new(event_name).unwrap();
    let c_json = CString::new(json).unwrap();
    let json_len = c_json.as_bytes().len();

    callback(
        c_event.into_raw(),
        c_json.into_raw() as *const u8,
        json_len,
        user_data as *mut c_void,
    );
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_create(config_json: *const c_char) -> *mut c_void {
    if config_json.is_null() {
        return std::ptr::null_mut();
    }
    let json = CStr::from_ptr(config_json);
    let config: HttpClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let transport = match HttpTransport::new(config) {
        Ok(t) => t,
        Err(_) => return std::ptr::null_mut(),
    };
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    handles()
        .get_or_insert_with(HashMap::new)
        .insert(id, Arc::new(transport));
    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_get(
    handle: *mut c_void,
    url: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    let url_str = std::str::from_utf8(std::slice::from_raw_parts(url.data as *const u8, url.len))
        .unwrap_or("/")
        .to_string();
    let ud = user_data as usize;

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        tokio::task::spawn(async move {
            let result = t.get(&url_str).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_post(
    handle: *mut c_void,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    let url_str = std::str::from_utf8(std::slice::from_raw_parts(url.data as *const u8, url.len))
        .unwrap_or("/")
        .to_string();
    let body_data = std::slice::from_raw_parts(body, body_len).to_vec();
    let ct_str = std::str::from_utf8(std::slice::from_raw_parts(
        content_type.data as *const u8,
        content_type.len,
    ))
    .unwrap_or("application/octet-stream")
    .to_string();
    let ud = user_data as usize;

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        tokio::task::spawn(async move {
            let result = t.post(&url_str, &body_data, &ct_str).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}

/// Generic HTTP request — accepts method as a string (\"GET\", \"POST\", \"PUT\", \"DELETE\", \"PATCH\").
/// This is the preferred entry point for FFI consumers that need all HTTP methods.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);

    let method_str = std::str::from_utf8(std::slice::from_raw_parts(
        method.data as *const u8,
        method.len,
    ))
    .unwrap_or("GET")
    .to_string();
    let url_str = std::str::from_utf8(std::slice::from_raw_parts(url.data as *const u8, url.len))
        .unwrap_or("/")
        .to_string();
    let body_data = if !body.is_null() && body_len > 0 {
        Some(std::slice::from_raw_parts(body, body_len).to_vec())
    } else {
        None
    };
    let ct_str = if !content_type.data.is_null() && content_type.len > 0 {
        Some(
            std::str::from_utf8(std::slice::from_raw_parts(
                content_type.data as *const u8,
                content_type.len,
            ))
            .unwrap_or("application/json")
            .to_string(),
        )
    } else {
        None
    };
    let ud = user_data as usize;

    let http_method = match method_str.to_uppercase().as_str() {
        "GET" => HttpMethod::GET,
        "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT,
        "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        _ => HttpMethod::GET,
    };

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        tokio::task::spawn(async move {
            let request = HttpRequest {
                method: http_method,
                url: url_str,
                headers: Default::default(),
                body: body_data,
                content_type: ct_str,
                timeout_ms: None,
            };
            let result = t.execute(request).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    handles().as_mut().map(|m| m.remove(&id));
    drop(Box::from_raw(handle as *mut usize));
}
