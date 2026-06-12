//! HTTP C ABI — create / get / post / execute / destroy / cancel (N-03)
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Arc;

use crate::transport::http_client::HttpTransport;
use crate::types::http::{HttpClientConfig, HttpMethod, HttpRequest};

use catcher_core::{EventCallback, FfiString, HandleRegistry};

/// 将 HttpResponse 序列化为 JSON，body 使用 base64 编码以避免 Vec<u8> 展开为数字数组（~5x 膨胀）。
fn response_to_json(resp: &crate::types::http::HttpResponse) -> String {
    use base64::Engine;
    let body_b64 = base64::engine::general_purpose::STANDARD.encode(&resp.body);
    serde_json::json!({
        "status": resp.status,
        "headers": resp.headers,
        "body_base64": body_b64,
        "elapsed_ms": resp.elapsed_ms,
    }).to_string()
}

/// 将 HTTP 执行结果序列化为 FFI JSON。
/// `CatcherError::HttpError`（4xx/5xx）转为正常 response JSON，调用方可读取 status code。
fn http_result_to_json(result: Result<crate::types::http::HttpResponse, catcher_core::CatcherError>) -> String {
    match result {
        Ok(resp) => response_to_json(&resp),
        Err(catcher_core::CatcherError::HttpError { status, body }) => {
            use base64::Engine;
            let body_b64 = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
            serde_json::json!({
                "status": status,
                "headers": {},
                "body_base64": body_b64,
                "elapsed_ms": 0u64,
            }).to_string()
        }
        Err(e) => error_json(&e.to_string()),
    }
}

