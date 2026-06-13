//! Integration tests for HTTP C ABI symbols.
//!
//! These tests call the `#[no_mangle] pub unsafe extern "C"` functions
//! defined in catcher-http/src/ffi/http_ffi.rs and catcher-ffi/src/lib.rs.
//!
//! Run with:
//!   cargo test -p catcher-ffi --test http_test

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, Mutex};

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
    unsafe {
        catcher_core::ffi_types::catcher_free_event_data(
            _event_type as *mut c_char,
            event_data as *mut u8,
        );
    }
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

// ── N-03: Per-request cancel tests ──

extern "C" fn capture_to_result(
    _event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    user_data: *mut c_void,
) {
    let bytes = unsafe { std::slice::from_raw_parts(event_data, event_data_len) };
    let json = String::from_utf8_lossy(bytes).to_string();
    unsafe { catcher_core::ffi_types::catcher_free_event_data(_event_type as *mut c_char, event_data as *mut u8); }
    let result: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    *result.lock().unwrap() = Some(json);
}


fn make_result_cell() -> (Arc<Mutex<Option<String>>>, *mut c_void) {
    let cell = Arc::new(Mutex::new(None::<String>));
    let ptr = Arc::as_ptr(&cell) as *mut c_void;
    (cell, ptr)
}

#[tokio::test]
async fn h08_execute_with_id_returns_request_id() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "connect_timeout_ms": 5000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (_cell, user_data) = make_result_cell();
    let request_id = unsafe {
        http::catcher_http_execute_with_id(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_to_result, user_data,
        )
    };
    assert!(request_id > 0);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let result_ref: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    let json = result_ref.lock().unwrap().clone().expect("callback should have been invoked");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(parsed["request_id"], request_id);

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h09_cancel_request_by_id() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "connect_timeout_ms": 2000, "response_timeout_ms": 5000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (_cell, user_data) = make_result_cell();
    let request_id = unsafe {
        http::catcher_http_execute_with_id(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_to_result, user_data,
        )
    };
    assert!(request_id > 0);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(unsafe { http::catcher_http_cancel_request(handle, request_id) }, 0);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let result_ref: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    let json = result_ref.lock().unwrap().clone().expect("callback should have been invoked");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert!(parsed.get("type").is_some() || parsed.get("error").is_some(), "expected error or cancelled, got: {json}");

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h10_cancel_request_nonexistent() {
    let config = r#"{"base_url":"https://httpbin.org","connect_timeout_ms":5000}"#;
    let c_config = CString::new(config).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());
    assert_eq!(unsafe { http::catcher_http_cancel_request(handle, 99999) }, -1);
    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h11_cancel_all_with_per_request() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(5)))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "connect_timeout_ms": 10000, "response_timeout_ms": 10000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (cell1, ud1) = make_result_cell();
    let id1 = unsafe {
        http::catcher_http_execute_with_id(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""), std::ptr::null(), 0,
            capture_to_result, ud1,
        )
    };
    assert!(id1 > 0);

    unsafe { http::catcher_http_client_cancel_all(handle); }
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let (cell2, ud2) = make_result_cell();
    let new_id = unsafe {
        http::catcher_http_execute_with_id(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""), std::ptr::null(), 0,
            capture_to_result, ud2,
        )
    };
    assert!(new_id > 0);
    assert_ne!(new_id, id1);

    unsafe { http::catcher_http_client_cancel_all(handle); }
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    drop(cell1.lock().unwrap());
    drop(cell2.lock().unwrap());

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h12_cancel_request_null_handle() {
    assert_eq!(unsafe { http::catcher_http_cancel_request(std::ptr::null_mut(), 1) }, -1);
}

// N-02: Streaming download tests

