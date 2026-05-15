# catcher-http API Reference

> Rust HTTP 传输层 crate — `catcher-http` 0.2.x

```toml
[dependencies]
catcher-http = "0.2"
tokio = { version = "1", features = ["full"] }
```

---

## 模块结构

```
catcher-http
├── transport    → HttpTransport (reqwest + middleware)
├── resilience   → CircuitBreaker, AdaptiveTimeout, retry_with_backoff, build_retry_policy
├── scheduler    → PriorityRequestQueue, concurrency_for_quality
├── observability → MetricsCollector, MetricsSnapshot, NetworkQualityEvaluator
├── sse          → SseClient, SseStream
├── ffi          → C ABI (内部使用)
└── types        → 类型定义
```

## 公开 API

```rust
use catcher_http::{
    // 传输层
    HttpTransport,

    // 韧性组件
    CircuitBreaker,
    AdaptiveTimeout,
    retry_with_backoff,
    build_retry_policy,

    // 调度器
    PriorityRequestQueue,
    concurrency_for_quality,

    // 可观测性
    MetricsCollector,
    MetricsSnapshot,
    NetworkQualityEvaluator,

    // SSE
    SseClient,
    SseStream,
};

use catcher_core::{
    CatcherError,
    ErrorCategory,
    types::resilience::{
        RetryConfig, BackoffKind,
        CircuitBreakerConfig, CbState,
    },
    types::sse::{SseClientConfig, SseMethod, SseReconnectConfig},
};
```

---

## HttpTransport

```rust
pub struct HttpTransport { /* private fields */ }

impl HttpTransport {
    /// 根据 HttpClientConfig 构建
    pub fn new(config: HttpClientConfig) -> Result<Self, CatcherError>;

    /// 通用执行
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError>;

    /// 便捷方法
    pub async fn get(&self, url: &str, body: Option<&[u8]>, headers: Option<&HashMap<String, String>>) -> Result<HttpResponse, CatcherError>;
    pub async fn post(&self, url: &str, body: Option<&[u8]>, headers: Option<&HashMap<String, String>>) -> Result<HttpResponse, CatcherError>;
    pub async fn put(&self, url: &str, body: Option<&[u8]>, headers: Option<&HashMap<String, String>>) -> Result<HttpResponse, CatcherError>;
    pub async fn delete(&self, url: &str, body: Option<&[u8]>, headers: Option<&HashMap<String, String>>) -> Result<HttpResponse, CatcherError>;
    pub async fn patch(&self, url: &str, body: Option<&[u8]>, headers: Option<&HashMap<String, String>>) -> Result<HttpResponse, CatcherError>;

    /// 熔断器状态
    pub fn circuit_breaker_state(&self) -> Option<CbState>;
}
```

### HttpClientConfig

```rust
pub struct HttpClientConfig {
    pub base_url: String,
    pub connect_timeout_ms: u64,      // 默认 5000
    pub response_timeout_ms: u64,     // 默认 30000
    pub pool: PoolConfig,
    pub retry: Option<RetryConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub dns: Option<DnsConfig>,
    pub proxy: Option<ProxyConfig>,
    pub tls: TlsConfig,
    pub adaptive_timeout: Option<AdaptiveTimeoutConfig>,
}

pub struct PoolConfig {
    pub keep_alive: bool,                    // 默认 true
    pub max_idle_per_host: usize,            // 默认 10
    pub idle_timeout_secs: u64,              // 默认 60
    pub keep_alive_interval_secs: u64,       // 默认 30
}
```

### HttpRequest / HttpResponse

```rust
pub struct HttpRequest {
    pub method: HttpMethod,  // Get, Post, Put, Delete, Patch
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub content_type: Option<String>,
    pub timeout_ms: Option<u64>,
}

pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}
```

### 示例

```rust
use catcher_http::{HttpTransport, types::http::*};
use std::collections::HashMap;

let config = HttpClientConfig {
    base_url: "https://api.example.com".into(),
    connect_timeout_ms: 5000,
    response_timeout_ms: 30_000,
    pool: PoolConfig::default(),
    retry: Some(RetryConfig { max_attempts: 3, ..Default::default() }),
    ..Default::default()
};

let transport = HttpTransport::new(config)?;

// GET
let resp = transport.get("/users/1", None, None).await?;

// POST with body
let resp = transport.post("/messages", Some(b"hello"), None).await?;

// 可选 headers
let mut headers = HashMap::new();
headers.insert("Authorization".into(), "Bearer xxx".into());
let resp = transport.get("/protected", None, Some(&headers)).await?;
```

---

## CircuitBreaker

```rust
pub struct CircuitBreaker { /* private */ }

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self;

    /// 请求前检查 — Err(CircuitBreakerOpen) 表示应快速失败
    pub fn before_request(&self) -> Result<(), CatcherError>;

    /// 请求成功后调用
    pub fn on_success(&self);

    /// 请求失败后调用
    pub fn on_failure(&self);

    /// 当前状态
    pub fn state(&self) -> CbState;

    /// 手动重置到 CLOSED
    pub fn reset(&self);
}
```

### CircuitBreakerConfig

```rust
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,        // 连续失败 N 次 → OPEN
    pub success_threshold: u32,        // HALF_OPEN 连续成功 N 次 → CLOSED
    pub reset_timeout_ms: u64,         // OPEN → HALF_OPEN 等待时间
    pub half_open_max_requests: u32,   // HALF_OPEN 允许的最大并发探测请求
}
```

