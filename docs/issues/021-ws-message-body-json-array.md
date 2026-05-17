# Performance: WebSocket 消息 `serde_json::to_string` 将 binary data 序列化为 JSON 数字数组

**严重程度**: 🔴 High — 与 #019 同类：1MB binary message → ~5MB JSON

**状态**: Open

**位置**:

| 函数 | 文件 | 行号 |
|------|------|------|
| `catcher_ws_create` event loop | `packages/catcher-ws/src/ffi/ws_ffi.rs` | 84 |
| `JsWsClient::new` event loop | `packages/catcher-napi-ws/src/lib.rs` | 79-80 |

---

## 当前代码

```rust
// ws_ffi.rs — 每个 WS 事件序列化为 JSON 字符串，发给 C 回调
while let Some(event) = rx.recv().await {
    let json = serde_json::to_string(&event).unwrap_or_default();
    invoke_event_callback(cb, "ws_event", json, ud);
}
```

```rust
// napi-ws/lib.rs — 每个 WS 事件序列化为 JSON 字符串，发给 JS 回调
while let Some(event) = rx.recv().await {
    if let (Ok(json), Some(ref t)) = (serde_json::to_string(&event), &tsfn) {
        let _ = t.call(Ok(json), ThreadsafeFunctionCallMode::Blocking);
    }
}
```

`WsEvent::Message { data: Vec<u8>, is_binary: bool }` 使用 `#[serde(tag = "type")]` 派生，`data` 字段被 serde 序列化为 JSON 数字数组：

```json
{"type":"Message","data":[0,1,2,3,...,255],"is_binary":true}
```

## 问题

与 #019 完全相同的根因：`Vec<u8>` → JSON number array，**5x 膨胀**。

| binary payload | JSON 大小 | 说明 |
|---------------|----------|------|
| 1 KB | ~5 KB | 可接受 |
| 64 KB | ~320 KB | 每个 WS 消息 |
| 1 MB | **~5 MB** | 严重 |

在高频 WS 消息场景（如实时数据推送、音视频帧），这个开销非常显著。

## 修复

### 方案 A：对 Message 变体使用 base64

```rust
// 替代 serde_json::to_string(&event)
match &event {
    WsEvent::Message { data, is_binary } if is_binary => {
        serde_json::json!({
            "type": "Message",
            "data_base64": base64_encode(data),
            "is_binary": true,
        }).to_string()
    }
    _ => serde_json::to_string(&event).unwrap_or_default(),
}
```

膨胀比从 5x 降到 ~1.3x。

### 方案 B：binary data 单独通道传递

在 FFI 层用 `FfiResult.data` 传递 binary payload，JSON 中只放元数据。

## 关联

- #019 HTTP 响应体同问题（FFI 回调路径）
