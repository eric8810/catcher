# 02 — Phase 1: Foundation (types + error + codec)

> 对应 arch 文档：`03-types.md`, `10-error-handling.md`, `07-codec.md`
> 工期：4 人天
> 目标：Crate 骨架就绪，error / config / types / codec 四个纯数据模块编译通过且有完整单元测试

---

## 1. 实现步骤

### Step 1.1 — `src/error.rs`

**参考**：`arch-rs/03-types.md` (行 1-68), `arch-rs/10-error-handling.md`

```rust
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CatcherError {
    #[error("connection timeout after {0}ms")]
    ConnectionTimeout(u64),
    #[error("request timeout after {0}ms")]
    RequestTimeout(u64),
    #[error("TLS error: {0}")]
    TlsError(String),
    #[error("DNS resolution failed for {host}: {reason}")]
    DnsError { host: String, reason: String },
    #[error("HTTP error: status {status}, body: {body}")]
    HttpError { status: u16, body: String },
    #[error("WS handshake timeout after {0}ms")]
    WsHandshakeTimeout(u64),
    #[error("WS disconnected: code={code}, reason={reason}")]
    WsDisconnected { code: u16, reason: String },
    #[error("all WS endpoints failed ({count} attempted)")]
    WsAllEndpointsFailed { count: usize },
    #[error("retry exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted { attempts: u32, last_error: String },
    #[error("circuit breaker is OPEN, request rejected")]
    CircuitBreakerOpen,
    #[error("queue timeout after {0}ms")]
    QueueTimeout(u64),
    #[error("msgpack encode error: {0}")]
    EncodeError(String),
    #[error("msgpack decode error: {0}")]
    DecodeError(String),
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Retryable,
    NonRetryable,
}

impl CatcherError {
    pub fn category(&self) -> ErrorCategory { /* arch-rs/03-types 68-106 */ }
}
```

**关键设计**：
- `category()` 方法判定 Retryable / NonRetryable
- 5xx → Retryable，4xx → NonRetryable（401/403/404）
- 网络错误（ConnectionTimeout, DnsError, TlsError）→ Retryable
- 编解码错误（EncodeError, DecodeError）→ NonRetryable

**测试要求**（`tests/integration/error.rs`）：

| 测试用例 | 断言 |
|---------|------|
| `timeout_is_retryable` | `ConnectionTimeout(5000).category() == Retryable` |
| `http_500_is_retryable` | `HttpError { status: 503 }.category() == Retryable` |
| `http_401_is_non_retryable` | `HttpError { status: 401 }.category() == NonRetryable` |
| `http_403_is_non_retryable` | `HttpError { status: 403 }.category() == NonRetryable` |
| `encode_error_is_non_retryable` | `EncodeError("...".into()).category() == NonRetryable` |
| `circuit_breaker_open_is_retryable` | `CircuitBreakerOpen.category() == Retryable` |

### Step 1.2 — `src/config.rs`

**参考**：`arch-rs/03-types.md`

```rust
use serde::{Serialize, Deserialize};
use crate::types::http::HttpClientConfig;
use crate::types::ws::WsClientConfig;
use crate::types::scheduler::QueueConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatcherConfig {
    pub http: Option<HttpClientConfig>,
    pub ws: Option<WsClientConfig>,
    pub queue: Option<QueueConfig>,
    pub log_level: String,
}
```

### Step 1.3 — `src/types/` 模块

五个文件，按下面顺序创建：

**1. `src/types/http.rs`** — HTTP 请求/响应类型
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod { GET, POST, PUT, DELETE, PATCH }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub content_type: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientConfig {
    pub base_url: String,
    pub connect_timeout_ms: u64,
    pub response_timeout_ms: u64,
    pub keep_alive: bool,
    pub keep_alive_interval_secs: u64,
    pub max_idle_per_host: usize,
    pub idle_timeout_secs: u64,
    pub tls: TlsConfig,
    pub dns: Option<DnsConfig>,
    pub retry: Option<RetryConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub reject_unauthorized: bool,
    pub ca_cert_pem: Option<String>,
    pub client_cert_pem: Option<String>,
    pub client_key_pem: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub cache_ttl_secs: u32,
    pub nameservers: Vec<String>,
}
```

**2. `src/types/ws.rs`** — WebSocket 类型
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsClientConfig {
    pub urls: Vec<String>,
    pub protocols: Vec<String>,
    pub headers: HashMap<String, String>,
    pub per_message_deflate: bool,
    pub deflate_threshold_bytes: u32,
    pub handshake_timeout_ms: u64,
    pub max_payload_bytes: u64,
    pub reconnect: Option<ReconnectConfig>,
    pub heartbeat: Option<HeartbeatConfig>,
    pub race_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub interval_ms: u64,
    pub adaptive: bool,
    pub pong_timeout_ms: u64,
    pub max_missed_pongs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WsEvent {
    Connected { url: String, latency_ms: u64 },
    Disconnected { code: u16, reason: String },
    Reconnecting { attempt: u32, delay_ms: u64 },
    Message { data: Vec<u8>, is_binary: bool },
    Error { message: String },
    HeartbeatRtt { rtt_ms: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WsState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}
```

