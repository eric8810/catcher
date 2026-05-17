# Bug: 流式下载 Chunk 数据通过 `String::from_utf8_lossy` 传递，二进制数据被破坏

**严重程度**: 🔴 High — 二进制流下载数据被 UTF-8 替换字符破坏

**状态**: Open

**位置**: `packages/catcher-http/src/ffi/http_ffi.rs:412`

---

## 当前代码

```rust
crate::types::http::StreamEvent::Chunk(data) => {
    ("stream_chunk", String::from_utf8_lossy(data).to_string())
}
```

`data` 是 `bytes::Bytes`（来自 `StreamEvent::Chunk(Bytes)`）。

## 问题

1. **二进制数据破坏**：对于非 UTF-8 字节，`from_utf8_lossy` 替换为 `�`（U+FFFD），原始数据不可恢复。二进制文件下载（图片、视频、音频等）的每个 chunk 都会被破坏。

2. **双重分配**：`from_utf8_lossy` → `Cow<str>` → `.to_string()` → 新 `String`。每次 chunk 两次分配。

3. **5x 膨胀延续**：文本 chunk（有效 UTF-8）在此路径没问题。但二进制 chunk 的修复字符 `�` 是 3 字节 UTF-8 序列，而原始字节是 1 字节 → 膨胀。

## 修复

与 #019、#021 一样，对 Chunk 数据使用 base64：

```rust
crate::types::http::StreamEvent::Chunk(data) => {
    use base64::Engine;
    let data_b64 = base64::engine::general_purpose::STANDARD.encode(data);
    ("stream_chunk", serde_json::json!({
        "data_base64": data_b64,
        "request_id": request_id,
    }).to_string())
}
```

注意：这会改变 `stream_chunk` 事件的 JSON 形状。如果调用方期望 `String::from_utf8_lossy` 格式的文本数据，可能需要为文本流和二进制流提供不同的处理路径。

## 关联

- #019 FFI HTTP 响应体同根因（已修复：base64）
- #021 WS 消息体同根因（已修复：base64）
- 同类模式：napi `stream_event_to_json` 已经正确使用 base64 编码 chunk 数据
