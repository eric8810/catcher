# Performance: 流式下载每 chunk 额外拷贝 (`chunk.to_vec()`)

**严重程度**: 🟡 Low — 每 chunk 一次 O(n) 内存复制

**状态**: Open

**位置**: `packages/catcher-http/src/transport/http_client.rs:453`

---

## 当前代码

```rust
// execute_stream()
Some(Ok(chunk)) => chunk_callback(StreamEvent::Chunk(chunk.to_vec())),
```

`chunk` 类型为 `bytes::Bytes`，`to_vec()` 将其内容完整复制到一个新的 `Vec<u8>`。

## 问题

在流式下载中，每个 chunk 到达时都被复制一次。对于大文件下载（如 100MB 文件分 64KB chunks = ~1600 chunks），总共复制 100MB 额外数据。

`StreamEvent::Chunk` 定义为 `Chunk(Vec<u8>)`，强制所有权转移。如果改为 `Chunk(Bytes)`，则 `Bytes` 是引用计数共享的，`chunk_callback` 可以在不复制的情况下保存数据。

## 修复

将 `StreamEvent::Chunk` 改为使用 `Bytes`：

```rust
// types/http.rs
pub enum StreamEvent {
    Headers { status: u16, headers: HashMap<String, String> },
    Chunk(bytes::Bytes),  // ← 替代 Vec<u8>
    Done,
    Error(String),
}

// http_client.rs
Some(Ok(chunk)) => chunk_callback(StreamEvent::Chunk(chunk)),  // ← 无复制
```

下游代码中如果有需要 `Vec<u8>` 的场景，调用方自行 `chunk.to_vec()`。

## 关联

- `Bytes` crate 已是项目依赖（用于 SSE stream）
- napi 层 `helpers.rs:28` 也有类似转换（`stream_event_to_json` 中 `Chunk(data)` 用 base64 编码，可改为直接引用）
