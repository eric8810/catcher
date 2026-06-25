//! Network Quality C ABI — evaluate_quality + quality_history
#![allow(clippy::missing_safety_doc)]

use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, OnceLock};

use crate::observability::network_quality::NetworkQualityEvaluator;

use catcher_core::ffi_helpers::{self};
use catcher_core::{EventCallback, FfiString};

type SharedEvaluator = Arc<tokio::sync::Mutex<NetworkQualityEvaluator>>;

static EVALUATOR: std::sync::OnceLock<SharedEvaluator> = std::sync::OnceLock::new();
static SUBSCRIPTIONS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());

fn evaluator() -> SharedEvaluator {
    EVALUATOR.get_or_init(|| Arc::new(tokio::sync::Mutex::new(NetworkQualityEvaluator::new(50)))).clone()
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime"))
}

#[no_mangle]
pub unsafe extern "C" fn catcher_evaluate_quality(
    host: FfiString, callback: EventCallback, user_data: *mut c_void,
) {
    let host_str = ffi_helpers::ffi_str(host, "https://www.example.com");
    let ud = user_data as usize;
    let shared = evaluator();
    runtime().spawn(async move {
        let mut guard = shared.lock().await;
        let result = guard.measure_http_rtt(&host_str, "/").await;
        let eval_result = guard.evaluate();
        let mut json = serde_json::to_value(&eval_result).unwrap_or_default();
        if let Err(e) = result {
            if let Some(obj) = json.as_object_mut() {
                obj.insert("error".into(), e.to_string().into());
            }
        }
        drop(guard);
        ffi_helpers::invoke_callback(callback, "quality_result", serde_json::to_string(&json).unwrap_or_default(), ud);
    });
}

#[no_mangle]
pub unsafe extern "C" fn catcher_quality_history() -> *mut c_char {
    let shared = evaluator();
    let json = match shared.try_lock() {
        Ok(mut guard) => {
            let snapshot = guard.rtt_snapshot();
            let level = guard.evaluate();
            serde_json::json!({
                "rtt_samples": {"avg_rtt_ms": snapshot.avg_rtt_ms, "min_rtt_ms": snapshot.min_rtt_ms, "max_rtt_ms": snapshot.max_rtt_ms, "jitter_ms": snapshot.jitter_ms, "sample_count": snapshot.sample_count},
                "current_level": format!("{:?}", level.level),
            }).to_string()
        }
        Err(_) => serde_json::json!({"rtt_samples": null, "current_level": "busy"}).to_string(),
    };
    CString::new(json).unwrap_or_default().into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn catcher_quality_subscribe(
    host: FfiString, interval_ms: u32, callback: EventCallback, user_data: *mut c_void,
) -> *mut c_void {
    let host_str = ffi_helpers::ffi_str(host, "https://www.example.com");
    let sub = crate::observability::network_quality::QualitySubscription::start(host_str, interval_ms as u64, callback, user_data as usize);
    let boxed = Box::new(sub);
    let ptr = Box::into_raw(boxed) as *mut c_void;
    SUBSCRIPTIONS.lock().unwrap().push(ptr as usize);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn catcher_quality_unsubscribe(sub_handle: *mut c_void) {
    if sub_handle.is_null() { return; }
    {
        let mut subs = SUBSCRIPTIONS.lock().unwrap();
        let Some(pos) = subs.iter().position(|&p| p == sub_handle as usize) else { return; };
        subs.remove(pos);
    }
    let sub: Box<crate::observability::network_quality::QualitySubscription> = Box::from_raw(sub_handle as *mut _);
    sub.unsubscribe();
}
