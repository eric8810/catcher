# Review Round 2 — Dart FFI Bindings

**Date:** 2026-05-13  
**Scope:** `ffi_bindings.dart`, `ws_client.dart`, `http_client.dart`, `catcher_core.dart`  
**Cross-referenced:** Rust `ffi_types.rs`, `http_ffi.rs`, `ws_ffi.rs`, `quality_ffi.rs`, `types/*.rs`

---

## Summary

Reviewed all four Dart files against their Rust FFI counterparts. Found **1 HIGH**, **2 MEDIUM**, and **5 LOW** issues. The struct layouts and typedefs in `ffi_bindings.dart` are correct. The main risks are in resource lifecycle management.

---

## Issues

### Issue 1 — `http_client.dart`: Double-dispose causes use-after-free (UB in Rust)

**Severity: HIGH**  
**File:** `packages/catcher_core/lib/src/http_client.dart:24,81-85`

`_handle` is declared as `late final Pointer<Void>` (non-nullable, immutable after constructor). The `dispose()` method cannot null it out:

```dart
// line 24
late final Pointer<Void> _handle;

// line 81-85
void dispose() {
  if (_handle != nullptr) {
    _destroy(_handle);
  }
}
```

Calling `dispose()` a second time passes the already-freed pointer to Rust `catcher_http_client_destroy`, which dereferences it (`*(handle as *const usize)`) and then calls `Box::from_raw` again — **use-after-free + double-free**.

By contrast, `ws_client.dart` correctly uses `Pointer<Void>?` and sets `_handle = null` after destroy (line 133).

