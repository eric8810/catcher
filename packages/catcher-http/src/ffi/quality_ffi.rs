//! Network Quality C ABI — evaluate_quality

use std::ffi::{c_void, CString};

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::{EventCallback, FfiString};

/// Global tokio runtime for quality FFI operations.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-http quality FFI")
    })
}

/// Safely read an FfiString as a Rust String.
fn ffi_string_to_string(s: FfiString, default: &str) -> String {
    s.to_string_lossy(default)
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
    runtime().spawn_blocking(move || {
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
