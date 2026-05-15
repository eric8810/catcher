//! HTTP C ABI — create / get / post / execute / destroy / cancel (N-03)

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;

use crate::transport::http_client::HttpTransport;
use crate::types::http::{HttpClientConfig, HttpMethod, HttpRequest};

use catcher_core::{EventCallback, FfiString};

static HANDLES: std::sync::Mutex<Option<HashMap<usize, Arc<HttpTransport>>>> = std::sync::Mutex::new(None);
static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-http FFI")
    })
}

fn handles() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Arc<HttpTransport>>>> {
    HANDLES.lock().unwrap()
}

fn ffi_string_to_string(s: FfiString, default: &str) -> String {
    s.to_string_lossy(default)
}

fn read_body_bytes(body: *const u8, body_len: usize) -> Vec<u8> {
    if body.is_null() || body_len == 0 { Vec::new() }
    else { unsafe { std::slice::from_raw_parts(body, body_len).to_vec() } }
}

fn error_json(msg: &str) -> String {
    serde_json::json!({"error": msg}).to_string()
}

fn parse_headers_json(headers_json: *const c_char) -> HashMap<String, String> {
    if headers_json.is_null() { return HashMap::new(); }
    let json_str = unsafe {
        match CStr::from_ptr(headers_json).to_str() {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        }
    };
    if json_str.is_empty() { return HashMap::new(); }
    serde_json::from_str::<HashMap<String, String>>(json_str).unwrap_or_default()
}

fn invoke_http_callback(
    callback: EventCallback, event_name: &str, json: String, user_data: usize,
) {
    let c_event = CString::new(event_name.replace('\0', "")).unwrap_or_default();
    let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
    let json_len = c_json.as_bytes().len();
    callback(c_event.into_raw(), c_json.into_raw() as *const u8, json_len, user_data as *mut c_void);
}

// ── Lifecycle ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_create(config_json: *const c_char) -> *mut c_void {
    if config_json.is_null() { return std::ptr::null_mut(); }
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
    handles().get_or_insert_with(HashMap::new).insert(id, Arc::new(transport));
    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_destroy(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    handles().as_mut().map(|m| m.remove(&id));
    drop(Box::from_raw(handle as *mut usize));
}

// ── Original FFI (unchanged signatures — backward compatible) ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_get(
    handle: *mut c_void, url: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    let url_str = ffi_string_to_string(url, "/");
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;
    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: HttpMethod::GET, url: url_str,
                headers: per_request_headers, body: None,
                content_type: None, timeout_ms: per_request_timeout,
            };
            let result = t.execute(request).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => error_json(&e.to_string()),
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_post(
    handle: *mut c_void, url: FfiString,
    body: *const u8, body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    let url_str = ffi_string_to_string(url, "/");
    let body_data = read_body_bytes(body, body_len);
    let ct_str = ffi_string_to_string(content_type, "application/octet-stream");
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;
    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: HttpMethod::POST, url: url_str,
                headers: per_request_headers, body: Some(body_data),
                content_type: Some(ct_str), timeout_ms: per_request_timeout,
            };
            let result = t.execute(request).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => error_json(&e.to_string()),
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString, url: FfiString,
    body: *const u8, body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    let method_str = ffi_string_to_string(method, "GET");
    let url_str = ffi_string_to_string(url, "/");
    let body_data = if !body.is_null() && body_len > 0 {
        Some(read_body_bytes(body, body_len))
    } else { None };
    let ct_str = {
        let s = ffi_string_to_string(content_type, "");
        if s.is_empty() { None } else { Some(s) }
    };
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;
    let http_method = match method_str.to_uppercase().as_str() {
        "GET" => HttpMethod::GET, "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT, "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        other => {
            invoke_http_callback(callback, "http_result",
                error_json(&format!("Unsupported HTTP method: {other}")), ud);
            return;
        }
    };
    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: http_method, url: url_str,
                headers: per_request_headers, body: body_data,
                content_type: ct_str, timeout_ms: per_request_timeout,
            };
            let result = t.execute(request).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => error_json(&e.to_string()),
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}

// ── N-03: Per-request cancel (new C ABI symbols) ──

/// Like catcher_http_execute but returns request_id synchronously.
/// The result is delivered via callback with extra "request_id" field.
/// Use catcher_http_cancel_request(handle, request_id) to cancel this request.
/// Returns 0 on error.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute_with_id(
    handle: *mut c_void,
    method: FfiString, url: FfiString,
    body: *const u8, body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) -> u64 {
    if handle.is_null() { return 0; }
    let id = *(handle as *const usize);
    let method_str = ffi_string_to_string(method, "GET");
    let url_str = ffi_string_to_string(url, "/");
    let body_data = if !body.is_null() && body_len > 0 {
        Some(read_body_bytes(body, body_len))
    } else { None };
    let ct_str = {
        let s = ffi_string_to_string(content_type, "");
        if s.is_empty() { None } else { Some(s) }
    };
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;
    let http_method = match method_str.to_uppercase().as_str() {
        "GET" => HttpMethod::GET, "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT, "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        other => {
            invoke_http_callback(callback, "http_result",
                error_json(&format!("Unsupported HTTP method: {other}")), ud);
            return 0;
        }
    };
    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        let (request_id, per_request_token) = t.allocate_pending_request();
        runtime().spawn(async move {
            let request = HttpRequest {
                method: http_method, url: url_str,
                headers: per_request_headers, body: body_data,
                content_type: ct_str, timeout_ms: per_request_timeout,
            };
            let (_rid, result) = t.execute_with_token(request_id, per_request_token, request).await;
            let json = match result {
                Ok(resp) => {
                    serde_json::json!({
                        "status": resp.status,
                        "headers": resp.headers,
                        "body": resp.body,
                        "elapsed_ms": resp.elapsed_ms,
                        "request_id": request_id,
                    }).to_string()
                }
                Err(e) => {
                    serde_json::json!({"error": e.to_string(), "request_id": request_id}).to_string()
                }
            };
            invoke_http_callback(callback, "http_result", json, ud);
        });
        request_id
    } else { 0 }
}