#[tokio::test]
async fn h13_execute_stream_headers_and_chunks() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hello world"))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "connect_timeout_ms": 5000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (_cell, user_data) = make_result_cell();
    let rid = unsafe {
        http::catcher_http_execute_stream(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_to_result, user_data,
        )
    };
    assert!(rid > 0);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let result_ref: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    let json = result_ref.lock().unwrap().clone().expect("callback should have been invoked");
    assert!(json.contains(&format!("{rid}")), "expected request_id in callback, got: {json}");

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h14_execute_stream_returns_request_id() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "connect_timeout_ms": 5000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (_cell, user_data) = make_result_cell();
    let rid = unsafe {
        http::catcher_http_execute_stream(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_to_result, user_data,
        )
    };
    assert!(rid > 0);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    // request_id should appear in the callback data
    let result_ref: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    let json = result_ref.lock().unwrap().clone().expect("callback should have been invoked");
    assert!(json.contains(&format!("{rid}")));

    unsafe { http::catcher_http_client_destroy(handle); }
}

// ── N-02/N-03 supplementary FFI tests ──

// Capture multiple stream events
extern "C" fn capture_stream_events(
    event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    user_data: *mut c_void,
) {
    let et = if event_type.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(event_type) }.to_string_lossy().to_string()
    };
    let bytes = unsafe { std::slice::from_raw_parts(event_data, event_data_len) };
    let data = String::from_utf8_lossy(bytes).to_string();
    unsafe { catcher_core::ffi_types::catcher_free_event_data(event_type as *mut c_char, event_data as *mut u8); }
    let events: &Mutex<Vec<(String, String)>> = unsafe { &*(user_data as *const Mutex<Vec<(String, String)>>) };
    events.lock().unwrap().push((et, data));
}

#[allow(clippy::type_complexity)]
fn make_events_cell() -> (Arc<Mutex<Vec<(String, String)>>>, *mut c_void) {
    let cell = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
    let ptr = Arc::as_ptr(&cell) as *mut c_void;
    (cell, ptr)
}

#[tokio::test]
async fn h15_execute_stream_cancel() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_string("x".repeat(65536))
            .set_delay(std::time::Duration::from_secs(30)))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "response_timeout_ms": 60000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (events, user_data) = make_events_cell();
    let _rid = unsafe {
        http::catcher_http_execute_stream(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_stream_events, user_data,
        )
    };

    // Wait briefly then cancel all
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    unsafe { http::catcher_http_client_cancel_all(handle) };

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let evts = events.lock().unwrap();
    // Should have at least headers and possibly a cancel error
    assert!(!evts.is_empty(), "should have at least 1 stream event");
    let has_error = evts.iter().any(|(et, _)| et == "stream_error");
    assert!(has_error, "should have a stream_error event after cancel");

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h18b_execute_callback_receives_response_with_request_id() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "connect_timeout_ms": 5000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (_cell, user_data) = make_result_cell();
    let request_id = unsafe {
        http::catcher_http_execute_with_id(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_to_result, user_data,
        )
    };
    assert!(request_id > 0);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let result_ref: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    let json = result_ref.lock().unwrap().clone().expect("callback should have been invoked");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    // Verify response fields
    assert_eq!(parsed["status"], 200);
    assert!(parsed.get("request_id").is_some());
    assert_eq!(parsed["request_id"], request_id);
    assert!(parsed.get("elapsed_ms").is_some());

    unsafe { http::catcher_http_client_destroy(handle); }
}

#[tokio::test]
async fn h09b_cancel_request_returns_cancelled_json() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
        .mount(&server).await;

    let config = serde_json::json!({"base_url": server.uri(), "response_timeout_ms": 60000});
    let c_config = CString::new(config.to_string()).unwrap();
    let handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!handle.is_null());

    let (_cell, user_data) = make_result_cell();
    let request_id = unsafe {
        http::catcher_http_execute_with_id(
            handle, ffi_string("GET"), ffi_string("/test"),
            std::ptr::null(), 0, ffi_string(""),
            std::ptr::null(), 0,
            capture_to_result, user_data,
        )
    };
    assert!(request_id > 0);

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(unsafe { http::catcher_http_cancel_request(handle, request_id) }, 0);

    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let result_ref: &Mutex<Option<String>> = unsafe { &*(user_data as *const Mutex<Option<String>>) };
    let json = result_ref.lock().unwrap().clone().expect("callback should have been invoked");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    // N-03: cancelled callback should have type="cancelled" and request_id
    assert_eq!(parsed["type"], "cancelled");
    assert_eq!(parsed["request_id"], request_id);

    unsafe { http::catcher_http_client_destroy(handle); }
}
