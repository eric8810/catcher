//! Network Quality C ABI — evaluate_quality + quality_history
//!
//! Maintains a persistent NetworkQualityEvaluator with a sliding window
//! so history data accumulates across calls.
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void, CString};
use std::sync::Arc;

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::{EventCallback, FfiString};

// ── 类型别名 ──

// 使用 tokio::sync::Mutex 以便安全地跨 .await 持锁，
// 消除 take/put 模式的并发 panic 风险。
type SharedEvaluator = Arc<tokio::sync::Mutex<NetworkQualityEvaluator>>;

/// 全局共享 evaluator，懒初始化。
static EVALUATOR: std::sync::OnceLock<SharedEvaluator> = std::sync::OnceLock::new();

fn evaluator() -> SharedEvaluator {
    EVALUATOR
        .get_or_init(|| {
            Arc::new(tokio::sync::Mutex::new(NetworkQualityEvaluator::new(50)))
        })
        .clone()
}

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
    let shared = evaluator();

    runtime().spawn(async move {
        // 持锁覆盖整个 measure→evaluate 周期，消除竞态
        let mut guard = shared.lock().await;
        let result = guard.measure_http_rtt(&host_str, "/").await;
        match result {
            Ok(_rtt_ms) => {
                let eval_result = guard.evaluate();
                let json = serde_json::to_string(&eval_result).unwrap_or_default();
                drop(guard);
                invoke_quality_callback(callback, json, ud);
            }
            Err(e) => {
                let eval_result = guard.evaluate();
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
    // 同步路径：使用 try_lock 避免阻塞。如果 evaluator 正忙则返回 busy。
    let shared = evaluator();
    let json = match shared.try_lock() {
        Ok(mut guard) => {
            let snapshot = guard.rtt_snapshot();
            let level = guard.evaluate();
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
        Err(_) => serde_json::json!({"rtt_samples": null, "current_level": "busy"}).to_string(),
    };

    CString::new(json).unwrap_or_default().into_raw()
}

// ── Quality push subscription ──
static SUBSCRIPTIONS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

#[no_mangle]
pub unsafe extern "C" fn catcher_quality_subscribe(
    host: FfiString,
    interval_ms: u32,
    callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void {
    let host_str = ffi_string_to_string(host, "https://www.example.com");
    let ud = user_data as usize;
    let sub = crate::observability::network_quality::QualitySubscription::start(
        host_str, interval_ms as u64, callback, ud,
    );
    let boxed = Box::new(sub);
    let ptr = Box::into_raw(boxed) as *mut c_void;
    SUBSCRIPTIONS.lock().unwrap().push(ptr as usize);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn catcher_quality_unsubscribe(sub_handle: *mut c_void) {
    if sub_handle.is_null() { return; }
    let sub: Box<crate::observability::network_quality::QualitySubscription> = Box::from_raw(sub_handle as *mut _);
    sub.unsubscribe();
    SUBSCRIPTIONS.lock().unwrap().retain(|&p| p != sub_handle as usize);
}