static REGISTRY: HandleRegistry<HttpTransport> = HandleRegistry::new();

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-http FFI")
    })
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
    let id = REGISTRY.insert(Arc::new(transport));
    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_destroy(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    REGISTRY.remove(id);
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
    let transport = REGISTRY.get(id);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: HttpMethod::GET, url: url_str,
                headers: per_request_headers, body: None,
                content_type: None, timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let result = t.execute(request).await;
            let json = http_result_to_json(result);
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
    let transport = REGISTRY.get(id);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: HttpMethod::POST, url: url_str,
                headers: per_request_headers, body: Some(body_data),
                content_type: Some(ct_str), timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let result = t.execute(request).await;
            let json = http_result_to_json(result);
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
    let transport = REGISTRY.get(id);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: http_method, url: url_str,
                headers: per_request_headers, body: body_data,
                content_type: ct_str, timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let result = t.execute(request).await;
            let json = http_result_to_json(result);
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
    let transport = REGISTRY.get(id);
    if let Some(t) = transport {
        let (request_id, per_request_token) = t.allocate_pending_request();
        runtime().spawn(async move {
            let request = HttpRequest {
                method: http_method, url: url_str,
                headers: per_request_headers, body: body_data,
                content_type: ct_str, timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let (_rid, result) = t.execute_with_token(request_id, per_request_token, request).await;
            let json = match result {
                Ok(resp) => {
                    use base64::Engine;
                    let body_b64 = base64::engine::general_purpose::STANDARD.encode(&resp.body);
                    serde_json::json!({
                        "status": resp.status,
                        "headers": resp.headers,
                        "body_base64": body_b64,
                        "elapsed_ms": resp.elapsed_ms,
                        "request_id": request_id,
                    }).to_string()
                }
                Err(catcher_core::CatcherError::HttpError { status, body }) => {
                    use base64::Engine;
                    let body_b64 = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
                    serde_json::json!({
                        "status": status,
                        "headers": {},
                        "body_base64": body_b64,
                        "elapsed_ms": 0u64,
                        "request_id": request_id,
                    }).to_string()
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("cancelled") {
                        serde_json::json!({"type": "cancelled", "request_id": request_id}).to_string()
                    } else {
                        serde_json::json!({"error": msg, "request_id": request_id}).to_string()
                    }
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
    if let Some(transport) = REGISTRY.get(id) {
        if transport.cancel_request(request_id) { 0 } else { -1 }
    } else { -1 }
}

// ── Runtime control (unchanged) ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_circuit_breaker_state(handle: *mut c_void) -> *mut c_char {
    if handle.is_null() { return std::ptr::null_mut(); }
    let id = *(handle as *const usize);
    let state = REGISTRY.get(id).and_then(|t| t.circuit_breaker_state());
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
    let snapshot = REGISTRY.get(id).map(|t| t.metrics());
    let json = match snapshot {
        Some(s) => serde_json::to_string(&s).unwrap_or_default(),
        None => "{}".to_string(),
    };
    CString::new(json).unwrap_or_default().into_raw()
}

/// 通知 HTTP 客户端网络环境已变化（WiFi 切换 / VPN 换节点等）。
/// 清空 DNS 缓存、重建连接池（丢弃半开连接）、重置熔断器。
/// 返回 0 成功，1 句柄无效，2 重建失败。
#[no_mangle]
pub unsafe extern "C" fn catcher_http_network_changed(handle: *mut c_void) -> i32 {
    if handle.is_null() { return 1; }
    let id = *(handle as *const usize);
    match REGISTRY.get(id) {
        Some(transport) => match transport.network_changed() {
            Ok(()) => 0,
            Err(_) => 2,
        },
        None => 1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_cancel_all(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = *(handle as *const usize);
    if let Some(transport) = REGISTRY.get(id) {
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
    if let Some(transport) = REGISTRY.get(id) {
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
    let transport = REGISTRY.get(id);
    if let Some(t) = transport {
        let (request_id, pt) = t.allocate_pending_request();
        runtime().spawn(async move {
            let req = HttpRequest {
                method: http_method, url: url_str,
                headers: per_request_headers, body: body_data,
                content_type: ct_str, timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let _ = t.execute_stream(req, pt, move |event| {
                let (et, ed) = match &event {
                    crate::types::http::StreamEvent::Headers { status, headers } => {
                        ("stream_headers", serde_json::json!({"status":status,"headers":headers,"request_id":request_id}).to_string())
                    }
                    crate::types::http::StreamEvent::Chunk(data) => {
                        use base64::Engine;
                        let data_b64 = base64::engine::general_purpose::STANDARD.encode(data);
                        ("stream_chunk", serde_json::json!({
                            "data_base64": data_b64,
                            "request_id": request_id,
                        }).to_string())
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

// ── Multipart upload (B-02) ──────────────────────────────────

/// Multipart part descriptor (FFI-safe).
/// `data` points to bytes of length `data_len`.
/// For text fields, `filename` and `content_type` may be null.
/// For file/binary fields, `content_type` should be set.
#[repr(C)]
pub struct FfiMultipartPart {
    pub name: *const c_char,
    pub value_or_filename: *const c_char,  // text value OR filename
    pub content_type: *const c_char,        // null for text parts
    pub data: *const u8,                     // null for text parts
    pub data_len: usize,
}

/// Upload a multipart/form-data request.
///
/// `parts` is an array of `FfiMultipartPart` with `parts_count` elements.
/// Text parts: set `name` + `value_or_filename`, leave `data`/`content_type` null.
/// File parts: set `name` + `value_or_filename` (filename) + `content_type` + `data`/`data_len`.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_multipart(
    handle: *mut c_void,
    method: *const c_char,       // "POST" or "PUT"
    url: FfiString,
    parts: *const FfiMultipartPart,
    parts_count: usize,
    headers_json: *const c_char,
    timeout_ms: u64,
    callback: Option<EventCallback>,
    user_data: usize,
) {
    if handle.is_null() { return; }
    let callback = match callback {
        Some(cb) => cb,
        None => return,
    };
    let id_val = *(handle as *const usize);

    let method_str = if method.is_null() {
        HttpMethod::POST
    } else {
        match CStr::from_ptr(method).to_str().unwrap_or("POST") {
            "PUT" => HttpMethod::PUT,
            "PATCH" => HttpMethod::PATCH,
            _ => HttpMethod::POST,
        }
    };

    let url_str = ffi_string_to_string(url, "");
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms) } else { None };

    // Build multipart form from FFI parts array
    let mut form = crate::transport::multipart::MultipartForm::new();
    if !parts.is_null() && parts_count > 0 {
        let parts_slice = std::slice::from_raw_parts(parts, parts_count);
        for part in parts_slice {
            let name = if part.name.is_null() { continue; }
            else { CStr::from_ptr(part.name).to_str().unwrap_or("").to_string() };
            if name.is_empty() { continue; }

            if part.data.is_null() || part.data_len == 0 {
                // Text field
                let value = if part.value_or_filename.is_null() { String::new() }
                else { CStr::from_ptr(part.value_or_filename).to_str().unwrap_or("").to_string() };
                form = form.text(name, value);
            } else {
                // Binary/file field
                let data = std::slice::from_raw_parts(part.data, part.data_len).to_vec();
                let filename = if part.value_or_filename.is_null() { "file".to_string() }
                else { CStr::from_ptr(part.value_or_filename).to_str().unwrap_or("file").to_string() };
                let ct = if part.content_type.is_null() { "application/octet-stream".to_string() }
                else { CStr::from_ptr(part.content_type).to_str().unwrap_or("application/octet-stream").to_string() };
                form = form.file(name, filename, ct, data);
            }
        }
    }

    let ud = user_data;
    let transport = REGISTRY.get(id_val);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: method_str,
                url: url_str,
                headers: per_request_headers,
                multipart: Some(form),
                timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let result = t.execute(request).await;
            let json = http_result_to_json(result);
            invoke_http_callback(callback, "http_result", json, ud);
        });
    }
}
