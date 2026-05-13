# Review Round 3 — Rust FFI + UniFFI

**Date:** Round 3 (after 36 issues found and fixed in rounds 1-2)

## CRITICAL

### 1. Duplicate `catcher_free_result` symbol — linker error
- **File:** `packages/catcher-ws/src/ffi/ws_ffi.rs:188-189`
- Both `catcher-core/src/ffi_types.rs:92` and `catcher-ws/src/ffi/ws_ffi.rs:188` define
  `#[no_mangle] pub extern "C" fn catcher_free_result`. When `catcher-ws` links
  `catcher-core`, the linker sees two definitions of the same symbol → duplicate symbol error.
- **Fix:** Remove the duplicate from `ws_ffi.rs` — the one in `catcher-core` is canonical.

### 2. `block_on_aux_thread` OnceLock is broken for concurrent calls
- **File:** `packages/catcher-uniffi/src/lib.rs:42-51`
- `OnceLock<Runtime>` with `new_current_thread()` means the first thread creates the runtime.
  Subsequent threads get a reference to the SAME single-threaded runtime.
  `current_thread::Runtime::block_on()` will panic if called from a different thread than
  the one that created it, or if called concurrently.
- **Fix:** Remove `OnceLock`, create a new `current_thread` runtime per thread. Overhead is
  acceptable for synchronous UniFFI calls.

## HIGH

### 3. No tokio runtime in FFI crates — `tokio::task::spawn` will panic
- **File:** `packages/catcher-ws/src/ffi/ws_ffi.rs:84` and `packages/catcher-http/src/ffi/http_ffi.rs:101`
- Both use `tokio::task::spawn` and `tokio::task::spawn_blocking` without ensuring a tokio
  runtime exists. When called from Dart, there is no tokio runtime context → panic.
- **Fix:** Add a `runtime()` helper (like in uniffi/lib.rs) and use `runtime().spawn()` instead.

## MEDIUM

### 4. `ffi_string_to_string` duplicated in 3 files
- **Files:** `ws_ffi.rs:20-29`, `http_ffi.rs:20-29`, `quality_ffi.rs:10-19`
- Same function copy-pasted 3 times. Should be a method on `FfiString` in `catcher-core`.

### 5. `error_json` and `invoke_*_callback` duplicated
- `error_json` in ws_ffi.rs and http_ffi.rs.
- `invoke_event_callback` / `invoke_http_callback` are identical except name.
- Should be shared utilities.

## Total: 5 issues (2 CRITICAL, 1 HIGH, 2 MEDIUM)
