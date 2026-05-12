use std::ffi::{c_char, c_void};

/// FFI 安全的结果类型
#[repr(C)]
pub struct FfiResult {
    pub error_code: i32, // 0 = 成功
    pub error_message: *mut c_char,
    pub data: *mut c_void,
    pub data_len: usize,
}

#[repr(C)]
pub struct FfiString {
    pub data: *const c_char,
    pub len: usize,
}

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
        let c_msg = std::ffi::CString::new(msg).unwrap();
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

// Safety: FfiResult, FfiString, FfiBytes are all #[repr(C)] and contain
// only FFI-safe primitive types or function pointers.
unsafe impl Send for FfiResult {}
unsafe impl Send for FfiString {}
unsafe impl Send for FfiBytes {}
