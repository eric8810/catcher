//! HTTP C ABI — create / get / post / execute / destroy / cancel / stream / multipart
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, OnceLock};

use crate::transport::http_client::HttpTransport;
use crate::types::http::{HttpClientConfig, HttpMethod, HttpRequest};

use catcher_core::ffi_helpers::{self, CancellationGuard};
use catcher_core::{EventCallback, FfiString, HandleRegistry};

static REGISTRY: HandleRegistry<HttpTransport> = HandleRegistry::new();
static HTTP_GUARD: OnceLock<CancellationGuard> = OnceLock::new();
fn http_guard() -> &'static CancellationGuard { HTTP_GUARD.get_or_init(CancellationGuard::new) }

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-http FFI")
    })
}

// ── 通用辅助 ──

fn http_result_to_json(
    result: Result<&crate::types::http::HttpResponse, &catcher_core::CatcherError>,
    request_id: Option<u64>,
) -> String {
    use base64::Engine;
    let (status, headers, body_b64, elapsed_ms) = match result {
        Ok(resp) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&resp.body);
            (resp.status, &resp.headers, b64, resp.elapsed_ms)
        }
        Err(catcher_core::CatcherError::HttpError { status, body }) => {
            let b64 = base64::engine::general_purpose::STANDARD.encode(body.as_bytes());
            (*status, &HashMap::new(), b64, 0u64)
        }
        Err(e) => {
            let msg = e.to_string();
            return if msg.contains("cancelled") {
                serde_json::json!({"type": "cancelled", "request_id": request_id}).to_string()
            } else {
                let mut obj = serde_json::json!({"error": msg});
                if let Some(rid) = request_id {
                    obj["request_id"] = serde_json::json!(rid);
                }
                obj.to_string()
            };
        }
    };

    let mut obj = serde_json::json!({
        "status": status, "headers": headers,
        "body_base64": body_b64, "elapsed_ms": elapsed_ms,
    });
    if let Some(rid) = request_id {
        obj["request_id"] = serde_json::json!(rid);
    }
    obj.to_string()
}

// ── 参数解析 ──

struct FfiHttpArgs {
    id: usize, method_str: String, url_str: String,
    body_data: Option<Vec<u8>>, ct_str: Option<String>,
    per_request_headers: HashMap<String, String>,
    per_request_timeout: Option<u64>, ud: usize,
}

#[allow(clippy::too_many_arguments)]
fn parse_http_args(
    handle: *mut c_void, method: FfiString, url: FfiString,
    body: *const u8, body_len: usize, content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32, user_data: *mut c_void,
) -> Option<FfiHttpArgs> {
    if handle.is_null() { return None; }
    let id = handle as usize;
    let method_str = ffi_helpers::ffi_str(method, "GET");
    let url_str = ffi_helpers::ffi_str(url, "/");
    let body_data = if !body.is_null() && body_len > 0 {
        Some(unsafe { ffi_helpers::read_body_bytes(body, body_len) })
    } else { None };
    let ct_str = {
        let s = ffi_helpers::ffi_str(content_type, "");
        if s.is_empty() { None } else { Some(s) }
    };
    let per_request_headers = unsafe { ffi_helpers::parse_headers_json(headers_json) };
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;
    Some(FfiHttpArgs { id, method_str, url_str, body_data, ct_str, per_request_headers, per_request_timeout, ud })
}

fn parse_http_method(method_str: &str) -> Option<HttpMethod> {
    match method_str.to_uppercase().as_str() {
        "GET" => Some(HttpMethod::GET), "POST" => Some(HttpMethod::POST),
        "PUT" => Some(HttpMethod::PUT), "DELETE" => Some(HttpMethod::DELETE),
        "PATCH" => Some(HttpMethod::PATCH),
        _ => None,
    }
}

fn build_request(args: &FfiHttpArgs, method: HttpMethod) -> HttpRequest {
    HttpRequest {
        method, url: args.url_str.clone(), headers: args.per_request_headers.clone(),
        body: args.body_data.clone(), content_type: args.ct_str.clone(),
        timeout_ms: args.per_request_timeout,
        ..Default::default()
    }
}

// ── Lifecycle ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_create(config_json: *const c_char) -> *mut c_void {
    if config_json.is_null() { return std::ptr::null_mut(); }
    let json = CStr::from_ptr(config_json);
    let config: HttpClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c, Err(_) => return std::ptr::null_mut(),
    };
    let transport = match HttpTransport::new(config) {
        Ok(t) => t, Err(_) => return std::ptr::null_mut(),
    };
    let id = REGISTRY.insert(Arc::new(transport));
    id as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_destroy(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = handle as usize;
    http_guard().mark(id);
    if let Some(t) = REGISTRY.get(id) { t.cancel_all(); }
    REGISTRY.remove(id);
}

