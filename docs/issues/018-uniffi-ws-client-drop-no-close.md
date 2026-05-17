# Resource Leak: UniFFI `WsClient::drop` 不关闭底层 WebSocket 连接

**严重程度**: 🟡 Medium — Drop 后连接和 task 继续运行，资源泄露

**状态**: Open

**位置**: `packages/catcher-uniffi/src/lib.rs:467-471`

---

## 当前代码

```rust
impl Drop for WsClient {
    fn drop(&mut self) {
        self._event_task.abort();  // ← 只停事件转发，不停连接
    }
}
```

## 问题

`WsClient` 持有 `handle: Arc<WsHandle>` 和 `_event_task: JoinHandle<()>`。

当 `WsClient` 被 drop 时：
1. `_event_task.abort()` — 停止事件转发，observer 不再收到事件 ✅
2. `Arc<WsHandle>` 的 refcount 减 1 — 但 `connection_manager` task 仍然持有 `cmd_rx` 和 `event_tx`，连接继续存活
3. WebSocket 连接保持打开，heartbeat 继续发送，重连逻辑继续运行
4. Swift/Kotlin 的 observer 已被 GC，但 Rust 侧仍在消耗 CPU + 网络资源

当 `WsClient` 在 Swift/Kotlin 侧被释放时，调用方期望连接关闭。当前实现不会。

## 修复

```rust
impl Drop for WsClient {
    fn drop(&mut self) {
        // 先关闭连接
        let _ = self.handle.close(1000, "client dropped");
        // 再停止事件转发
        self._event_task.abort();
    }
}
```

## 关联

- `WsHandle::close()` 发送 Close 帧到 `cmd_tx`，触发 `connection_manager` 的 `CleanClose` 路径
- `SseClientHandle` 也有类似问题（无 Drop 实现，底层 SSE client 无法停止）
