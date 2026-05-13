# Review Round 3 — Dart FFI Bindings

**Date:** Round 3 (after 36 issues found and fixed in rounds 1-2)

## HIGH

### 1. `_freeResult` looks up function on every call
- **File:** `packages/catcher_core/lib/src/ws_client.dart:182-186`
- The `_freeResult` method calls `_lib!.lookupFunction` on every invocation.
  Should be cached as a field like `_freeEventDataFn` in `http_client.dart`.

### 2. `_freeEventData` looks up function on every call
- **File:** `packages/catcher_core/lib/src/ws_client.dart:190-193`
- Same issue — `_freeEventData` does a lookup on every callback event, which is hot path.

## MEDIUM

### 3. `CatcherFreeEventDataNative` type mismatch for `eventData`
- **File:** `packages/catcher_core/lib/src/ffi_bindings.dart:191-193`
- Rust declares `event_data: *mut u8` but Dart binding uses `Pointer<Char> eventData`.
  Should be `Pointer<Uint8>` to match the Rust type. Works because both are pointer-sized,
  but semantically incorrect.

## Total: 3 issues (2 HIGH, 1 MEDIUM)
