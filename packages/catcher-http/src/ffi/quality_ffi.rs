//! Network Quality C ABI — evaluate_quality + quality_history
//!
//! Maintains a persistent NetworkQualityEvaluator with a sliding window
//! so history data accumulates across calls.

use std::ffi::{c_char, c_void, CString};
use std::sync::Mutex;

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::{EventCallback, FfiString};

/// Global persistent evaluator — sliding window survives across FFI calls.
static EVALUATOR: Mutex<Option<NetworkQualityEvaluator>> = Mutex::new(None);

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

fn invoke_quality_callback(callback: EventCallback, json: String, user_data: usize) {
    let c_event = CString::new("quality_result").unwrap_or_default();
    let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
    let json_len = c_json.as_bytes().len();

    callback(
        c_event.into_raw(),
        c_json.into_raw() as *const u8,
        json_len,
        user_data as *mut c_void,
    );
}

/// Evaluate network quality by performing an HTTP HEAD request to the given host
/// and recording the RTT into a persistent sliding window.
#[no_mangle]
pub unsafe extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    let host_str = ffi_string_to_string(host, "https://www.example.com");
    let ud = user_data as usize;

    runtime().spawn(async move {
        // Ensure evaluator exists (lock briefly, then release before await)
        {
            let mut guard = EVALUATOR.lock().unwrap();
            if guard.is_none() {
                *guard = Some(NetworkQualityEvaluator::new(50));
            }
        }

        // Perform the async measurement without holding any lock.
        // We clone the host_str to avoid borrow issues.
        let result = {
            // Lock briefly to get the evaluator reference, then release.
            // We need to call measure_http_rtt on the evaluator inside the lock.
            // Since measure_http_rtt is async and borrows &mut self, we can't hold the lock across it.
            // Solution: take the evaluator out, use it, put it back.
            let mut evaluator = EVALUATOR.lock().unwrap().take().unwrap();
            let result = evaluator.measure_http_rtt(&host_str, "/").await;
            EVALUATOR.lock().unwrap().replace(evaluator);
            result
        };

        // Record and evaluate (lock briefly)
        let mut guard = EVALUATOR.lock().unwrap();
        let evaluator = guard.as_mut().unwrap();
        match result {
            Ok(_rtt_ms) => {
                let eval_result = evaluator.evaluate();
                let json = serde_json::to_string(&eval_result).unwrap_or_default();
                drop(guard);
                invoke_quality_callback(callback, json, ud);
            }
            Err(e) => {
                let eval_result = evaluator.evaluate();
                let mut map = serde_json::to_value(&eval_result).unwrap_or_default();
                if let Some(obj) = map.as_object_mut() {
                    obj.insert("error".into(), e.to_string().into());
                }
                let json = serde_json::to_string(&map).unwrap_or_default();
                drop(guard);
                invoke_quality_callback(callback, json, ud);
            }
        }
    });
}

/// Query the persistent sliding window history for network quality.
/// Returns a JSON string with rtt_snapshot and current quality level.
/// Caller must free the returned C string via `catcher_free_data`.
#[no_mangle]
pub unsafe extern "C" fn catcher_quality_history() -> *mut c_char {
    let guard = EVALUATOR.lock().unwrap();
    let json = match guard.as_ref() {
        Some(evaluator) => {
            let snapshot = evaluator.rtt_snapshot();
            let level = evaluator.evaluate();
            serde_json::json!({
                "rtt_samples": {
                    "avg_rtt_ms": snapshot.avg_rtt_ms,
                    "min_rtt_ms": snapshot.min_rtt_ms,
                    "max_rtt_ms": snapshot.max_rtt_ms,
                    "jitter_ms": snapshot.jitter_ms,
                    "sample_count": snapshot.sample_count,
                },
                "current_level": format!("{:?}", level.level),
            })
            .to_string()
        }
        None => serde_json::json!({"rtt_samples": null, "current_level": "unknown"}).to_string(),
    };

    CString::new(json).unwrap_or_default().into_raw()
}
