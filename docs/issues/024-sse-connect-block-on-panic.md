# Bug: `catcher_sse_connect` 使用 `block_on`，在 tokio 线程内调用会 panic

**严重程度**: 🟡 Medium — 在 WS 回调等 tokio 上下文内调用时 re-entrance panic

**状态**: Open

**位置**: `packages/catcher-http/src/ffi/sse_ffi.rs:186`

---

## 当前代码

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_connect(...) -> *mut c_void {
    // ...
    let handle = sse_runtime().block_on(async move {  // ← block_on!
        match SseClient::connect(config).await {
            Ok(client) => {
                // ...
            }
            Err(_) => std::ptr::null_mut(),
        }
    });
    handle
}
```

## 问题

`tokio::runtime::Runtime::block_on()` 不能在 tokio 运行时上下文内调用。如果调用方在一个 tokio worker 线程上（例如，在 WS 事件的 Dart/Swift 回调中），会触发：

```
Cannot block the current thread from within a runtime.
```

这是 UniFFI `block_on_aux_thread` 设计注释中明确避免的问题：

> This avoids the `block_on()` re-entrance panic that would occur if a
> WsEventObserver callback (running on a tokio thread) calls back into
> an HttpClient method.

`catcher_sse_connect` 没有使用 `block_on_aux_thread` 或类似的保护机制。

## 修复

### 方案 A：使用与 UniFFI 相同的 `block_on_aux_thread` 模式

```rust
let handle = std::thread::spawn(move || {
    let rt = aux_runtime();
    rt.block_on(async { SseClient::connect(config).await })
}).join().expect("SSE connect panicked");
```

### 方案 B：改为异步 FFI 模式

使用 `runtime().spawn()` + 通过 `FfiResult` 回传结果（与 `catcher_ws_create` 模式一致）。

## 关联

- UniFFI `block_on_aux_thread` 设计（`uniffi/lib.rs:57-69`）
- 同样使用 `block_on` 的地方需排查（当前仅在 SSE connect 中发现）
