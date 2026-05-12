//! HTTP C ABI — create / get / post / destroy
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{Arc, Mutex};

use crate::transport::http_client::HttpTransport;
use crate::types::http::HttpClientConfig;

use catcher_core::{EventCallback, FfiString};

static HANDLES: Mutex<Option<HashMap<usize, Arc<HttpTransport>>>> = Mutex::new(None);
static NEXT_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn handles() -> std::sync::MutexGuard<'static, Option<HashMap<usize, Arc<HttpTransport>>>> {
    HANDLES.lock().unwrap()
}

#[no_mangle]
pub extern "C" fn catcher_http_client_create(config_json: *const c_char) -> *mut c_void {
    if config_json.is_null() {
        return std::ptr::null_mut();
    }
    let json = unsafe { CStr::from_ptr(config_json) };
    let config: HttpClientConfig = match serde_json::from_str(json.to_str().unwrap_or("")) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    let transport = match HttpTransport::new(config) {
        Ok(t) => t,
        Err(_) => return std::ptr::null_mut(),
    };
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    handles()
        .get_or_insert_with(HashMap::new)
        .insert(id, Arc::new(transport));
    Box::into_raw(Box::new(id)) as *mut c_void
}

#[no_mangle]
pub extern "C" fn catcher_http_get(
    handle: *mut c_void,
    url: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = unsafe { *(handle as *const usize) };
    let url_str = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(url.data as *const u8, url.len))
            .unwrap_or("/")
    }
    .to_string();
    let ud = user_data as usize;

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        tokio::task::spawn(async move {
            let result = t.get(&url_str).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            };
            let c_event = std::ffi::CString::new("http_result").unwrap();
            callback(
                c_event.as_ptr(),
                json.as_ptr(),
                json.len(),
                ud as *mut c_void,
            );
        });
    }
}

#[no_mangle]
pub extern "C" fn catcher_http_post(
    handle: *mut c_void,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
) {
    if handle.is_null() {
        return;
    }
    let id = unsafe { *(handle as *const usize) };
    let url_str = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(url.data as *const u8, url.len))
            .unwrap_or("/")
    }
    .to_string();
    let body_data = unsafe { std::slice::from_raw_parts(body, body_len) }.to_vec();
    let ct_str = unsafe {
        std::str::from_utf8(std::slice::from_raw_parts(
            content_type.data as *const u8,
            content_type.len,
        ))
        .unwrap_or("application/octet-stream")
    }
    .to_string();
    let ud = user_data as usize;

    let transport = handles().as_ref().and_then(|m| m.get(&id)).cloned();
    if let Some(t) = transport {
        tokio::task::spawn(async move {
            let result = t.post(&url_str, &body_data, &ct_str).await;
            let json = match result {
                Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
                Err(e) => format!("{{\"error\":\"{e}\"}}"),
            };
            let c_event = std::ffi::CString::new("http_result").unwrap();
            callback(
                c_event.as_ptr(),
                json.as_ptr(),
                json.len(),
                ud as *mut c_void,
            );
        });
    }
}

#[no_mangle]
pub extern "C" fn catcher_http_client_destroy(handle: *mut c_void) {
    if handle.is_null() {
        return;
    }
    let id = unsafe { *(handle as *const usize) };
    handles().as_mut().map(|m| m.remove(&id));
    unsafe {
        drop(Box::from_raw(handle as *mut usize));
    }
}
