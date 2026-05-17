# Performance: FFI 回调 `serde_json::to_string(&resp)` 将 body 序列化为 JSON 数字数组

**严重程度**: 🔴 High — 1MB body → ~5MB JSON 字符串，内存和 CPU 膨胀

**状态**: Open

**位置**:

| 函数 | 文件 | 行号 |
|------|------|------|
| `catcher_http_get` | `packages/catcher-http/src/ffi/http_ffi.rs` | 109 |
| `catcher_http_post` | `packages/catcher-http/src/ffi/http_ffi.rs` | 144 |
| `catcher_http_execute` | `packages/catcher-http/src/ffi/http_ffi.rs` | 197 |
| `catcher_http_execute_with_id` | `packages/catcher-http/src/ffi/http_ffi.rs` | 256-258 |
| `catcher_http_multipart` | `packages/catcher-http/src/ffi/http_ffi.rs` | 511 |

---

## 当前代码

```rust
let result = t.execute(request).await;
let json = match result {
    Ok(resp) => serde_json::to_string(&resp).unwrap_or_default(),
    Err(e) => error_json(&e.to_string()),
};
invoke_http_callback(callback, "http_result", json, ud);
```

`HttpResponse` 包含 `body: Vec<u8>`，serde 将 `Vec<u8>` 序列化为 JSON 数字数组：

```json
{"status":200,"headers":{...},"body":[72,101,108,108,111],"elapsed_ms":5}
```

## 问题

| body 大小 | JSON 膨胀比 | 示例 |
|-----------|------------|------|
| 1 KB | ~5x | ~5 KB JSON |
| 100 KB | ~5x | ~500 KB JSON |
| 1 MB | ~5x | **~5 MB JSON** |

每字节变成 `"255,"`（4 个字符），加上数组方括号和逗号分隔。在 FFI 回调路径上，二进制响应体被展开为海量 JSON 数字，造成：

- 内存膨胀 5x
- `serde_json::to_string` 序列化 CPU 开销巨大
- CString 再分配一次（JSON 字符串 → CString）
- 回调传递巨大的 JSON 字符串到 FFI 调用方

## 修复

### 方案 A：base64 编码 body（推荐）

```rust
Ok(resp) => serde_json::json!({
    "status": resp.status,
    "headers": resp.headers,
    "body_base64": base64_encode(&resp.body),
    "elapsed_ms": resp.elapsed_ms,
}).to_string()
```

膨胀比从 5x 降到 ~1.3x（base64）。

### 方案 B：body 单独传递

在 `HttpResponse` 上用 `#[serde(skip)]` 跳过 body，body 通过 `FfiResult.data` 指针单独传递。

### 方案 C：不序列化 body

对于许多用例（如 HEAD 请求、状态检查），body 不需要传回 FFI 调用方。

## 关联

- 所有 `serde_json::to_string(&resp)` 调用点
- `catcher_http_execute_with_id` 路径额外包装了 `request_id` 字段
