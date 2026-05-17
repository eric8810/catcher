# Performance: `HttpClientConfig` 每次请求克隆

**严重程度**: 🟡 Low — 配置结构体较大，每请求一次 clone 增加分配压力

**状态**: Open

**位置**:

| 场景 | 文件 | 行号 |
|------|------|------|
| 优先级队列路径获取 config | `packages/catcher-http/src/transport/http_client.rs` | 232 |
| `execute_http_request` 接收 owned config | `packages/catcher-http/src/transport/http_client.rs` | 548-550 |

---

## 当前代码

```rust
// http_client.rs:229-242
let result = if let Some(ref queue) = self.priority_queue {
    let client = self.client.clone();
    let config = self.config.clone();  // ← 每请求 clone 整个 HttpClientConfig
    // ...
    let queue_result = tokio::select! {
        r = queue.submit(priority, timeout_ms, move || {
            let c = client.clone();
            let cfg = config.clone();  // ← 再次 clone
            let req = request;
            async move { execute_http_request(c, cfg, req).await }
        }) => r,
        // ...
    };
};
```

```rust
// 独立函数签名
async fn execute_http_request(
    client: ClientWithMiddleware,
    config: HttpClientConfig,   // ← owned，调用方已 clone
    request: HttpRequest,
) -> Result<HttpResponse, CatcherError> {
```

## 问题

`HttpClientConfig` 是一个大型结构体，包含多个 `HashMap<String, String>`、`Option<Box<...>>` 等堆分配字段。每次请求克隆一次配置：

- 优先级队列路径：clone 两次（一次提取、一次传入闭包）
- `execute_http_request` 需要 owned config，因为它在 tokio task 中运行需要 `'static`

## 修复

将 `HttpTransport` 中的 `config: HttpClientConfig` 改为 `config: Arc<HttpClientConfig>`：

```rust
pub struct HttpTransport {
    client: ClientWithMiddleware,
    config: Arc<HttpClientConfig>,  // ← Arc 替代裸结构体
    // ...
}

// 使用时
let config = self.config.clone();  // Arc clone，仅原子计数 +1
```

然后在 `execute_http_request` 中也用 `Arc<HttpClientConfig>` 替代。

## 关联

- 这是热路径（每 HTTP 请求触发）
- `HttpClientConfig` 的大小约为 20+ 字段