**Fix:** Change to `Pointer<Void>? _handle`, null it after `_destroy`, and guard all `_handle` usages (like ws_client.dart's `_ensureHandle`).

---

### Issue 2 — `http_client.dart`: NativeCallable + ReceivePort leaked on timeout if Rust never calls back

**Severity: MEDIUM**  
**File:** `packages/catcher_core/lib/src/http_client.dart:121-226`

When the 30s timeout fires (line 212-226), `onTimeout` completes the completer with an error but deliberately does NOT close `nativeCallback` or `receivePort` (to avoid UB if Rust calls back later). The comment says "The callback will be cleaned up by GC" — but `NativeCallable.listener` native resources are **not** reclaimed by GC; `close()` must be called explicitly.

If the late callback eventually arrives, the `sub` listener cleans up correctly. But if Rust never invokes the callback (e.g., tokio runtime shutdown, task panic), the `NativeCallable` and `ReceivePort` leak permanently.

**Fix:** Add a safety-net timer (e.g., 60s after timeout) that force-closes the NativeCallable and ReceivePort, since by then Rust has certainly finished or been cancelled.

---

### Issue 3 — `ws_client.dart`: Callback may fire after `catcher_ws_destroy` but before `NativeCallable.close()`

**Severity: MEDIUM**  
**File:** `packages/catcher_core/lib/src/ws_client.dart:130-140`

In `dispose()`, `_destroy(_handle!)` removes the handle from Rust's HashMap and frees the Box. However, the spawned tokio task holds an `Arc<WsHandle>` and continues running. It may invoke the callback one more time between `_destroy` and `_nativeCallback?.close()`. After `close()`, any further Rust invocation of the function pointer is UB.

```dart
void dispose() {
  if (_handle != null && _handle != nullptr) {
    _destroy(_handle!);      // line 132 — tokio task may still be running
    _handle = null;
  }
  _nativeCallback?.close();  // line 135 — closes after destroy
  // ...
}
```

**Fix:** Close the NativeCallable BEFORE destroying the handle (reverse the order), or call `close()` on the WsHandle first to stop the event loop, then destroy.

---

### Issue 4 — `http_client.dart`: No use-after-dispose protection on `_handle`

**Severity: LOW**  
**File:** `packages/catcher_core/lib/src/http_client.dart:111-193`

Unlike `ws_client.dart` which has `_ensureHandle()`, `http_client.dart`'s `_execute()` uses `_handle` directly without checking validity. If `dispose()` is called and then `get()`/`post()` is invoked, it passes a freed pointer to Rust.

**Fix:** Add an `_ensureHandle()` guard and make `_handle` nullable (see Issue 1).

---

### Issue 5 — `http_client.dart`: `catcher_free_event_data` lookup on every callback invocation

**Severity: LOW**  
**File:** `packages/catcher_core/lib/src/http_client.dart:129-131`

The free function is looked up from the DynamicLibrary on every callback invocation:

```dart
final freeFn = _lib.lookupFunction<CatcherFreeEventDataNative,
    CatcherFreeEventDataDart>('catcher_free_event_data');
```

This is a hash-table lookup + string comparison per HTTP response. Should be cached as a field.

**Fix:** Cache the resolved function pointer as a `late final` field, matching the pattern used for `_create`/`_destroy`.

---

### Issue 6 — `http_client.dart` / `ws_client.dart`: Duplicated `_allocFfiString` / `_freeFfiString`

**Severity: LOW**  
**Files:**
- `packages/catcher_core/lib/src/http_client.dart:90-105`
- `packages/catcher_core/lib/src/ws_client.dart:150-165`

Identical helper methods are copy-pasted in both clients. Divergence risk if one is updated without the other.

**Fix:** Extract to a shared utility (e.g., `lib/src/ffi_utils.dart`).

---

### Issue 7 — Duplicate struct definitions in `ffi_bindings.dart` and `ffi_types.dart`

**Severity: LOW**  
**Files:**
- `packages/catcher_core/lib/src/ffi_bindings.dart:9-27` (`FfiStringNative`, `FfiResultNative`)
- `packages/catcher_core/lib/src/ffi_types.dart:4-22` (`FfiString`, `FfiResult`)

Both files define identical FFI structs with different names. `EventCallbackNative`/`EventCallbackDart` are also duplicated. If one is updated (e.g., adding a field), the other may silently diverge.

Note: `ffi_types.dart` also defines `FfiBytes` (matching Rust `FfiBytes`) which is absent from `ffi_bindings.dart`. The files in `models/` appear to use the `ffi_types.dart` versions while `ws_client.dart`/`http_client.dart` use the `ffi_bindings.dart` versions.

**Fix:** Consolidate to a single definition site; re-export from one canonical file.

---

### Issue 8 — `http_client.dart`: New NativeCallable allocated per request

**Severity: LOW**  
**File:** `packages/catcher_core/lib/src/http_client.dart:121`

Every `_execute()` call creates a fresh `NativeCallable<EventCallbackDart>.listener(...)`. For high-throughput scenarios, this allocates and registers a native trampoline per HTTP request. The ws_client correctly reuses a single NativeCallable.

**Fix:** Consider a shared callback with a request-ID dispatch map, or at minimum document this as a known cost.

---

## Items Verified — No Issues Found

### `ffi_bindings.dart` — Struct layouts ✅

| Dart Struct | Rust Struct | Fields Match | Alignment |
|---|---|---|---|
| `FfiStringNative` | `FfiString` | `data: *const c_char`, `len: usize` | ✅ |
| `FfiResultNative` | `FfiResult` | `error_code: i32`, `error_message: *mut c_char`, `data: *mut c_void`, `data_len: usize` | ✅ (Dart FFI inserts correct padding after Int32) |

### `ffi_bindings.dart` — Typedefs ✅

All 12 function typedef pairs verified against Rust signatures:
- `EventCallback` — `extern "C" fn(*const c_char, *const u8, usize, *mut c_void)` → `Pointer<Char>, Pointer<Uint8>, Size, Pointer<Void>` ✅
- `catcher_http_client_create / destroy` ✅
- `catcher_http_get / post / execute` ✅
- `catcher_ws_create / send_text / send_binary / close / destroy` ✅
- `catcher_free_result / catcher_free_event_data` ✅
- `catcher_pack / catcher_unpack` ✅

### `ws_client.dart` — Memory management ✅

- `_checkResult` correctly reads error message THEN calls `_freeResult` (copies before freeing) — line 167-175
- `_freeEventData` called after every callback, data copied beforehand — line 58-63
- `_allocFfiString` uses `utf8.encode()` for correct byte-length measurement — line 151
- Successful `FfiResult` (errorCode == 0) correctly skips `_freeResult` — nothing to free (error_message is null) — line 168

### `http_client.dart` — FFI string management ✅

- `FfiStringNative` structs passed by value to Rust; Rust copies data immediately (`.to_string()`); Dart frees afterward — line 184-210
- `bodyPtr` correctly set to null pointer for empty body; Rust handles null — line 172-174
- `catcher_free_event_data` called after copying callback data — line 129-131

### `catcher_core.dart` — Exports ✅

All public API types are correctly exported with `show` lists:
- HTTP: `CatcherHttpClient`, `HttpClientConfig`, `RetryConfig`, `CircuitBreakerConfig`, `PoolConfig`, `HttpResponse`, `CatcherHttpError`
- WS: `CatcherWsClient`, `WsClientConfig`, `WsReconnectConfig`, `WsHeartbeatConfig`, all `WsEvent` subtypes, `CatcherWsError`
- FFI: `loadCatcherLibrary`

---

## Observations (Out of Scope)

These files exist in the repo but are not exported and appear to be work-in-progress with compile errors:

1. **`quality.dart`** — Missing `catcher_free_event_data` call (memory leak). Also uses `ReceivePort.fromRawReceivePort(int)` which is not a valid API. Incorrect import path `../ffi_bindings.dart`.

2. **`codec.dart`** — Calls `bindings.catcherPack(...)` but `catcherPack` is a typedef, not a resolved function. Incorrect import path `../ffi_bindings.dart`.

3. **`test/catcher_core_test.dart`** — References `config.keepAlive` on `HttpClientConfig` (field doesn't exist; it's nested in `pool`), and `WsClientConfig()` without required `urls` parameter, and `WsEvent.fromJson()` (the exported `WsEvent` is abstract with no `fromJson`). The test appears to target the `models/` versions of these classes rather than the `ws_client.dart`/`http_client.dart` versions exported by the barrel file.

4. **`models/*.dart`** — These files define alternative (simpler) versions of the same classes with different structures (e.g., `WsEvent` as a flat class with optional fields vs. the ws_client.dart's abstract class with typed subclasses). They are not exported and appear to be an older or parallel implementation.