// ── GET / POST ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_get(
    handle: *mut c_void, url: FfiString, headers_json: *const c_char,
    timeout_ms: u32, callback: EventCallback, user_data: *mut c_void,
) {
    let null_ffi = FfiString { data: std::ptr::null(), len: 0 };
    let null_ct = FfiString { data: std::ptr::null(), len: 0 };
    let Some(args) = parse_http_args(handle, null_ffi, url, std::ptr::null(), 0, null_ct, headers_json, timeout_ms, user_data) else { return };
    let transport = REGISTRY.get(args.id);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let r = t.execute(build_request(&args, HttpMethod::GET)).await;
            let json = match &r { Ok(resp) => http_result_to_json(Ok(resp), None), Err(e) => http_result_to_json(Err(e), None) };
            ffi_helpers::invoke_callback_if_active(http_guard(), args.id, callback, "http_result", json, args.ud);
        });
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_post(
    handle: *mut c_void, url: FfiString, body: *const u8, body_len: usize,
    content_type: FfiString, headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) {
    let null_ffi = FfiString { data: std::ptr::null(), len: 0 };
    let Some(args) = parse_http_args(handle, null_ffi, url, body, body_len, content_type, headers_json, timeout_ms, user_data) else { return };
    let transport = REGISTRY.get(args.id);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let r = t.execute(build_request(&args, HttpMethod::POST)).await;
            let json = match &r { Ok(resp) => http_result_to_json(Ok(resp), None), Err(e) => http_result_to_json(Err(e), None) };
            ffi_helpers::invoke_callback_if_active(http_guard(), args.id, callback, "http_result", json, args.ud);
        });
    }
}

// ── 通用 execute ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void, method: FfiString, url: FfiString,
    body: *const u8, body_len: usize, content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) {
    let Some(args) = parse_http_args(handle, method, url, body, body_len, content_type, headers_json, timeout_ms, user_data) else { return };
    let Some(http_method) = parse_http_method(&args.method_str) else {
        ffi_helpers::invoke_callback(callback, "http_result", ffi_helpers::error_json(&format!("Unsupported method: {}", args.method_str)), args.ud);
        return;
    };
    let transport = REGISTRY.get(args.id);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let r = t.execute(build_request(&args, http_method)).await;
            let json = match &r { Ok(resp) => http_result_to_json(Ok(resp), None), Err(e) => http_result_to_json(Err(e), None) };
            ffi_helpers::invoke_callback_if_active(http_guard(), args.id, callback, "http_result", json, args.ud);
        });
    }
}

// ── execute_with_id ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute_with_id(
    handle: *mut c_void, method: FfiString, url: FfiString,
    body: *const u8, body_len: usize, content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) -> u64 {
    let Some(args) = parse_http_args(handle, method, url, body, body_len, content_type, headers_json, timeout_ms, user_data) else { return 0 };
    let Some(http_method) = parse_http_method(&args.method_str) else {
        ffi_helpers::invoke_callback(callback, "http_result", ffi_helpers::error_json(&format!("Unsupported method: {}", args.method_str)), args.ud);
        return 0;
    };
    let transport = REGISTRY.get(args.id);
    if let Some(t) = transport {
        let (request_id, pt) = t.allocate_pending_request();
        runtime().spawn(async move {
            let (_rid, result) = t.execute_with_token(request_id, pt, build_request(&args, http_method)).await;
            let json = match &result { Ok(resp) => http_result_to_json(Ok(resp), Some(request_id)), Err(e) => http_result_to_json(Err(e), Some(request_id)) };
            ffi_helpers::invoke_callback_if_active(http_guard(), args.id, callback, "http_result", json, args.ud);
        });
        request_id
    } else { 0 }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_cancel_request(handle: *mut c_void, request_id: u64) -> i32 {
    if handle.is_null() { return -1; }
    let id = handle as usize;
    if let Some(transport) = REGISTRY.get(id) {
        if transport.cancel_request(request_id) { 0 } else { -1 }
    } else { -1 }
}

// ── 流式下载 ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute_stream(
    handle: *mut c_void, method: FfiString, url: FfiString,
    body: *const u8, body_len: usize, content_type: FfiString,
    headers_json: *const c_char, timeout_ms: u32,
    callback: EventCallback, user_data: *mut c_void,
) -> u64 {
    let Some(args) = parse_http_args(handle, method, url, body, body_len, content_type, headers_json, timeout_ms, user_data) else { return 0 };
    let Some(http_method) = parse_http_method(&args.method_str) else {
        ffi_helpers::invoke_callback(callback, "stream_error", ffi_helpers::error_json(&format!("Unsupported method: {}", args.method_str)), args.ud);
        return 0;
    };
    let transport = REGISTRY.get(args.id);
    if let Some(t) = transport {
        let (request_id, pt) = t.allocate_pending_request();
        runtime().spawn(async move {
            let req = build_request(&args, http_method);
            let _ = t.execute_stream(req, pt, move |event| {
                let (et, ed) = match &event {
                    crate::types::http::StreamEvent::Headers { status, headers } => {
                        ("stream_headers", serde_json::json!({"status":status,"headers":headers,"request_id":request_id}).to_string())
                    }
                    crate::types::http::StreamEvent::Chunk(data) => {
                        use base64::Engine;
                        let data_b64 = base64::engine::general_purpose::STANDARD.encode(data);
                        ("stream_chunk", serde_json::json!({"data_base64": data_b64, "request_id": request_id}).to_string())
                    }
                    crate::types::http::StreamEvent::Done => {
                        ("stream_done", serde_json::json!({"request_id":request_id}).to_string())
                    }
                    crate::types::http::StreamEvent::Error(msg) => {
                        ("stream_error", serde_json::json!({"error":msg,"request_id":request_id}).to_string())
                    }
                };
                ffi_helpers::invoke_callback_if_active(http_guard(), args.id, callback, et, ed, args.ud);
            }).await;
        });
        request_id
    } else { 0 }
}

