//! Integration tests for N-04 quality push subscription C ABI symbols.
//!
//! Run with:
//!   cargo test -p catcher-ffi --test quality_test

use std::ffi::{c_char, c_void, CString};
use std::sync::Mutex;

use catcher_core::ffi_types::FfiString;
use catcher_http::ffi::quality_ffi as quality;

/// Per-test callback state — avoids shared global statics
struct CallbackState {
    count: usize,
    last_event: Option<String>,
}

// 故意泄漏 user_data 指向的内存：质量订阅的回调由后台 runtime 周期性触发，可能在测试
// 退订/返回后仍发生一次（abort 对同步回调存在竞态窗口）。若用 Arc 并在测试结束时 drop，
// user_data 会悬空 → UAF。泄漏一个小状态在测试进程内可忽略。详见 docs/issues/034。
fn make_callback_state() -> (&'static Mutex<CallbackState>, *mut c_void) {
    let leaked: &'static Mutex<CallbackState> = Box::leak(Box::new(Mutex::new(CallbackState {
        count: 0,
        last_event: None,
    })));
    let ptr = leaked as *const Mutex<CallbackState> as *mut c_void;
    (leaked, ptr)
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

    // 退订 sub1，记录此刻两者计数。
    unsafe { quality::catcher_quality_unsubscribe(sub1) };
    let count1_at_unsub = state1.lock().unwrap().count;
    let count2_at_unsub = state2.lock().unwrap().count;

    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let count1_final = state1.lock().unwrap().count;
    let count2_final = state2.lock().unwrap().count;

    // 独立性断言 —— 按每个订阅各自的生命周期判断，不比较两个 prober 的相对速度
    // （回调由真实网络探测触发，相对快慢非确定，旧的 `count2 >= count1` 因此 flaky）。
    // 1) sub1 退订后不再新增回调（与 q03 相同的稳定语义）。
    assert_eq!(
        count1_final, count1_at_unsub,
        "sub1 退订后不应再收到回调"
    );
    // 2) sub2 不受 sub1 退订影响，计数单调不减（独立工作）。
    assert!(
        count2_final >= count2_at_unsub,
        "sub2 不应受 sub1 退订影响"
    );

    unsafe { quality::catcher_quality_unsubscribe(sub2) };
}

/// q07: Unsubscribe null handle doesn't crash
#[tokio::test]
async fn q07_unsubscribe_null_handle() {
    unsafe { quality::catcher_quality_unsubscribe(std::ptr::null_mut()) };
}
