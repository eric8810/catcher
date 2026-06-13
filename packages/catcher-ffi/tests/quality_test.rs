//! Integration tests for N-04 quality push subscription C ABI symbols.
//!
//! Run with:
//!   cargo test -p catcher-ffi --test quality_test

use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, Mutex};

use catcher_core::ffi_types::FfiString;
use catcher_http::ffi::quality_ffi as quality;

/// Per-test callback state — avoids shared global statics
struct CallbackState {
    count: usize,
    last_event: Option<String>,
}

fn make_callback_state() -> (Arc<Mutex<CallbackState>>, *mut c_void) {
    let state = Arc::new(Mutex::new(CallbackState {
        count: 0,
        last_event: None,
    }));
    let ptr = Arc::as_ptr(&state) as *mut c_void;
    (state, ptr)
}

extern "C" fn capture_quality_callback(
    _event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    user_data: *mut c_void,
) {
    if user_data.is_null() {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(event_data, event_data_len) };
    let json = String::from_utf8_lossy(bytes).to_string();
    unsafe {
        catcher_core::ffi_types::catcher_free_event_data(
            _event_type as *mut c_char,
            event_data as *mut u8,
        );
    }
    let state: &Mutex<CallbackState> = unsafe { &*(user_data as *const Mutex<CallbackState>) };
    let mut s = state.lock().unwrap();
    s.count += 1;
    s.last_event = Some(json);
}

fn ffi_string(s: &str) -> FfiString {
    let c = CString::new(s).unwrap();
    let len = c.as_bytes().len();
    let data = c.into_raw();
    FfiString { data, len }
}

/// q02: Subscribe receives at least one callback within interval
#[tokio::test]
async fn q02_subscribe_receives_callback() {
    let (state, user_data) = make_callback_state();

    let sub_handle = unsafe {
        quality::catcher_quality_subscribe(
            ffi_string("https://www.example.com"),
            1000,
            capture_quality_callback,
            user_data,
        )
    };
    assert!(!sub_handle.is_null());

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let count = state.lock().unwrap().count;
    assert!(
        count >= 1,
        "should receive at least 1 callback, got {count}"
    );

    let event = state.lock().unwrap().last_event.clone();
    assert!(event.is_some(), "should have captured an event");
    let json = event.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert!(parsed.get("level").is_some(), "JSON should contain 'level'");

    unsafe { quality::catcher_quality_unsubscribe(sub_handle) };
}

/// q03: Subscribe then unsubscribe stops callbacks
#[tokio::test]
async fn q03_subscribe_unsubscribe() {
    let (state, user_data) = make_callback_state();

    let sub_handle = unsafe {
        quality::catcher_quality_subscribe(
            ffi_string("https://www.example.com"),
            500,
            capture_quality_callback,
            user_data,
        )
    };
    assert!(!sub_handle.is_null());

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let count_before = state.lock().unwrap().count;
    assert!(
        count_before >= 1,
        "should have callbacks before unsubscribe"
    );

    unsafe { quality::catcher_quality_unsubscribe(sub_handle) };

    let count_at_unsub = state.lock().unwrap().count;
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let count_after = state.lock().unwrap().count;
    assert_eq!(
        count_after, count_at_unsub,
        "should have no new callbacks after unsubscribe"
    );
}

/// q04: Subscribe to invalid host doesn't crash
#[tokio::test]
async fn q04_subscribe_invalid_host() {
    let (_state, user_data) = make_callback_state();

    let sub_handle = unsafe {
        quality::catcher_quality_subscribe(
            ffi_string("http://127.0.0.1:1"),
            500,
            capture_quality_callback,
            user_data,
        )
    };
    assert!(!sub_handle.is_null());

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    unsafe { quality::catcher_quality_unsubscribe(sub_handle) };
    // No panic = pass
}

/// q05: Multiple subscribers work independently
#[tokio::test]
async fn q05_subscribe_multiple() {
    let (state1, ud1) = make_callback_state();
    let (state2, ud2) = make_callback_state();

    let sub1 = unsafe {
        quality::catcher_quality_subscribe(
            ffi_string("https://www.example.com"),
            1000,
            capture_quality_callback,
            ud1,
        )
    };
    let sub2 = unsafe {
        quality::catcher_quality_subscribe(
            ffi_string("https://www.example.com"),
            1000,
            capture_quality_callback,
            ud2,
        )
    };
    assert!(!sub1.is_null());
    assert!(!sub2.is_null());
    assert_ne!(sub1, sub2);

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    unsafe { quality::catcher_quality_unsubscribe(sub1) };
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    let count1 = state1.lock().unwrap().count;
    let count2 = state2.lock().unwrap().count;
    // sub2 should still be receiving callbacks (or at least not panicked)
    assert!(
        count2 >= count1,
        "sub2 should have at least as many callbacks as sub1"
    );

    unsafe { quality::catcher_quality_unsubscribe(sub2) };
}

/// q07: Unsubscribe null handle doesn't crash
#[tokio::test]
async fn q07_unsubscribe_null_handle() {
    unsafe { quality::catcher_quality_unsubscribe(std::ptr::null_mut()) };
}
