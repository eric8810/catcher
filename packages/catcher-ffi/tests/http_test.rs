//! Integration tests for HTTP C ABI symbols.
//!
//! These tests call the `#[no_mangle] pub unsafe extern "C"` functions
//! defined in catcher-http/src/ffi/http_ffi.rs and catcher-ffi/src/lib.rs.
//!
//! Run with:
//!   cargo test -p catcher-ffi --test http_test

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Mutex;

use catcher_core::ffi_types::FfiString;
use catcher_http::ffi::http_ffi as http;

static LAST_RESULT: Mutex<Option<String>> = Mutex::new(None);

extern "C" fn capture_callback(
    _event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    _user_data: *mut c_void,
) {
    let bytes = unsafe { std::slice::from_raw_parts(event_data, event_data_len) };
    let json = String::from_utf8_lossy(bytes).to_string();
    // Free the CStrings Rust allocated
    catcher_core::ffi_types::catcher_free_event_data(
        _event_type as *mut c_char,
        event_data as *mut u8,
    );
    *LAST_RESULT.lock().unwrap() = Some(json);
}

fn ffi_string(s: &str) -> FfiString {
    let c = CString::new(s).unwrap();
    let len = c.as_bytes().len();
    let data = c.into_raw();
    FfiString { data, len }
}

unsafe fn read_c_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let s = CStr::from_ptr(ptr).to_string_lossy().to_string();
    catcher_ffi::catcher_free_data(ptr as *mut c_void, s.len() + 1);
    s
}

#[tokio::test]
async fn h01_create_and_destroy_client() {
    let config = r#"{"base_url":"https://httpbin.org","connect_timeout_ms":5000}"#;
    let c_config = CString::new(config).unwrap();

    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null(), "client creation should succeed");

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h02_get_request_with_wiremock() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server)
        .await;

    let config = serde_json::json!({
        "base_url": server.uri(),
        "connect_timeout_ms": 5000,
        "response_timeout_ms": 10000,
    });
    let c_config = CString::new(config.to_string()).unwrap();

    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let url = ffi_string("/test");
    *LAST_RESULT.lock().unwrap() = None;

    unsafe {
        http::catcher_http_get(
            handle,
            url,
            std::ptr::null(),
            0,
            capture_callback,
            std::ptr::null_mut(),
        );
    }

    // Wait for async callback
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let result = LAST_RESULT.lock().unwrap().clone();
    assert!(result.is_some(), "callback should have been invoked");
    let json = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(parsed["status"], 200);

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h03_execute_with_headers() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, header};

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(header("Authorization", "Bearer test-token"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let config = serde_json::json!({
        "base_url": server.uri(),
        "connect_timeout_ms": 5000,
    });
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let method_ffi = ffi_string("GET");
    let url_ffi = ffi_string("/test");
    let headers_json = r#"{"Authorization":"Bearer test-token"}"#;
    let c_headers = CString::new(headers_json).unwrap();

    *LAST_RESULT.lock().unwrap() = None;

    unsafe {
        http::catcher_http_execute(
            handle,
            method_ffi,
            url_ffi,
            std::ptr::null(),
            0,
            ffi_string(""),
            c_headers.as_ptr(),
            0,
            capture_callback,
            std::ptr::null_mut(),
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let result = LAST_RESULT.lock().unwrap().clone();
    assert!(result.is_some());
    let json = result.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(parsed["status"], 200);

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h04_circuit_breaker_state() {
    let config = r#"{"base_url":"https://httpbin.org","connect_timeout_ms":5000}"#;
    let c_config = CString::new(config).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let state_ptr = unsafe { http::catcher_http_circuit_breaker_state(handle) };
    let state_json = unsafe { read_c_string(state_ptr) };
    let parsed: serde_json::Value = serde_json::from_str(&state_json).unwrap_or_default();
    // No CB configured → "disabled"
    assert_eq!(parsed["state"], "disabled");

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h05_metrics_returns_data() {
    let config = r#"{"base_url":"https://httpbin.org","connect_timeout_ms":5000}"#;
    let c_config = CString::new(config).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let metrics_ptr = unsafe { http::catcher_http_metrics(handle) };
    let metrics_json = unsafe { read_c_string(metrics_ptr) };
    let parsed: serde_json::Value = serde_json::from_str(&metrics_json).unwrap_or_default();
    // Metrics should exist even with zero values
    assert!(parsed.get("http_requests").is_some());

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h06_cancel_all_does_not_panic() {
    let config = r#"{"base_url":"https://httpbin.org","connect_timeout_ms":5000}"#;
    let c_config = CString::new(config).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    // Cancel should not panic even with no in-flight requests
    unsafe { http::catcher_http_client_cancel_all(handle); }

    // New request after cancel should work
    let _metrics_ptr = unsafe { http::catcher_http_metrics(handle) };

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h07_adaptive_timeout_config() {
    let config = r#"{"base_url":"https://httpbin.org","connect_timeout_ms":5000}"#;
    let c_config = CString::new(config).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    // Enable adaptive timeout with P90 * 2.0, 100-30000ms window
    unsafe {
        http::catcher_http_adaptive_timeout_config(handle, 1, 100, 30000, 2000, 20);
    }

    // Disable
    unsafe {
        http::catcher_http_adaptive_timeout_config(handle, 0, 0, 0, 0, 0);
    }

    unsafe { http::catcher_http_client_destroy(handle); }
}