**3. `src/types/resilience.rs`** — 韧性类型
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff: BackoffKind,
    pub min_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub jitter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackoffKind {
    Fixed,
    Exponential,
    DecorrelatedJitter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub reset_timeout_ms: u64,
    pub half_open_max_requests: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbState {
    Closed,
    Open,
    HalfOpen,
}
```

**4. `src/types/scheduler.rs`** — 调度类型
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub max_concurrency: usize,
    pub queue_capacity: usize,
    pub default_timeout_ms: u64,
    pub concurrency_mode: ConcurrencyMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConcurrencyMode {
    Fixed(usize),
    Adaptive,
}
```

**5. `src/types/observability.rs`** — 可观测性类型
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority { Critical = 1, High = 2, Normal = 5, Low = 8, Background = 10 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkQualityLevel { Excellent, Good, Fair, Poor, Bad }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType { Wifi, Cellular, Ethernet, Vpn, Unknown }

#[derive(Debug, Clone, Default)]
pub struct RttSnapshot {
    pub avg_rtt_ms: u64,
    pub min_rtt_ms: u64,
    pub max_rtt_ms: u64,
    pub jitter_ms: u64,
    pub packet_loss_rate: f64,
    pub sample_count: usize,
}

#[derive(Debug, Clone)]
pub struct NetworkQualityResult {
    pub level: NetworkQualityLevel,
    pub avg_rtt_ms: u64,
    pub jitter_ms: u64,
    pub packet_loss_rate: f64,
    pub connection_type: ConnectionType,
}
```

### Step 1.4 — `src/codec/msgpack.rs`

**参考**：`arch-rs/07-codec.md`

实现 `pack()` / `unpack()` / `unpack_value()` 三个函数。

```rust
pub fn pack<T: Serialize>(value: &T) -> Result<Vec<u8>, CatcherError>
pub fn unpack<T: DeserializeOwned>(data: &[u8]) -> Result<T, CatcherError>
pub fn unpack_value(data: &[u8]) -> Result<serde_json::Value, CatcherError>
```

`unpack_value` 的 `rmpv::Value → serde_json::Value` 转换逻辑直接使用 `arch-rs/07-codec.md` 中的递归函数。

### Step 1.5 — `src/lib.rs` 初始状态

Phase 1 结束时 `lib.rs` 只导出四个模块：

```rust
pub mod error;
pub mod config;
pub mod types;
pub mod codec;

pub use error::{CatcherError, ErrorCategory};
pub use config::CatcherConfig;
```

---

## 2. 测试清单

### 2.1 Codec 单元测试（`tests/codec/msgpack_test.rs`）

| 测试 | 描述 |
|------|------|
| `pack_roundtrip_primitive` | `pack(42u32) → unpack::<u32>` 一致性 |
| `pack_roundtrip_string` | `pack("hello") → unpack::<String>` 一致性 |
| `pack_roundtrip_struct` | 自定义 struct 的 pack → unpack 一致性 |
| `pack_roundtrip_vec` | Vec 的 pack → unpack 一致性 |
| `pack_roundtrip_nested` | 嵌套对象的 pack → unpack 一致性 |
| `unpack_value_from_json` | JSON string → pack → unpack_value → 等价的 `serde_json::Value` |
| `unpack_empty_fails` | 空 slice 解码应返回 `DecodeError` |
| `pack_compare_size_vs_json` | 对典型 IM 消息体，msgpack < JSON 大小 (验证压缩率) |

### 2.2 Error 单元测试（`tests/integration/error.rs`）

已在 Step 1.1 列出。

### 2.3 类型默认值测试

| 测试 | 描述 |
|------|------|
| `http_config_defaults` | `HttpClientConfig` 默认值合理性 (timeout > 0, pool > 0) |
| `ws_config_defaults` | `WsClientConfig` 默认值合理性 |
| `serde_roundtrip_config` | `HttpClientConfig` JSON 序列化/反序列化一致性 |

---

## 3. Phase 1 完成标准

- [ ] `cargo check` 零错误
- [ ] `cargo test` 全部通过（≥12 个测试）
- [ ] `cargo clippy -- -D warnings` 零警告
- [ ] `cargo fmt --all -- --check` 通过
- [ ] Codec roundtrip 测试覆盖所有基本类型
- [ ] Error category 测试覆盖 6+ 种错误变体
