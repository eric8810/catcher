# Performance: SSE buffer 行提取导致 O(n²) 内存重分配

**严重程度**: 🟡 Low — 仅在长 SSE 流（数千行）时显现

**状态**: Open

**位置**:

| 文件 | 行号 |
|------|------|
| `packages/catcher-http/src/sse/client.rs` | 248-251 |
| `packages/catcher-http/src/sse/stream.rs` | 83-89 |

---

## 当前代码

```rust
// sse/client.rs — connect_once()
while let Some(newline_pos) = buffer.find('\n') {
    let line = buffer[..newline_pos]
        .trim_end_matches('\r')
        .to_string();
    buffer = buffer[newline_pos + 1..].to_string();  // ← 重新分配整个剩余 buffer
    // ...
}
```

```rust
// sse/stream.rs — process_buffer()
while let Some(newline_pos) = self.buffer.find('\n') {
    let line = self.buffer[..newline_pos]
        .trim_end_matches('\r')
        .to_string();
    self.buffer = self.buffer[newline_pos + 1..].to_string();  // ← 同上
    // ...
}
```

## 问题

每次提取一行后，`buffer[newline_pos + 1..].to_string()` 将整个剩余缓冲区重新分配并复制。对于包含 N 行的 SSE 流，总复制量为 O(N²)。

在典型使用场景（API 流式响应，通常几十到几百行）影响不明显，但在长时间运行的 SSE 连接（如实时推送，数千行）中会成为可观测的内存分配开销。

## 修复

### 方案 A：`String::drain`（推荐）

```rust
// ✅ 原地移除已处理部分，无额外分配
let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
buffer.drain(..newline_pos + 1);
```

### 方案 B：偏移追踪

```rust
// ✅ 维护偏移量，避免修改 String
let mut offset = 0;
while let Some(newline_pos) = buffer[offset..].find('\n') {
    let abs_pos = offset + newline_pos;
    let line = buffer[offset..abs_pos].trim_end_matches('\r').to_string();
    offset = abs_pos + 1;
    // ...
}
```

方案 A 更简洁，且 `drain` 在 `String` 上的实现是 O(remaining) 但只对剩余部分做一次 memmove（而非每次分配新 String），实际比 `to_string()` 更高效。

## 关联

- SSE 客户端 / 流是热路径（每行触发一次）
- 两个文件中的逻辑完全相同，可抽取公共 `parse_sse_lines` 函数
