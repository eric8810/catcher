//! Network Quality C ABI — evaluate_quality

use std::ffi::{c_void, CString};

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::{EventCallback, FfiString};

/// Safely read an FfiString as a Rust String.
fn ffi_string_to_string(s: FfiString, default: &str) -> String {
    if s.data.is_null() || s.len == 0 {
        return default.to_string();
    }
    unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(s.data as *const u8, s.len))
            .unwrap_or(default)
            .to_string()
    }
}

#[no_mangle]
pub unsafe extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    let host_str = ffi_string_to_string(host, "https://www.example.com");
    let ud = user_data as usize;

    // Use spawn_blocking because evaluate() is CPU-bound / potentially blocking
    tokio::task::spawn_blocking(move || {
        let evaluator = NetworkQualityEvaluator::new(20);
        // TODO: pass host_str into evaluator when host-specific evaluation is implemented
        let _ = host_str;
        let result = evaluator.evaluate();
        let json = serde_json::to_string(&result).unwrap_or_default();

        let c_event = CString::new("quality_result").unwrap_or_default();
        let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
        let json_len = c_json.as_bytes().len();

        callback(
            c_event.into_raw(),
            c_json.into_raw() as *const u8,
            json_len,
            ud as *mut c_void,
        );
    });
}
