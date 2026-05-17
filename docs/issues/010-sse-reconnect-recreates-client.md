# Performance: SSE 每次重连创建新 `reqwest::Client`，丢失连接池

**严重程度**: 🟡 Low — 每次重连重新建立 TCP+TLS 连接

**状态**: Open

**位置**: `packages/catcher-http/src/sse/client.rs:181-183`

---

## 当前代码

```rust
async fn connect_once(
    config: &SseClientConfig,
    lines_tx: &mpsc::UnboundedSender<...>,
    ...
) -> Result<(), CatcherError> {
    let client = Client::builder()
        .build()
        .map_err(|e| CatcherError::Internal(format!("reqwest build: {e}")))?;
    // ...
}
```

每次 `connect_once()` 调用（包括自动重连）都创建全新的 `reqwest::Client`。

## 问题

`reqwest::Client` 内部维护连接池、TLS 会话缓存、DNS 缓存。每次重建意味着：

- 每次重连重新 TCP 握手 + TLS 握手
- 无法复用 keep-alive 连接
- 无法复用 TLS session ticket
- `Client::builder().build()` 本身有初始化开销（构建 TLS backend、加载根证书等）

对于长时间运行的 SSE 连接，重连频率低（通常几分钟一次），影响有限。但对于频繁断连的场景（网络不稳定），这是一个可观测的开销。

## 修复

在 `SseClientConfig` 或 `SseClient::connect()` 层面创建一次 `reqwest::Client` 并共享：

```rust
// 方案 A：SseClient 在 connect() 时创建 client 并传入 connect_once
pub async fn connect(config: SseClientConfig) -> Result<Self, CatcherError> {
    let client = Client::builder()
        .build()
        .map_err(|e| CatcherError::Internal(format!("reqwest build: {e}")))?;
    let client = Arc::new(client);
    // 传入 connect_once...
}
```

`SseStream` 也有相同模式（`sse/stream.rs:30-32`），但它是 one-shot 无重连，影响较小。

## 关联

- 之前报告：`005-config-clone-per-request.md`（`HttpClientConfig` 未用 Arc）
- `SseStream` 同位置：`packages/catcher-http/src/sse/stream.rs:30-32`
