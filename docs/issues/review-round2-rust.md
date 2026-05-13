# Review Round 2: Rust FFI + UniFFI

**Date:** 2026-05-13  
**Scope:** All recently modified Rust FFI and UniFFI files  
**Reviewer:** Automated review pass  

---

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 3     |
| HIGH     | 4     |
| MEDIUM   | 7     |
| LOW      | 3     |

---

## CRITICAL Issues

### C-01. `catcher_http_post`: null `body` pointer causes undefined behavior

**File:** `packages/catcher-http/src/ffi/http_ffi.rs:108`

```rust
let body_data = std::slice::from_raw_parts(body, body_len).to_vec();
```

`body` is `*const u8` with **no null check**. Calling `from_raw_parts` with a null pointer is undefined behavior regardless of `body_len` (even `body_len == 0` is UB because Rust references must be non-null and aligned).

`catcher_http_execute` (line 157) correctly guards with `if !body.is_null() && body_len > 0`, but `catcher_http_post` does not. Inconsistent and dangerous.

**Fix:** Add a null guard before slicing, consistent with `catcher_http_execute`:
```rust
let body_data = if !body.is_null() && body_len > 0 {
    std::slice::from_raw_parts(body, body_len).to_vec()
} else {
    Vec::new()
};
```

---

### C-02. `WsEventObserver` callback runs on a tokio thread — `block_on()` re-entrance panic

**File:** `packages/catcher-uniffi/src/lib.rs:268-292`

```rust
runtime().spawn(async move {
    while let Some(event) = rx.recv().await {
        // ...
        observer.on_event(dto);  // ← runs on tokio worker thread
    }
});
```

UniFFI callback interface dispatch is synchronous: `observer.on_event(dto)` runs on the current tokio worker thread. If the foreign-language (Swift/Kotlin) implementation of `on_event` calls back into Rust and invokes any `HttpClient` method, the `runtime().block_on()` call (e.g., line 90) will panic:

```
Cannot start a runtime from within a runtime
```

This is a **very common usage pattern** (receive a WS event → make an HTTP request), making this likely to trigger in production.

**Fix options:**
1. Use `tokio::task::spawn_blocking` + `block_on` for HTTP methods, or better, restructure to avoid `block_on` by using `tokio::runtime::Handle::spawn` + `futures::block_on` on a separate thread.
2. Use a dedicated thread for observer callbacks: dispatch `observer.on_event()` via `std::thread::spawn` so it runs outside the tokio runtime.
3. Replace `block_on` with `spawn` + oneshot channel, and have the foreign side await results asynchronously (requires restructuring the UniFFI API).

---

### C-03. `catcher_ws_send_binary`: null `data` pointer causes undefined behavior

**File:** `packages/catcher-ws/src/ffi/ws_ffi.rs:126`

```rust
let bytes = std::slice::from_raw_parts(data, len);
```

No null check on `data` (`*const u8`). If the caller passes null with `len > 0`, this is immediate UB. Even `len == 0` with a null pointer violates Rust's reference requirements.

**Fix:** Add a null + length guard:
```rust
if data.is_null() {
    return FfiResult::error(1, "null data pointer");
}
let bytes = std::slice::from_raw_parts(data, len);
```

---

## HIGH Issues

### H-01. `catcher_http_get` / `catcher_http_post` / `catcher_http_execute`: no null check on `FfiString.data`

**Files:**
- `packages/catcher-http/src/ffi/http_ffi.rs:73` (`url.data`)
- `packages/catcher-http/src/ffi/http_ffi.rs:105` (`url.data`)
- `packages/catcher-http/src/ffi/http_ffi.rs:148` (`method.data`)
- `packages/catcher-http/src/ffi/http_ffi.rs:154` (`url.data`)
- `packages/catcher-http/src/ffi/http_ffi.rs:164` (`content_type.data` — partially guarded but only with `.is_null()` + `.len > 0`)

Each `FfiString.data` is `*const c_char`. If null with non-zero `len`, `std::slice::from_raw_parts` causes UB.

**Fix:** Add a null guard at the top of each FFI function for every `FfiString` argument:
```rust
if url.data.is_null() {
    return; // or return FfiResult::error(...)
}
```

---

### H-02. `quality_ffi`: `host` parameter is completely ignored

**File:** `packages/catcher-http/src/ffi/quality_ffi.rs:15-18`

```rust
let _host_str =
    std::str::from_utf8(std::slice::from_raw_parts(host.data as *const u8, host.len))
        .unwrap_or("https://www.example.com")
        .to_string();
```

The variable is named `_host_str` (underscore-prefixed = intentionally unused). The `host` parameter is parsed and then discarded. `NetworkQualityEvaluator::new(20)` is called with a fixed parameter and no host. Callers expect the evaluation to be specific to the provided host, but all calls produce identical results regardless of input.

Additionally, `host.data` has no null check — same UB risk as other FfiString fields.

