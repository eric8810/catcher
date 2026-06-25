//! FFI 共享辅助函数 — 供 catcher-http / catcher-ws / catcher-uniffi 的 FFI 层复用
//!
//! 提供：取消守卫、CString 回调调用、JSON 头解析、body 字节读取等。
//! 注意：tokio runtime 初始化不在本模块中 — 各 crate 自行管理 runtime。

use std::collections::{HashMap, HashSet};
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::RwLock;

use crate::EventCallback;

// ── 取消守卫（防止 destroy 后回调 → use-after-free） ──

/// 取消守卫：记录已销毁的句柄 id，供异步回调前检查。
/// id 由 HandleRegistry 单调分配、不复用，故集合只增不清是安全的。
///
/// **每个 FFI 模块应持有自己的 CancellationGuard 实例**，避免跨模块 ID 碰撞。
/// 使用 `CancellationGuard::new()` 创建并通过 `OnceLock` 懒初始化。
pub struct CancellationGuard {
    ids: RwLock<HashSet<usize>>,
}

impl CancellationGuard {
    pub fn new() -> Self {
        Self { ids: RwLock::new(HashSet::new()) }
    }

    pub fn mark(&self, id: usize) {
        match self.ids.write() {
            Ok(mut ids) => { ids.insert(id); }
            Err(poisoned) => { poisoned.into_inner().insert(id); }
        }
    }

    pub fn is_cancelled(&self, id: usize) -> bool {
        match self.ids.read() {
            Ok(ids) => ids.contains(&id),
            Err(poisoned) => poisoned.into_inner().contains(&id),
        }
    }
}

impl Default for CancellationGuard {
    fn default() -> Self { Self::new() }
}

// ── FFI 回调工具 ──

/// 构建 JSON 错误字符串。
pub fn error_json(msg: &str) -> String {
    serde_json::json!({"error": msg}).to_string()
}

/// 安全读取 FfiString → Rust String（委托给 FfiString::to_string_lossy）。
pub fn ffi_str(s: crate::FfiString, default: &str) -> String {
    s.to_string_lossy(default)
}

/// 从裸指针安全读取 body 字节。
///
/// # Safety
/// `body` must be a valid pointer to `body_len` bytes, or null.
pub unsafe fn read_body_bytes(body: *const u8, body_len: usize) -> Vec<u8> {
    if body.is_null() || body_len == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(body, body_len).to_vec() }
    }
}

/// 解析 C 字符串 JSON → HashMap<String, String>。
///
/// # Safety
/// `headers_json` must be a valid null-terminated C string, or null.
pub unsafe fn parse_headers_json(headers_json: *const c_char) -> HashMap<String, String> {
    if headers_json.is_null() {
        return HashMap::new();
    }
    let json_str = unsafe {
        match CStr::from_ptr(headers_json).to_str() {
            Ok(s) => s,
            Err(_) => return HashMap::new(),
        }
    };
    if json_str.is_empty() {
        return HashMap::new();
    }
    serde_json::from_str::<HashMap<String, String>>(json_str).unwrap_or_default()
}

/// 调用 FFI 事件回调（所有权转移 CString）。
pub fn invoke_callback(cb: EventCallback, event_name: &str, json: String, user_data: usize) {
    let c_event = CString::new(event_name.replace('\0', "")).unwrap_or_default();
    let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
    let json_len = c_json.as_bytes().len();
    cb(
        c_event.into_raw(),
        c_json.into_raw() as *const u8,
        json_len,
        user_data as *mut c_void,
    );
}

/// 仅在句柄未被 destroy 时回调。返回 true 表示实际调用了回调。
pub fn invoke_callback_if_active(
    guard: &CancellationGuard,
    id: usize,
    callback: EventCallback,
    event_name: &str,
    json: String,
    user_data: usize,
) -> bool {
    if guard.is_cancelled(id) {
        return false;
    }
    invoke_callback(callback, event_name, json, user_data);
    true
}