/// Cancel a single in-flight request (N-03). Returns 0 on success, -1 if not found.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_cancel_request(
    handle: *mut c_void, request_id: u64,
) -> i32 {
    if handle.is_null() { return -1; }
    let id = *(handle as *const usize);
    if let Some(transport) = handles().as_ref().and_then(|m| m.get(&id)) {
        if transport.cancel_request(request_id) { 0 } else { -1 }
    } else { -1 }
}

// ── Runtime control (unchanged) ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_circuit_breaker_state(handle: *mut c_void) -> *mut c_char {
    if handle.is_null() { return std::ptr::null_mut(); }
    let id = *(handle as *const usize);
    let state = handles().as_ref().and_then(|m| m.get(&id)).and_then(|t| t.circuit_breaker_state());
    let json = match state {
        Some(s) => serde_json::to_string(&s).unwrap_or_default(),
        None => serde_json::json!({"state":"disabled"}).to_string(),
    };
    CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_metrics(handle: *mut c_void) -> *mut c_char {
    if handle.is_null() { return std::ptr::null_mut(); }
    let id = *(handle as *const usize);
    let snapshot = handles().as_ref().and_then(|m| m.get(&id)).map(|t| t.metrics());
    let json = match snapshot {
        Some(s) => serde_json::to_string(&s).unwrap_or_default(),
        None => "{}".to_string(),
    };
    CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_cancel_all(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    if let Some(transport) = handles().as_ref().and_then(|m| m.get(&id)) {
        transport.cancel_all();
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_adaptive_timeout_config(
    handle: *mut c_void, enabled: i32,
    min_timeout_ms: u32, max_timeout_ms: u32,
    multiplier_scaled: u32, window_size: u32,
) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    if let Some(transport) = handles().as_ref().and_then(|m| m.get(&id)) {
        if enabled != 0 {
            transport.set_adaptive_timeout(
                min_timeout_ms as u64, max_timeout_ms as u64,
                multiplier_scaled as f64 / 1000.0, window_size as usize,
            );
        } else {
            transport.disable_adaptive_timeout();
        }
    }

}
// N-02: Streaming download C ABI
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute_stream(
    handle: *mut c_void,
    method: FfiString, url: FfiString,
    body: *const u8, body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) -> u64 {
    if handle.is_null() { return 0; }
    let id = *(handle as *const usize);
    let method_str = ffi_string_to_string(method, "GET");
    let url_str = ffi_string_to_string(url, "/");
    let body_data = if !body.is_null() && body_len > 0 {
        Some(read_body_bytes(body, body_len))
    } else { None };
    let ct_str = {
        let s = ffi_string_to_string(content_type, "");
        if s.is_empty() { None } else { Some(s) }
    };
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;
    let http_method = match method_str.to_uppercase().as_str() {
        "GET" => HttpMethod::GET, "POST" => HttpMethod::POST,
        "PUT" => HttpMethod::PUT, "DELETE" => HttpMethod::DELETE,
        "PATCH" => HttpMethod::PATCH,
        other => {
            invoke_http_callback(callback, "stream_error",
                serde_json::json!({"error": format!("Unsupported method: {other}")}).to_string(), ud);
            return 0;
        }
    };
    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        let (request_id, _pt) = t.allocate_pending_request();
        runtime().spawn(async move {
            let req = HttpRequest {
                method: http_method, url: url_str,
                headers: per_request_headers, body: body_data,
                content_type: ct_str, timeout_ms: per_request_timeout,
            };
            let _ = t.execute_stream(req, move |event| {
                let (et, ed) = match &event {
                    crate::types::http::StreamEvent::Headers { status, headers } => {
                        ("stream_headers", serde_json::json!({"status":status,"headers":headers,"request_id":request_id}).to_string())
                    }
                    crate::types::http::StreamEvent::Chunk(data) => {
                        ("stream_chunk", String::from_utf8_lossy(data).to_string())
                    }
                    crate::types::http::StreamEvent::Done => {
                        ("stream_done", serde_json::json!({"request_id":request_id}).to_string())
                    }
                    crate::types::http::StreamEvent::Error(msg) => {
                        ("stream_error", serde_json::json!({"error":msg,"request_id":request_id}).to_string())
                    }
                };
                invoke_http_callback(callback, et, ed, ud);
            }).await;
        });
        request_id
    } else { 0 }
}
