# 03 — 核心类型系统

> 对应源文件：`src/error.rs` / `src/config.rs` / `src/types/`

---

## src/error.rs — 统一错误类型

```rust
use thiserror::Error;

/// 所有 catcher-rs 操作返回的错误类型
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

/// 错误分类：区分可重试和不可重试
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Retryable,
    NonRetryable,
}

impl CatcherError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            CatcherError::ConnectionTimeout(_)
            | CatcherError::RequestTimeout(_)
            | CatcherError::TlsError(_)
            | CatcherError::DnsError { .. }
            | CatcherError::WsDisconnected { .. }
            | CatcherError::WsAllEndpointsFailed { .. }
            | CatcherError::CircuitBreakerOpen => ErrorCategory::Retryable,

            CatcherError::HttpError { status, .. } => {
                if *status >= 500 { ErrorCategory::Retryable }
                else              { ErrorCategory::NonRetryable }
            }

            CatcherError::RetryExhausted { .. }
            | CatcherError::QueueTimeout(_)
            | CatcherError::EncodeError(_)
            | CatcherError::DecodeError(_)
            | CatcherError::InvalidConfig(_)
            | CatcherError::Internal(_)
            | CatcherError::WsHandshakeTimeout(_) => ErrorCategory::NonRetryable,
        }
    }
}
```

---

## src/config.rs — 全局配置类型

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    pub reject_unauthorized: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self { reject_unauthorized: true, ca_cert_path: None, client_cert_path: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    pub cache_size: usize,
    pub positive_ttl_secs: u64,
    pub negative_ttl_secs: u64,
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self { cache_size: 512, positive_ttl_secs: 300, negative_ttl_secs: 60 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    pub keep_alive: bool,
    pub keep_alive_interval_secs: u64,
    pub max_idle_per_host: usize,
    pub idle_timeout_secs: u64,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self { keep_alive: true, keep_alive_interval_secs: 30, max_idle_per_host: 10, idle_timeout_secs: 90 }
    }
}
```

---

## src/types/http.rs

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::{TlsConfig, DnsConfig, ConnectionPoolConfig};
use crate::types::resilience::{RetryConfig, CircuitBreakerConfig};
use crate::types::observability::Priority;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientConfig {
    pub base_url: String,
    pub connect_timeout_ms: u64,
    pub response_timeout_ms: u64,
    pub default_headers: HashMap<String, String>,
    pub tls: TlsConfig,
    pub pool: ConnectionPoolConfig,
    pub dns: DnsConfig,
    pub retry: Option<RetryConfig>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub max_concurrency: u32,
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u64>,
    pub priority: Priority,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    GET, POST, PUT, DELETE, PATCH,
}

impl HttpMethod {
    pub fn is_idempotent(&self) -> bool {
        matches!(self, HttpMethod::GET | HttpMethod::HEAD | HttpMethod::PUT | HttpMethod::DELETE)
    }
}
```

---

## src/types/ws.rs

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::config::TlsConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsClientConfig {
    pub urls: Vec<String>,
    pub protocols: Vec<String>,
    pub headers: HashMap<String, String>,
    pub tls: TlsConfig,
    pub handshake_timeout_ms: u64,
    pub max_payload_bytes: u64,
    pub deflate: Option<DeflateConfig>,
    pub reconnect: Option<ReconnectConfig>,
    pub heartbeat: Option<HeartbeatConfig>,
    pub race_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WsState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting { attempt: u32, delay_ms: u64 },
}

#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected { endpoint: String, latency_ms: u64 },
    Disconnected { code: u16, reason: String },
    Reconnecting { attempt: u32, delay_ms: u64 },
    Message { data: Vec<u8>, is_binary: bool },
    Error { message: String },
    HeartbeatRtt { rtt_ms: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_attempts: u32,
    pub jitter: bool,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self { initial_delay_ms: 1000, max_delay_ms: 30_000, backoff_multiplier: 2.0, max_attempts: 20, jitter: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    pub interval_ms: u64,
    pub adaptive: bool,
    pub pong_timeout_ms: u64,
    pub max_missed_pongs: u32,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self { interval_ms: 10_000, adaptive: true, pong_timeout_ms: 5_000, max_missed_pongs: 3 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeflateConfig {
    pub compression_level: u8,
    pub mem_level: u8,
    pub threshold_bytes: u32,
}
```

---

## src/types/resilience.rs

```rust
use serde::{Deserialize, Serialize};
use crate::error::ErrorCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackoffKind {
    Fixed,
    Exponential,
    DecorrelatedJitter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff: BackoffKind,
    pub min_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub jitter: bool,
    pub retry_on_status: Vec<u16>,
    pub retry_on_category: Vec<ErrorCategory>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffKind::Exponential,
            min_backoff_ms: 500,
            max_backoff_ms: 30_000,
            jitter: true,
            retry_on_status: vec![],
            retry_on_category: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub reset_timeout_ms: u64,
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self { failure_threshold: 5, success_threshold: 2, reset_timeout_ms: 30_000, half_open_max_requests: 3 }
    }
}
```

---

## src/types/observability.rs

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkQualityLevel {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionType {
    Wifi,
    Cellular,
    Ethernet,
    Vpn,
    Other,
    None,
}

#[derive(Debug, Clone)]
pub struct RttSnapshot {
    pub avg_rtt_ms: u64,
    pub min_rtt_ms: u64,
    pub max_rtt_ms: u64,
    pub jitter_ms: u64,
    pub packet_loss_rate: f64,
    pub sample_count: u32,
}

#[derive(Debug, Clone)]
pub struct NetworkQualityResult {
    pub level: NetworkQualityLevel,
    pub rtt: RttSnapshot,
    pub connection_type: ConnectionType,
}

pub type Priority = u8;

impl Priority {
    pub const HIGHEST: Priority = 0;
    pub const HIGH: Priority = 3;
    pub const NORMAL: Priority = 5;
    pub const LOW: Priority = 7;
    pub const LOWEST: Priority = 10;
}
```

---

## src/types/scheduler.rs

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub max_concurrency: usize,
    pub timeout_ms: u64,
    pub concurrency_mode: ConcurrencyMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyMode {
    Fixed(u32),
    Dynamic,
}
```
