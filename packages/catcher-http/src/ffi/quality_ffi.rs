//! Network Quality C ABI — evaluate_quality

use std::ffi::{c_void, CString};

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::{EventCallback, FfiString};

#[no_mangle]
pub unsafe extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    let _host_str =
        std::str::from_utf8(std::slice::from_raw_parts(host.data as *const u8, host.len))
            .unwrap_or("https://www.example.com")
            .to_string();
    let ud = user_data as usize;

    tokio::task::spawn(async move {
        let evaluator = NetworkQualityEvaluator::new(20);
        let result = evaluator.evaluate();
        let json = serde_json::to_string(&result).unwrap_or_default();

        // Use into_raw() to transfer ownership — Dart must call
        // catcher_free_event_data() after reading the data.
        let c_event = CString::new("quality_result").unwrap();
        let c_json = CString::new(json).unwrap();
        let json_len = c_json.as_bytes().len();

        callback(
            c_event.into_raw(),
            c_json.into_raw() as *const u8,
            json_len,
            ud as *mut c_void,
        );
    });
}
