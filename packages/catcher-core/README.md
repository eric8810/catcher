# catcher-core

[![crates.io](https://img.shields.io/crates/v/catcher-core.svg)](https://crates.io/crates/catcher-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Shared core types and errors for the [catcher](https://github.com/eric8810/catcher) resilient networking toolkit.

**Zero I/O dependencies** — pure data types only, used by all catcher crates.

> **⚠️ Breaking Change (0.3.0)**: `BackoffKind::default()` is now `Fixed` (was unspecified/`Exponential`). `RetryConfig::default().backoff` is now `Fixed`. All config structs now accept `camelCase` field names via `#[serde(alias)]`.

## Types

| Category | Types |
|----------|-------|
| **Error** | `CatcherError`, `ErrorCategory` |
| **Resilience** | `RetryConfig`, `CircuitBreakerConfig`, `BackoffKind`, `CbState` |
| **Observability** | `NetworkQualityLevel`, `NetworkQualityResult`, `RttSnapshot`, `ConnectionType`, `Priority` |
| **Scheduler** | `QueueConfig`, `ConcurrencyMode` |
| **SSE** | `SseClientConfig`, `SseMethod`, `SseReconnectConfig` |
| **FFI** | `FfiResult`, `FfiString`, `FfiBytes`, `EventCallback` |

## Usage

```toml
[dependencies]
catcher-core = "0.3.11"
```

```rust
use catcher_core::{CatcherError, ErrorCategory, RetryConfig, BackoffKind};

let retry = RetryConfig {
    max_attempts: 3,
    backoff: BackoffKind::Fixed,     // default is now Fixed
    min_backoff_ms: 100,
    max_backoff_ms: 10_000,
    jitter: true,
};
```

## License

MIT