// ── Runtime control ──

#[no_mangle]
pub unsafe extern "C" fn catcher_http_circuit_breaker_state(handle: *mut c_void) -> *mut c_char {
    if handle.is_null() { return std::ptr::null_mut(); }
    let id = handle as usize;
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
    let id = handle as usize;
    let snapshot = REGISTRY.get(id).map(|t| t.metrics());
    let json = match snapshot {
        Some(s) => serde_json::to_string(&s).unwrap_or_default(),
        None => "{}".to_string(),
    };
    CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_network_changed(handle: *mut c_void) -> i32 {
    if handle.is_null() { return 1; }
    let id = handle as usize;
    match REGISTRY.get(id) {
        Some(transport) => match transport.network_changed() { Ok(()) => 0, Err(_) => 2 },
        None => 1,
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_cancel_all(handle: *mut c_void) {
    if handle.is_null() { return; }
    let id = handle as usize;
    if let Some(transport) = REGISTRY.get(id) { transport.cancel_all(); }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_adaptive_timeout_config(
    handle: *mut c_void, enabled: i32, min_timeout_ms: u32, max_timeout_ms: u32,
    multiplier_scaled: u32, window_size: u32,
) {
    if handle.is_null() { return; }
    let id = handle as usize;
    if let Some(transport) = REGISTRY.get(id) {
        if enabled != 0 {
            transport.set_adaptive_timeout(min_timeout_ms as u64, max_timeout_ms as u64, multiplier_scaled as f64 / 1000.0, window_size as usize);
        } else {
            transport.disable_adaptive_timeout();
        }
    }
}

// ── Multipart ──

#[repr(C)]
pub struct FfiMultipartPart {
    pub name: *const c_char, pub value_or_filename: *const c_char,
    pub content_type: *const c_char, pub data: *const u8, pub data_len: usize,
}

#[no_mangle]
pub unsafe extern "C" fn catcher_http_multipart(
    handle: *mut c_void, method: *const c_char, url: FfiString,
    parts: *const FfiMultipartPart, parts_count: usize,
    headers_json: *const c_char, timeout_ms: u64,
    callback: Option<EventCallback>, user_data: usize,
) {
    if handle.is_null() { return; }
    let callback = match callback { Some(cb) => cb, None => return };
    let id_val = handle as usize;
    let method_str = if method.is_null() { HttpMethod::POST } else {
        match CStr::from_ptr(method).to_str().unwrap_or("POST") {
            "PUT" => HttpMethod::PUT, "PATCH" => HttpMethod::PATCH, _ => HttpMethod::POST,
        }
    };
    let url_str = ffi_helpers::ffi_str(url, "");
    let per_request_headers = unsafe { ffi_helpers::parse_headers_json(headers_json) };
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms) } else { None };

    let mut form = crate::transport::multipart::MultipartForm::new();
    if !parts.is_null() && parts_count > 0 {
        let parts_slice = std::slice::from_raw_parts(parts, parts_count);
        for part in parts_slice {
            let name = if part.name.is_null() { continue } else {
                CStr::from_ptr(part.name).to_str().unwrap_or("").to_string()
            };
            if name.is_empty() { continue; }
            if part.data.is_null() || part.data_len == 0 {
                let value = if part.value_or_filename.is_null() { String::new() } else {
                    CStr::from_ptr(part.value_or_filename).to_str().unwrap_or("").to_string()
                };
                form = form.text(name, value);
            } else {
                let data = std::slice::from_raw_parts(part.data, part.data_len).to_vec();
                let filename = if part.value_or_filename.is_null() { "file".to_string() } else {
                    CStr::from_ptr(part.value_or_filename).to_str().unwrap_or("file").to_string()
                };
                let ct = if part.content_type.is_null() { "application/octet-stream".to_string() } else {
                    CStr::from_ptr(part.content_type).to_str().unwrap_or("application/octet-stream").to_string()
                };
                form = form.file(name, filename, ct, data);
            }
        }
    }

    let transport = REGISTRY.get(id_val);
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: method_str, url: url_str,
                headers: per_request_headers, multipart: Some(form),
                timeout_ms: per_request_timeout,
                ..Default::default()
            };
            let result = t.execute(request).await;
            let json = match &result { Ok(resp) => http_result_to_json(Ok(resp), None), Err(e) => http_result_to_json(Err(e), None) };
            ffi_helpers::invoke_callback_if_active(http_guard(), id_val, callback, "http_result", json, user_data);
        });
    }
}
