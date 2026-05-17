# Bug: FFI 流式请求的 per-request cancel 不生效

**严重程度**: 🟡 Medium — `catcher_http_cancel_request` 对 `catcher_http_execute_stream` 发出的请求无效

**状态**: Open

**位置**: `packages/catcher-http/src/ffi/http_ffi.rs:386-416` + `packages/catcher-http/src/transport/http_client.rs:390-469`

---

## 问题

`catcher_http_execute_stream` 调用了 `allocate_pending_request()` 并注册了 per-request CancellationToken，但 `execute_stream()` 方法内部**只监听 `global_token`**，不监听 per-request token。

```rust
// http_ffi.rs:388
let (request_id, _pt) = t.allocate_pending_request();  // _pt 立即 drop
runtime().spawn(async move {
    let _ = t.execute_stream(req, move |event| { ... }).await;
});
```

```rust
// http_client.rs:426-437 — execute_stream 内部
let global_token = {
    let token = self.cancel_token.lock().unwrap();
    token.clone()
};

let response = tokio::select! {
    r = req.send() => ...,
    _ = global_token.cancelled() => { ... }  // ← 只监听 global token
};
```

`cancel_all()` 能生效（它同时 cancel global_token 和 per-request token 并 drain pending_requests）。但 `cancel_request(request_id)` 只 cancel per-request token，而 `execute_stream` 从不检查它。

## 对比：`execute_with_token` 正确实现

```rust
// http_client.rs:200-299 — execute_with_token 正确
tokio::select! {
    r = self.do_execute(request) => r,
    _ = per_request_token.cancelled() => { ... }  // ← 监听 per-request token
    _ = global_token.cancelled() => { ... }
}
```

## 修复

将 `execute_stream` 改为接受 per-request token 并在 select 中监听：

```rust
pub async fn execute_stream(
    &self,
    request: HttpRequest,
    per_request_token: tokio_util::sync::CancellationToken,  // ← 新增参数
    chunk_callback: impl Fn(StreamEvent) + Send + 'static,
) -> Result<(), CatcherError> {
    // ...
    let response = tokio::select! {
        r = req.send() => ...,
        _ = per_request_token.cancelled() => { ... }  // ← 新增
        _ = global_token.cancelled() => { ... }
    };
    // ...对所有 select 分支同样处理...
}
```

## 影响

- `catcher_http_cancel_request()` 对 streaming 请求返回 0（成功）但实际不取消
- 调用方以为请求已取消，但 stream 继续消耗资源
