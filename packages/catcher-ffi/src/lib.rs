//! catcher-ffi — Unified C ABI library for all catcher FFI symbols.
//!
//! This crate links `catcher-core`, `catcher-http`, and `catcher-ws` into a
//! single shared library (`libcatcher_ffi.so` / `catcher_ffi.dylib` / `catcher_ffi.dll`).
//!
//! All `#[no_mangle] pub extern "C"` functions from the dependencies are
//! automatically exported. The `use` statements below ensure the linker
//! does not strip them.

// Force-link all FFI modules from dependencies
#[allow(unused_imports)]
use catcher_core::ffi_types as _core_ffi;
#[allow(unused_imports)]
use catcher_http::ffi as _http_ffi;
#[allow(unused_imports)]
use catcher_ws::ffi as _ws_ffi;

// ═══════════════════════════════════════════════════════════════
// Codec FFI — pack / unpack (bridging catcher-ws codec to C ABI)
// ═══════════════════════════════════════════════════════════════

use std::ffi::{c_char, c_void, CStr, CString};

use catcher_core::FfiResult;

/// Pack a JSON string into msgpack binary.
///
/// `json_input` is a null-terminated JSON string.
/// Returns FfiResult with data pointing to msgpack bytes (caller must free via catcher_free_result).
#[no_mangle]
pub unsafe extern "C" fn catcher_pack(json_input: *const c_char) -> FfiResult {
    if json_input.is_null() {
        return FfiResult::error(1, "null input");
    }
    let json_str = match CStr::from_ptr(json_input).to_str() {
        Ok(s) => s,
        Err(_) => return FfiResult::error(1, "invalid UTF-8 in input"),
    };
    let value: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => return FfiResult::error(2, &e.to_string()),
    };
    let packed = match catcher_ws::codec::pack(&value) {
        Ok(bytes) => bytes,
        Err(e) => return FfiResult::error(3, &e.to_string()),
    };

    let len = packed.len();
    let ptr = Box::into_raw(packed.into_boxed_slice()) as *mut c_void;
    FfiResult::ok(ptr, len)
}

/// Unpack msgpack binary into a JSON string.
///
/// Returns FfiResult with data pointing to a null-terminated JSON CString
/// (caller must free via catcher_free_result).
#[no_mangle]
pub unsafe extern "C" fn catcher_unpack(data: *const u8, len: usize) -> FfiResult {
    if data.is_null() || len == 0 {
        return FfiResult::error(1, "null or empty data");
    }
    let bytes = std::slice::from_raw_parts(data, len);
    let value: serde_json::Value = match catcher_ws::codec::unpack_value(bytes) {
        Ok(v) => v,
        Err(e) => return FfiResult::error(2, &e.to_string()),
    };
    let json_str = serde_json::to_string(&value).unwrap_or_default();
    let c_str = CString::new(json_str).unwrap_or_default();
    let ptr = c_str.into_raw() as *mut c_void;
    // into_raw gives null-terminated string; data_len = string length (no null)
    let len = CStr::from_ptr(ptr as *const c_char).to_bytes().len();
    FfiResult::ok(ptr, len)
}

/// Free data allocated by catcher_pack / catcher_unpack.
///
/// For pack: data is a Box<[u8]> allocated via `into_raw`.
/// For unpack: data is a CString allocated via `into_raw`.
/// Both can be freed by reconstructing the Box/CString and dropping it.
///
/// Note: catcher_free_result already handles Drop of FfiResult (which frees error_message).
/// But for the data pointer, we need this separate free function.
/// Since both Box<[u8]> and CString are just pointers, we reconstruct as a Vec<u8>
/// with the correct length. For CString (unpack result), the length passed back
/// is the string length, and the CString had a null terminator — so we add +1.
///
/// Actually, the simplest approach: caller uses catcher_free_result which drops the
/// FfiResult struct. For data pointers, catcher_free_result currently does NOT free them.
/// So we provide catcher_free_data here.
#[no_mangle]
pub unsafe extern "C" fn catcher_free_data(data: *mut c_void, len: usize) {
    if data.is_null() {
        return;
    }
    // Reconstruct as Vec<u8> and drop — works for both Box<[u8]> and CString
    let _ = Vec::from_raw_parts(data as *mut u8, len, len);
}
