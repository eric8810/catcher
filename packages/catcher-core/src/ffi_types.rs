use std::ffi::{c_char, c_void};

/// FFI-safe result type.
///
/// **Ownership:** Returned by-value from FFI functions. Caller must invoke
/// `catcher_free_result()` when done (Drop impl frees `error_message`).
#[repr(C)]
pub struct FfiResult {
    pub error_code: i32, // 0 = success
    pub error_message: *mut c_char,
    pub data: *mut c_void,
    pub data_len: usize,
}

/// FFI-safe string view (borrowed, not owned).
///
/// **Ownership:** Caller allocates and frees the backing memory.
/// Rust reads but never frees `data`. Intended for pass-by-value FFI args.
#[repr(C)]
pub struct FfiString {
    pub data: *const c_char,
    pub len: usize,
}

/// FFI-safe byte slice with optional custom deallocator.
///
/// **Ownership:** If `free_fn` is Some, caller must call it with `free_ctx`
/// to reclaim `data`. If None, caller manages the memory externally.
#[repr(C)]
pub struct FfiBytes {
    pub data: *const u8,
    pub len: usize,
    pub free_fn: Option<extern "C" fn(*mut c_void)>,
    pub free_ctx: *mut c_void,
}

pub type EventCallback = extern "C" fn(
    event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    user_data: *mut c_void,
);

impl FfiString {
    /// Safely read an FfiString as a Rust String. Returns default on null/invalid.
    pub fn to_string_lossy(&self, default: &str) -> String {
        if self.data.is_null() || self.len == 0 {
            return default.to_string();
        }
        unsafe {
            std::str::from_utf8(std::slice::from_raw_parts(self.data as *const u8, self.len))
                .unwrap_or(default)
                .to_string()
        }
    }
}

impl FfiResult {
    pub fn ok(data: *mut c_void, len: usize) -> Self {
        Self {
            error_code: 0,
            error_message: std::ptr::null_mut(),
            data,
            data_len: len,
        }
    }

    pub fn error(code: i32, msg: &str) -> Self {
        // Strip null bytes to prevent CString::new panic
        let safe_msg = msg.replace('\0', "");
        let c_msg = std::ffi::CString::new(safe_msg).unwrap_or_else(|_| {
            std::ffi::CString::new("error message contained null bytes").unwrap()
        });
        Self {
            error_code: code,
            error_message: c_msg.into_raw(),
            data: std::ptr::null_mut(),
            data_len: 0,
        }
    }
}

impl Drop for FfiResult {
    fn drop(&mut self) {
        if !self.error_message.is_null() {
            unsafe {
                let _ = std::ffi::CString::from_raw(self.error_message);
            }
        }
    }
}

impl Drop for FfiBytes {
    fn drop(&mut self) {
        if let Some(free_fn) = self.free_fn {
            if !self.free_ctx.is_null() {
                free_fn(self.free_ctx);
            }
        }
    }
}

/// Free an FfiResult returned by FFI functions.
/// Takes ownership — Drop impl frees error_message.
#[no_mangle]
pub extern "C" fn catcher_free_result(result: FfiResult) {
    drop(result);
}

/// Free event data strings allocated by Rust for the FFI callback bridge.
///
/// Rust calls `CString::into_raw()` to transfer ownership of the event type
/// and event data strings to the Dart callback. Dart must call this function
/// after reading the data to prevent memory leaks.
///
/// Note: `event_data` type is `*mut u8` (matching EventCallback's `*const u8`).
/// The const-to-mut cast is safe because into_raw() returns a mutable pointer
/// that was originally passed as const through the callback.
#[no_mangle]
pub extern "C" fn catcher_free_event_data(
    event_type: *mut c_char,
    event_data: *mut u8,
) {
    unsafe {
        if !event_type.is_null() {
            let _ = std::ffi::CString::from_raw(event_type);
        }
        if !event_data.is_null() {
            let _ = std::ffi::CString::from_raw(event_data as *mut c_char);
        }
    }
}

// Safety: FfiResult, FfiString, FfiBytes are all #[repr(C)] and contain
// only FFI-safe primitive types or function pointers.
unsafe impl Send for FfiResult {}
unsafe impl Send for FfiString {}
unsafe impl Send for FfiBytes {}
