# catcher-http

[![crates.io](https://img.shields.io/crates/v/catcher-http.svg)](https://crates.io/crates/catcher-http)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient HTTP client for the [catcher](https://github.com/eric8810/catcher) toolkit — built on **reqwest** with middleware for retry, circuit breaker, and priority scheduling.

> **⚠️ Breaking Change (0.3.0)**: `RetryConfig::default().backoff` is now `Fixed` (was `Exponential`). `BackoffKind::default()` is `Fixed`. All config structs accept `camelCase` via `#[serde(alias)]`. `default_true()` is now a public export from `catcher-core`.

## Features

- **HTTP transport** — reqwest + reqwest-middleware with HTTP/2, gzip, brotli, deflate
- **Retry** — exponential backoff with jitter via `backon`
- **Circuit breaker** — CLOSED → OPEN → HALF_OPEN state machine
- **Adaptive timeout** — based on P90 RTT measurements
- **Priority queue** — concurrency-aware request scheduling
- **Network quality** — evaluator for connection health
- **SSE streaming** — Server-Sent Events with auto-reconnect and `Last-Event-ID`
- **Metrics** — request latency, retry count, circuit breaker state
- **FFI C ABI** — exported symbols for cross-language bindings
- **DNS caching** — via optional `hickory-dns` feature

## Usage

```toml
[dependencies]
catcher-http = { version = "0.3.9", default-features = true }
```

### Basic HTTP request

```rust
use catcher_http::{HttpTransport, CircuitBreaker};
use catcher_http::types::http::{HttpClientConfig, HttpMethod, HttpRequest};

let config = HttpClientConfig::default();
let transport = HttpTransport::new(config)?;

let response = transport.execute(HttpRequest {
    method: HttpMethod::GET,
    url: "https://httpbin.org/get".into(),
    headers: Default::default(),
    body: None,
    content_type: None,
    timeout_ms: None,
}).await?;

println!("Status: {}, Body: {} bytes", response.status, response.body.len());
```

### SSE streaming

```rust
use catcher_http::sse::{SseClient, SseStream};
use catcher_core::types::sse::{SseClientConfig, SseMethod, SseReconnectConfig};
use tokio_stream::StreamExt;

// One-shot stream
let config = SseClientConfig {
    url: "https://api.example.com/events".into(),
    method: SseMethod::GET,
    headers: Default::default(),
    body: None,
    reconnect: None,
    timeout_ms: 30_000,
    circuit_breaker: None,
};

let mut stream = SseStream::connect(config).await?;
while let Some(result) = stream.next().await {
    let line = result?;
    if let Some(payload) = line.strip_prefix("data: ") {
        println!("{}", payload);
    }
}

// Auto-reconnect client
let config = SseClientConfig {
    url: "https://api.example.com/events".into(),
    reconnect: Some(SseReconnectConfig {
        max_retries: 10,
        initial_delay_ms: 1000,
        max_delay_ms: 30_000,
        backoff_multiplier: 2.0,
    }),
    ..Default::default()
};
let mut client = SseClient::connect(config).await?;
while let Some(result) = client.next_line().await {
    let line = result?;
    println!("{}", line);
}
```

### Circuit breaker state

```rust
let state = transport.circuit_breaker_state();
// CbState::Closed | CbState::Open | CbState::HalfOpen
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `rustls-tls` | ✅ | TLS via rustls |
| `native-tls` | ❌ | TLS via native-tls |
| `hickory-dns` | ✅ | DNS caching with hickory |
| `napi` | ❌ | napi-rs bindings (internal) |

## Re-exports

| Type | Source |
|------|--------|
| `HttpTransport` | Main async HTTP client |
| `types::http::DnsConfig` | Shared DNS config from `catcher-dns` |
| `CircuitBreaker`, `AdaptiveTimeout` | Resilience primitives |
| `PriorityRequestQueue` | Concurrency scheduler |
| `MetricsCollector`, `NetworkQualityEvaluator` | Observability |
| `SseClient`, `SseStream` | SSE streaming |

## License

MIT
