# Performance: UniFFI `evaluate_quality` 每次调用创建新 `reqwest::Client`

**严重程度**: 🟡 Low — 复用 #010 同类问题

**状态**: Open

**位置**: `packages/catcher-uniffi/src/lib.rs:573-574`

---

## 当前代码

```rust
pub fn evaluate_quality(host: String) -> Result<String, CatcherError> {
    let handle = block_on_aux_thread(async move {
        let mut evaluator = NetworkQualityEvaluator::new(20);  // ← 每次创建新 Client
        match evaluator.measure_http_rtt(&host, "/").await {
            // ...
        }
    });
    // ...
}
```

而 `NetworkQualityEvaluator::new()` 内部：
```rust
pub fn new(window_size: usize) -> Self {
    Self {
        http_client: reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap(),  // ← 每次创建新 reqwest::Client
        // ...
    }
}
```

## 问题

每次调用 `evaluate_quality` 都创建一个全新的 `reqwest::Client`：
- 无连接池复用（每次重新 TCP+TLS 握手）
- 无 TLS session 缓存
- `Client::builder().build()` 初始化 TLS backend 有固定开销

与此问题同类的还有 `quality_ffi.rs` 中使用同一个 `EVALUATOR` 静态变量（正确复用），但 uniffi 层绕过了它。

## 修复

复用 `NetworkQualityEvaluator`——与 `quality_ffi.rs` 的 `catcher_evaluate_quality` 一致：

```rust
static EVALUATOR: std::sync::Mutex<Option<NetworkQualityEvaluator>> = std::sync::Mutex::new(None);

pub fn evaluate_quality(host: String) -> Result<String, CatcherError> {
    let handle = block_on_aux_thread(async move {
        let mut guard = EVALUATOR.lock().unwrap();
        let evaluator = guard.get_or_insert_with(|| NetworkQualityEvaluator::new(50));
        match evaluator.measure_http_rtt(&host, "/").await {
            // ...
        }
    });
    // ...
}
```

## 关联

- 同类问题：#010 SSE 每次重连创建新 Client