### CbState

```rust
pub enum CbState {
    Closed,
    Open,
    HalfOpen,
}
```

### 状态转换

```
CLOSED ──(failure_threshold 次连续失败)──▶ OPEN
OPEN   ──(after reset_timeout_ms)───────▶ HALF_OPEN
HALF_OPEN ──(success_threshold 次连续成功)─▶ CLOSED
HALF_OPEN ──(any failure)──────────────────▶ OPEN
```

### 示例

```rust
use catcher_http::CircuitBreaker;
use catcher_core::types::resilience::{CircuitBreakerConfig, CbState};

let cb = CircuitBreaker::new(CircuitBreakerConfig {
    failure_threshold: 3,
    success_threshold: 2,
    reset_timeout_ms: 30_000,
    half_open_max_requests: 5,
});

match cb.before_request() {
    Ok(()) => {
        // 执行请求...
        if success { cb.on_success(); } else { cb.on_failure(); }
    }
    Err(CatcherError::CircuitBreakerOpen) => {
        // 熔断中，使用降级逻辑
    }
    Err(e) => return Err(e),
}
```

---

## retry_with_backoff

```rust
pub async fn retry_with_backoff<T, F, Fut>(
    config: &RetryConfig,
    operation: F,
    retry_if: impl Fn(&CatcherError) -> bool,
    on_retry: impl FnMut(u32, &CatcherError),
) -> Result<T, CatcherError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CatcherError>>,
```

### RetryConfig

```rust
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff: BackoffKind,    // Fixed | Exponential | DecorrelatedJitter
    pub min_backoff_ms: u64,     // 默认 500
    pub max_backoff_ms: u64,     // 默认 30000
    pub jitter: bool,
}

pub enum BackoffKind { Fixed, Exponential, DecorrelatedJitter }
```

### 示例

```rust
let result = retry_with_backoff(
    &RetryConfig {
        max_attempts: 3,
        backoff: BackoffKind::DecorrelatedJitter,
        min_backoff_ms: 100,
        max_backoff_ms: 10_000,
        jitter: true,
    },
    || async { do_network_call().await },  // 返回 Result<T, CatcherError>
    |e| matches!(e, CatcherError::ConnectionTimeout(_)),
    |attempt, error| tracing::warn!("retry #{}, {}", attempt, error),
).await;
```

---

## AdaptiveTimeout

```rust
pub struct AdaptiveTimeout { /* private */ }

impl AdaptiveTimeout {
    pub fn new(min_timeout_ms: u64, max_timeout_ms: u64, multiplier: f64, window_size: usize) -> Self;
    pub fn record(&mut self, rtt_ms: u64);
    pub fn timeout_ms(&self) -> u64;
    pub fn compute(&self) -> Duration;
    pub fn snapshot(&self) -> RttSnapshot;
    pub fn from_snapshot(snapshot: &RttSnapshot, multiplier: f64) -> Duration;
}
```

算法：`timeout = clamp(P90_RTT × multiplier, min_timeout, max_timeout)`

```rust
let mut at = AdaptiveTimeout::new(500, 30_000, 3.0, 100);
at.record(120);
at.record(250);
let dur = at.compute();  // Duration
```

---

## PriorityRequestQueue

```rust
pub struct PriorityRequestQueue { /* private */ }

impl PriorityRequestQueue {
    pub fn new(concurrency: usize) -> Self;
    pub async fn enqueue<F, Fut>(&self, priority: u32, f: F) -> Result<Fut::Output, CatcherError>
    where F: FnOnce() -> Fut, Fut: Future;
    pub fn pending(&self) -> usize;
    pub fn set_concurrency(&self, concurrency: usize);
}
```

---

## concurrency_for_quality

```rust
pub fn concurrency_for_quality(quality: NetworkQuality) -> usize;
```

根据网络质量动态调整并发数。质量越差并发越低。

---

## MetricsCollector

```rust
pub struct MetricsCollector { /* private */ }

impl MetricsCollector {
    pub fn new() -> Self;
    pub fn record_request(&mut self, status: u16, latency_ms: u64);
    pub fn record_retry(&mut self);
    pub fn record_circuit_breaker_open(&mut self);
    pub fn snapshot(&self) -> MetricsSnapshot;
}

pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub success_rate: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: u64,
    pub p90_latency_ms: u64,
    pub p99_latency_ms: u64,
    pub retry_rate: f64,
    pub circuit_breaker_trips: u64,
}
```

---

## SseClient / SseStream

```rust
pub use catcher_http::sse::{SseClient, SseStream};

// SseClientConfig 来自 catcher_core
use catcher_core::types::sse::{SseClientConfig, SseMethod, SseReconnectConfig};
```

---

## 默认值速查

| 参数 | 默认值 |
|------|--------|
| `connect_timeout_ms` | `5000` |
| `response_timeout_ms` | `30000` |
| `pool.keep_alive` | `true` |
| `pool.max_idle_per_host` | `10` |
| `pool.idle_timeout_secs` | `60` |
| `retry.backoff` | `Exponential` |
| `retry.min_backoff_ms` | `500` |
| `retry.max_backoff_ms` | `30000` |
| `circuit_breaker.failure_threshold` | `5` |
| `circuit_breaker.success_threshold` | `2` |
| `circuit_breaker.reset_timeout_ms` | `30000` |
| `circuit_breaker.half_open_max_requests` | `5` |
