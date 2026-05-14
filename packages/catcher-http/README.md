# catcher-http

[![crates.io](https://img.shields.io/crates/v/catcher-http.svg)](https://crates.io/crates/catcher-http)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient HTTP client for the [catcher](https://github.com/eric8810/catcher) toolkit — built on **reqwest** with middleware for retry, circuit breaker, and priority scheduling.

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
catcher-http = { version = "0.2", default-features = true }
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
use catcher_core::types::sse::SseClientConfig;

let config = SseClientConfig {
    url: "https://api.example.com/events".into(),
    method: SseMethod::GET,
    headers: Default::default(),
    body: None,
    reconnect: Default::default(),
};

let mut stream = SseStream::connect(config).await?;
while let Some(line) = stream.next().await {
    let line = line?;
    if let Some(payload) = line.strip_prefix("data: ") {
        println!("{}", payload);
    }
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
| `CircuitBreaker`, `AdaptiveTimeout` | Resilience primitives |
| `PriorityRequestQueue` | Concurrency scheduler |
| `MetricsCollector`, `NetworkQualityEvaluator` | Observability |
| `SseClient`, `SseStream` | SSE streaming |

## License

MIT