**Fix:** Either pass `host_str` into the evaluator or document that `host` is currently unused. Also add null guard.

---

### H-03. `quality_ffi`: synchronous `evaluate()` blocks tokio worker thread

**File:** `packages/catcher-http/src/ffi/quality_ffi.rs:21-24`

```rust
tokio::task::spawn(async move {
    let evaluator = NetworkQualityEvaluator::new(20);
    let result = evaluator.evaluate();  // ← sync fn, not async
```

`evaluate()` is a synchronous function that computes RTT statistics. While the current implementation appears lightweight (in-memory math), it runs inside `tokio::spawn` and blocks the async executor. If the evaluator grows to do I/O (pings, DNS resolution), this becomes a serious bottleneck.

**Fix:** Use `tokio::task::spawn_blocking` for CPU-bound or potentially blocking work:
```rust
tokio::task::spawn_blocking(move || { ... })
```

---

### H-04. JSON injection via error message string interpolation

**Files:**
- `packages/catcher-ws/src/ffi/ws_ffi.rs:81`
- `packages/catcher-http/src/ffi/http_ffi.rs:84`
- `packages/catcher-http/src/ffi/http_ffi.rs:123`
- `packages/catcher-http/src/ffi/http_ffi.rs:199`

```rust
let json = format!("{{\"error\":\"{e}\"}}");
```

If the error message `e` contains `"`, `\`, or control characters, the resulting string is **not valid JSON**. Example: if `e = 'connection "timeout"'`, the output is `{"error":"connection "timeout""}`.

This breaks JSON parsing on the Dart/Swift/Kotlin side.

**Fix:** Use `serde_json::json!` or escape the error string:
```rust
let json = serde_json::json!({ "error": e.to_string() }).to_string();
```

---

## MEDIUM Issues

### M-01. `FfiBytes::free_fn` is never called — memory leak

**File:** `packages/catcher-core/src/ffi_types.rs:18-24`

```rust
pub struct FfiBytes {
    pub data: *const u8,
    pub len: usize,
    pub free_fn: Option<extern "C\" fn(*mut c_void)>,
    pub free_ctx: *mut c_void,
}
```

`FfiBytes` has a `free_fn` field intended to allow custom deallocation, but there is **no `Drop` implementation** that calls it. Any memory pointed to by `data` that requires `free_fn` to be freed will leak.

**Fix:** Add a `Drop` impl or remove `free_fn`/`free_ctx` if unused.

---

### M-02. `catcher_ws_create`: connection is async but handle is returned immediately (race condition)

**File:** `packages/catcher-ws/src/ffi/ws_ffi.rs:67-88`

```rust
tokio::task::spawn(async move {
    match WsTransport::connect(&first_url, &config).await {
        Ok((handle, mut rx)) => {
            ws_handles().get_or_insert_with(HashMap::new).insert(id, ws_handle);
            // ...
        }
        Err(e) => {
            // sends error callback, but handle ID was never registered
        }
    }
});
Box::into_raw(Box::new(id)) as *mut c_void  // ← returned immediately
```

The caller receives a valid handle pointer immediately, but the handle ID is only registered in `WS_HANDLES` after the async connection succeeds. Any `catcher_ws_send_text` / `catcher_ws_send_binary` call before connection completes will get `"handle not found"`. On connection failure, the handle ID is never registered, but the pointer is still valid — the caller must still call `catcher_ws_destroy`.

**Fix:** Either block until connected (like the UniFFI WsClient does), or document that callers must wait for a `ws_event`/`ws_error` callback before using the handle.

---

### M-03. `catcher_ws_create`: silent fallback to `ws://localhost`

**File:** `packages/catcher-ws/src/ffi/ws_ffi.rs:58-61`

```rust
let first_url = urls
    .first()
    .cloned()
    .unwrap_or_else(|| "ws://localhost".into());
```

If `config.urls` is empty, the code silently connects to `ws://localhost`. This is almost certainly wrong in production. The UniFFI `WsClient::new` (lib.rs:257-260) correctly returns an error in this case.

**Fix:** Return null (or send error callback) if `urls` is empty.

---

### M-04. `catcher_free_event_data` type mismatch with `EventCallback` signature

**File:** `packages/catcher-core/src/ffi_types.rs:26-31, 70-82`

`EventCallback` provides `event_data` as `*const u8`:
```rust
pub type EventCallback = extern "C\" fn(
    event_type: *const c_char,
    event_data: *const u8,    // ← *const u8
    event_data_len: usize,
    user_data: *mut c_void,
);
```

But `catcher_free_event_data` takes `event_data: *mut c_char`:
```rust
pub extern "C\" fn catcher_free_event_data(
    event_type: *mut c_char,
    event_data: *mut c_char,  // ← *mut c_char
)
```

The Dart side must cast `*const u8` → `*mut c_char` when calling the free function. This is an error-prone API contract that should use consistent types.

