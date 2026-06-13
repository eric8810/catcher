//! Integration tests for codec + quality C ABI symbols.
//!
//! Run with:
//!   cargo test -p catcher-ffi --test codec_quality_test

use std::ffi::{c_char, c_void, CStr, CString};

unsafe fn read_c_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let s = CStr::from_ptr(ptr).to_string_lossy().to_string();
    catcher_ffi::catcher_free_data(ptr as *mut c_void, s.len() + 1);
    s
}

// ── Codec tests ──

#[test]
fn c01_pack_roundtrip() {
    let json = r#"{"hello":"world","num":42}"#;
    let c_json = CString::new(json).unwrap();

    let packed = unsafe { catcher_ffi::catcher_pack(c_json.as_ptr()) };
    assert_eq!(packed.error_code, 0, "pack should succeed");
    assert!(!packed.data.is_null());
    assert!(packed.data_len > 0);

    // Unpack back
    let unpacked =
        unsafe { catcher_ffi::catcher_unpack(packed.data as *const u8, packed.data_len) };
    assert_eq!(unpacked.error_code, 0, "unpack should succeed");

    // Free
    catcher_core::ffi_types::catcher_free_result(packed);
    catcher_core::ffi_types::catcher_free_result(unpacked);
}

#[test]
fn c02_pack_invalid_json() {
    let c_json = CString::new("{invalid}").unwrap();
    let result = unsafe { catcher_ffi::catcher_pack(c_json.as_ptr()) };
    assert_ne!(result.error_code, 0, "pack should fail on invalid JSON");
    catcher_core::ffi_types::catcher_free_result(result);
}

#[test]
fn c03_pack_null_input() {
    let result = unsafe { catcher_ffi::catcher_pack(std::ptr::null()) };
    assert_ne!(result.error_code, 0, "pack should fail on null input");
    catcher_core::ffi_types::catcher_free_result(result);
}

// ── Quality tests ──

#[test]
fn q01_quality_history_returns_json() {
    let ptr = unsafe { catcher_http::ffi::quality_ffi::catcher_quality_history() };
    let json = unsafe { read_c_string(ptr) };
    assert!(!json.is_empty());
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert!(parsed.get("current_level").is_some());
}
