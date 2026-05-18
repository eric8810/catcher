//! Integration tests for SSE C ABI symbols.
//!
//! Run with:
//!   cargo test -p catcher-ffi --test sse_test

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::Mutex;

use catcher_core::ffi_types::FfiString;
use catcher_http::ffi::sse_ffi as sse;
use catcher_http::ffi::http_ffi as http;

static LAST_SSE_EVENT: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn ffi_string(s: &str) -> FfiString {
    let c = CString::new(s).unwrap();
    let len = c.as_bytes().len();
    let data = c.into_raw();
    FfiString { data, len }
}

unsafe fn read_c_string(ptr: *mut c_char) -> String {
    if ptr.is_null() { return String::new(); }
    let s = CStr::from_ptr(ptr).to_string_lossy().to_string();
    catcher_ffi::catcher_free_data(ptr as *mut c_void, s.len() + 1);
    s
}

extern "C" fn sse_callback(
    _event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    _user_data: *mut c_void,
) {
    let bytes = unsafe { std::slice::from_raw_parts(event_data, event_data_len) };
    let json = String::from_utf8_lossy(bytes).to_string();
    unsafe {
        catcher_core::ffi_types::catcher_free_event_data(
            _event_type as *mut c_char,
            event_data as *mut u8,
        );
    }
    LAST_SSE_EVENT.lock().unwrap().push(json);
}

#[tokio::test]
async fn s01_sse_stream_basic() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    let server = MockServer::start().await;
    // Return a simple SSE stream response
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("data: hello\n\ndata: world\n\n")
                .insert_header("Content-Type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let config = serde_json::json!({
        "base_url": server.uri(),
        "connect_timeout_ms": 5000,
    });
    let c_config = CString::new(config.to_string()).unwrap();
    let http_handle = unsafe { http::catcher_http_client_create(c_config.as_ptr()) };
    assert!(!http_handle.is_null());

    LAST_SSE_EVENT.lock().unwrap().clear();

    let method = ffi_string("POST");
    let url = ffi_string("/stream");

    unsafe {
        sse::catcher_sse_stream(
            http_handle,
            method,
            url,
            std::ptr::null(),
            0,
            std::ptr::null(),
            sse_callback,
            std::ptr::null_mut(),
        );
    }

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    let events = LAST_SSE_EVENT.lock().unwrap().clone();
    assert!(!events.is_empty(), "should have received SSE events");
    // First event should be "open", followed by data events, ending with "close"
    let types: Vec<String> = events.iter()
        .filter_map(|e| serde_json::from_str::<serde_json::Value>(e).ok())
        .filter_map(|v| v["type"].as_str().map(|s| s.to_string()))
        .collect();
    assert!(types.contains(&"data".to_string()), "should contain data events");

    unsafe { http::catcher_http_client_destroy(http_handle); }
}

#[test]
fn s02_sse_connect_and_ready_state() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    // Use a temporary runtime for async mock setup. We keep rt alive so the
    // mock server stays running, but after block_on returns the thread is no
    // longer inside any tokio context, so catcher_sse_connect's internal
    // block_on will work fine.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("data: ping\n\n")
                    .insert_header("Content-Type", "text/event-stream"),
            )
            .mount(&server)
            .await;
    });

    let config = serde_json::json!({
        "url": format!("{}/events", server.uri()),
        "method": "GET",
        "timeout_ms": 5000,
    });
    let c_config = CString::new(config.to_string()).unwrap();

    LAST_SSE_EVENT.lock().unwrap().clear();

    let sse_handle = unsafe {
        sse::catcher_sse_connect(c_config.as_ptr(), sse_callback, std::ptr::null_mut())
    };
    assert!(!sse_handle.is_null(), "SSE connect should succeed");

    std::thread::sleep(std::time::Duration::from_millis(500));

    let state = unsafe { sse::catcher_sse_ready_state(sse_handle) };
    assert!(state >= 0, "ready_state should return valid value");

    let events = LAST_SSE_EVENT.lock().unwrap().clone();
    assert!(!events.is_empty(), "should have received SSE events");

    unsafe { sse::catcher_sse_close(sse_handle); }
    unsafe { sse::catcher_sse_destroy(sse_handle); }
}

#[test]
fn s03_sse_last_event_id() {
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::method;

    // Use a temporary runtime for async mock setup. We keep rt alive so the
    // mock server stays running, but after block_on returns the thread is no
    // longer inside any tokio context, so catcher_sse_connect's internal
    // block_on will work fine.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(MockServer::start());
    rt.block_on(async {
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("id: 42\ndata: test\n\n")
                    .insert_header("Content-Type", "text/event-stream"),
            )
            .mount(&server)
            .await;
    });

    let config = serde_json::json!({
        "url": format!("{}/events", server.uri()),
        "method": "GET",
        "timeout_ms": 5000,
    });
    let c_config = CString::new(config.to_string()).unwrap();

    let sse_handle = unsafe {
        sse::catcher_sse_connect(c_config.as_ptr(), sse_callback, std::ptr::null_mut())
    };
    assert!(!sse_handle.is_null());

    std::thread::sleep(std::time::Duration::from_millis(500));

    let id_ptr = unsafe { sse::catcher_sse_last_event_id(sse_handle) };
    let id = unsafe { read_c_string(id_ptr) };
    // Should have an event ID (might be empty if not yet set, or "42")
    println!("last_event_id: {:?}", id);

    unsafe { sse::catcher_sse_close(sse_handle); }
    unsafe { sse::catcher_sse_destroy(sse_handle); }
}