**Fix:** Change `catcher_free_event_data` to accept `*mut u8` for `event_data`, matching the callback signature (or document the required cast).

---

### M-05. `invoke_event_callback` / `invoke_http_callback`: `CString::new(json).unwrap()` can panic on null bytes

**Files:**
- `packages/catcher-ws/src/ffi/ws_ffi.rs:31`
- `packages/catcher-http/src/ffi/http_ffi.rs:30`

```rust
let c_json = CString::new(json).unwrap();
```

If the JSON string contains an interior null byte (possible in arbitrary WebSocket message data or error messages), `CString::new()` returns `Err` and `.unwrap()` panics, crashing the process.

**Fix:** Replace null bytes or use `unwrap_or_default()`:
```rust
let c_json = CString::new(json.replace('\0', "")).unwrap_or_default();
```

---

### M-06. `WsClient` spawned event-forwarding task outlives the `WsClient` object

**File:** `packages/catcher-uniffi/src/lib.rs:268-292`

The `observer` is moved into a `runtime().spawn()` task. When `WsClient` is dropped on the foreign-language side, the spawned task continues running and calling `observer.on_event()` until the `mpsc` channel closes. There is no cancellation mechanism.

This means:
1. Events are delivered to an observer whose owning object may have been garbage-collected in Kotlin/Swift.
2. The observer's foreign-language reference is kept alive by Rust, potentially preventing GC.

**Fix:** Use a `tokio::sync::watch` or `CancellationToken` pattern to abort the spawned task when `WsClient` is dropped. Store a `JoinHandle` in `WsClient` and abort it on `Drop`.

---

### M-07. `catcher_http_execute`: unknown HTTP method silently falls back to GET

**File:** `packages/catcher-http/src/ffi/http_ffi.rs:176-183`

```rust
let http_method = match method_str.to_uppercase().as_str() {
    "GET" => HttpMethod::GET,
    "POST" => HttpMethod::POST,
    // ...
    _ => HttpMethod::GET,  // ← silent fallback
};
```

If the caller passes `"HEAD"`, `"OPTIONS"`, or a typo like `"PSOT"`, it silently becomes `GET`. This can cause unexpected behavior (e.g., a DELETE-like endpoint receiving GET).

**Fix:** Return an error for unrecognized methods instead of defaulting to GET.

---

## LOW Issues

### L-01. `FfiResult::error()` uses `.unwrap()` on `CString::new()`

**File:** `packages/catcher-core/src/ffi_types.rs:44`

```rust
let c_msg = std::ffi::CString::new(msg).unwrap();
```

Will panic if `msg` contains a null byte. Error messages rarely contain null bytes, but `.unwrap()` in an error-construction path is risky — the last thing you want when reporting an error is another error.

**Fix:** Use `unwrap_or_else` with a fallback or `.replace('\0', "")`.

---

### L-02. `FfiString` and `FfiBytes` have no `Drop` implementation, inconsistent with `FfiResult`

**File:** `packages/catcher-core/src/ffi_types.rs:12-24`

`FfiResult` has a `Drop` impl that frees `error_message`, but `FfiString` and `FfiBytes` have no `Drop` impl. This is intentional (caller-managed memory for FFI), but not documented. The inconsistency can confuse future maintainers.

**Fix:** Add documentation comments explaining ownership semantics for each struct.

---

### L-03. `WsEventDto` variant mapping is fragile — uses exhaustive match

**File:** `packages/catcher-uniffi/src/lib.rs:270-289`

The `WsEvent` → `WsEventDto` mapping uses an exhaustive match, which is correct but fragile: if `WsEvent` gains a new variant, this code won't compile until the match is updated. This is actually good (compile-time error), but the mapping is duplicated if any other consumer needs it.

**Fix:** Consider adding a `From<WsEvent> for WsEventDto` impl in a shared location.

---

## Files Reviewed

| File | Status |
|------|--------|
| `packages/catcher-core/src/ffi_types.rs` | ✅ Reviewed |
| `packages/catcher-ws/src/ffi/ws_ffi.rs` | ✅ Reviewed |
| `packages/catcher-http/src/ffi/http_ffi.rs` | ✅ Reviewed |
| `packages/catcher-http/src/ffi/quality_ffi.rs` | ✅ Reviewed |
| `packages/catcher-uniffi/src/lib.rs` | ✅ Reviewed |
| `packages/catcher-uniffi/Cargo.toml` | ✅ Reviewed |

### Cargo.toml Notes

`packages/catcher-uniffi/Cargo.toml` — No issues found. Dependencies are correctly specified:
- `uniffi = "0.28"` with `cli` and `build` features ✓
- `tokio` with `rt-multi-thread` and `macros` ✓
- `thiserror` and `serde_json` ✓
- `crate-type = ["cdylib", "staticlib"]` correct for mobile library ✓
