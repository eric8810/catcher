//! Network Quality C ABI — evaluate_quality

use std::ffi::c_void;

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::{EventCallback, FfiString};

#[no_mangle]
pub extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    let _host_str = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(host.data as *const u8, host.len))
            .unwrap_or("https://www.example.com")
    }
    .to_string();
    let cb = callback;
    let ud = user_data as usize;

    tokio::task::spawn(async move {
        let evaluator = NetworkQualityEvaluator::new(20);
        let result = evaluator.evaluate();
        let json = serde_json::to_string(&result).unwrap_or_default();
        let c_event = std::ffi::CString::new("quality_result").unwrap();
        cb(
            c_event.as_ptr(),
            json.as_ptr(),
            json.len(),
            ud as *mut c_void,
        );
    });
}
