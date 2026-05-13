# Review Round 4 — Dart FFI Bindings

**Scope:** `ffi_bindings.dart`, `ws_client.dart`, `http_client.dart`, `catcher_core.dart`, `native_loader.dart`

**Verdict:** No new issues found.

All previously verified items remain correct:

- `FfiStringNative` / `FfiResultNative` struct layouts match `#[repr(C)]` (field order, types, alignment padding consistent between Dart and Rust on both 32-bit and 64-bit).
- `Pointer<Uint8>` used for `eventData` in `CatcherFreeEventDataNative` — correct.
- `utf8.encode()` used for byte-length calculation in `_allocFfiString` — correct.
- Both clients copy native data to Dart-owned memory **before** calling free — no use-after-free.
- `ws_client.dispose()` closes `NativeCallable` **before** destroying the Rust handle — prevents callback-after-destroy.
- `http_client._execute`: per-request `NativeCallable.listener` cleaned up in stream listener; 60s safety-net timer for Rust-never-responds case; `cleanedUp` flag prevents double-close.
- `_handle` is `Pointer<Void>?` with null-after-dispose guard; `_ensureHandle()` throws before any native call if disposed.
- `late final` caching of looked-up function symbols — no repeated lookups, no stale references.
- Constructor null/nullptr checks on handles — throws `StateError` immediately on creation failure.
