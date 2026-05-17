# Performance: UniFFI `sse_stream` 将全部事件缓冲到内存才返回

**严重程度**: 🟡 Medium — 长 SSE 流（如 OpenAI streaming）全量驻留内存

**状态**: Open

**位置**: `packages/catcher-uniffi/src/lib.rs:316-335`

---

## 当前代码

```rust
pub fn sse_stream(...) -> Result<Vec<String>, CatcherError> {
    let handle = block_on_aux_thread(async move {
        let mut stream = SseStream::connect(config).await?;
        let mut events = Vec::new();                    // ← 全量收集
        while let Some(line_result) = stream.next().await {
            match line_result {
                Ok(line) => {
                    events.push(serde_json::json!({...}).to_string());  // ← 每行 push
                }
                Err(e) => {
                    events.push(serde_json::json!({...}).to_string());
                }
            }
        }
        Ok::<_, CatcherError>(events)  // ← 全部事件一次性返回
    });
    handle.join()...
}
```

## 问题

1. **全量缓冲**：所有 SSE 事件先收集到 `Vec<String>` 中，流结束后才返回给调用方。对于一个 1000 行的流式响应，调用方在流结束前收不到任何数据——这违背了 SSE 的"流式推送"语义。

2. **内存积累**：流式响应可能很大（如 OpenAI 的长文本生成）。全部内容驻留在内存中，流越长内存压力越大。

3. **每行 JSON 包装**：`serde_json::json!({"type":"data","data":line}).to_string()` 为每行数据额外分配 JSON 包装字符串。对于 1000 行，这是 1000 次 JSON 序列化 + 1000 次 String 分配。

## 修复

改为流式回调模式（与 `SseClientHandle` 模式一致）：

```rust
// 方案：接受 observer 而非返回 Vec
pub fn sse_stream(
    config_json: String,
    observer: Box<dyn SseEventObserver>,  // ← 回调接口
) -> Result<(), CatcherError> {
    // 每收到一行立即回调，无需缓冲
}
```

或使用 UniFFI 的 `AsyncSequence` / `RustStream`（如果版本支持）。

## 关联

- 同类模式：`SseClientHandle::connect()`（line 501-538）使用回调模式，是正确的做法
- `sse_stream` 的 one-shot 语义决定了不需要全量缓冲
