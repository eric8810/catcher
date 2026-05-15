//! HTTP C ABI — create / get / post / execute / destroy

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex};

use crate::transport::http_client::HttpTransport;
use crate::types::http::{HttpClientConfig, HttpMethod, HttpRequest};

use catcher_core::{EventCallback, FfiString};

static HANDLES: Mutex<Option<HashMap<usize, Arc<HttpTransport>>>> = Mutex::new(None);
static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

/// Global tokio runtime for HTTP async operations (spawning, etc.)
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

/// Safely read an FfiString as a Rust String. Returns default on null/invalid.
fn ffi_string_to_string(s: FfiString, default: &str) -> String {
    s.to_string_lossy(default)
}

/// Safely read body bytes from a raw pointer. Returns empty vec on null.
fn read_body_bytes(body: *const u8, body_len: usize) -> Vec<u8> {
    if body.is_null() || body_len == 0 {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(body, body_len).to_vec() }
}

/// Build a JSON error string safely (no format! injection).
fn error_json(msg: &str) -> String {
    serde_json::json!({ "error": msg }).to_string()
}

/// Parse a null-terminated JSON string `{"k":"v",...}` into a HashMap.
/// Returns empty map on null, empty, or invalid input.
fn parse_headers_json(headers_json: *const c_char) -> HashMap<String, String> {
    if headers_json.is_null() {
        return HashMap::new();
    }
    let json_str = unsafe {
        match CStr::from_ptr(headers_json).to_str() {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        }
    };
    if json_str.is_empty() {
        return HashMap::new();
    }
    serde_json::from_str::<HashMap<String, String>>(json_str).unwrap_or_default()
}

/// Invoke an FFI event callback with ownership-transferred CStrings.
fn invoke_http_callback(
    callback: EventCallback,
    event_name: &str,
    json: String,
    user_data: usize,
) {
    // Replace null bytes to prevent CString::new panic
    let c_event = CString::new(event_name.replace('\0', "")).unwrap_or_default();
    let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
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
    headers_json: *const c_char,
    timeout_ms: u32,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    let url_str = ffi_string_to_string(url, "/");
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 { Some(timeout_ms as u64) } else { None };
    let ud = user_data as usize;

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: HttpMethod::GET,
                url: url_str,
                headers: per_request_headers,
                body: None,
                content_type: None,
                timeout_ms: per_request_timeout,
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
    handle: *mut c_void,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,
    timeout_ms: u32,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
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
                method: HttpMethod::POST,
                url: url_str,
                headers: per_request_headers,
                body: Some(body_data),
                content_type: Some(ct_str),
                timeout_ms: per_request_timeout,
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

/// Generic HTTP request — accepts method as a string ("GET", "POST", "PUT", "DELETE", "PATCH").
/// This is the preferred entry point for FFI consumers that need all HTTP methods.
///
/// `headers_json` — null-terminated JSON `{"k":"v",...}` for per-request headers.
///   NULL or empty = no per-request headers. These override config.default_headers.
/// `timeout_ms` — per-request timeout in milliseconds. 0 = use transport default.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,
    timeout_ms: u32,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);

    let method_str = ffi_string_to_string(method, "GET");
    let url_str = ffi_string_to_string(url, "/");
    let body_data = if !body.is_null() && body_len > 0 {
        Some(read_body_bytes(body, body_len))
    } else {
        None
    };
    let ct_str = {
        let s = ffi_string_to_string(content_type, "");
        if s.is_empty() { None } else { Some(s) }
    };
    let per_request_headers = parse_headers_json(headers_json);
    let per_request_timeout = if timeout_ms > 0 {
        Some(timeout_ms as u64)
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
        other => {
            // Return error for unknown methods instead of silent GET fallback
            let json = error_json(&format!("Unsupported HTTP method: {other}"));
            invoke_http_callback(callback, "http_result", json, ud);
            return;
        }
    };

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        runtime().spawn(async move {
            let request = HttpRequest {
                method: http_method,
                url: url_str,
                headers: per_request_headers,
                body: body_data,
                content_type: ct_str,
                timeout_ms: per_request_timeout,
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

/// Query the circuit breaker state for this HTTP client.
/// Returns a JSON string like `{"state":"closed","failure_count":0,"success_count":0}`.
/// Caller must free the returned C string via `catcher_free_data`.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_circuit_breaker_state(
    handle: *mut c_void,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let id = *(handle as *const usize);
    let state = handles()
        .as_ref()
        .and_then(|m| m.get(&id))
        .and_then(|t| t.circuit_breaker_state());

    let json = match state {
        Some(s) => serde_json::to_string(&s).unwrap_or_default(),
        None => serde_json::json!({"state":"disabled"}).to_string(),
    };

    CString::new(json).unwrap_or_default().into_raw()
}

/// Query runtime metrics for this HTTP client.
/// Returns a JSON string with MetricsSnapshot fields.
/// Caller must free the returned C string via `catcher_free_data`.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_metrics(
    handle: *mut c_void,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let id = *(handle as *const usize);
    let snapshot = handles()
        .as_ref()
        .and_then(|m| m.get(&id))
        .map(|t| t.metrics());

    let json = match snapshot {
        Some(s) => serde_json::to_string(&s).unwrap_or_default(),
        None => "{}".to_string(),
    };

    CString::new(json).unwrap_or_default().into_raw()
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

/// Cancel all in-flight requests on this HTTP client.
/// New requests after cancel will proceed normally.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_cancel_all(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    if let Some(transport) = handles().as_ref().and_then(|m| m.get(&id)) {
        transport.cancel_all();
    }
}

/// Configure adaptive timeout for this HTTP client.
/// `enabled` != 0 to enable; `min_timeout_ms`, `max_timeout_ms`, `multiplier`
/// (as float scaled by 1000, e.g. 2500 = 2.5), `window_size` for sliding window.
#[no_mangle]
pub unsafe extern "C" fn catcher_http_adaptive_timeout_config(
    handle: *mut c_void,
    enabled: i32,
    min_timeout_ms: u32,
    max_timeout_ms: u32,
    multiplier_scaled: u32,
    window_size: u32,
) {
    if handle.is_null() {
        return;
    }
    let id = *(handle as *const usize);
    if let Some(transport) = handles().as_ref().and_then(|m| m.get(&id)) {
        if enabled != 0 {
            let multiplier = multiplier_scaled as f64 / 1000.0;
            transport.set_adaptive_timeout(
                min_timeout_ms as u64,
                max_timeout_ms as u64,
                multiplier,
                window_size as usize,
            );
        } else {
            transport.disable_adaptive_timeout();
        }
    }
}
