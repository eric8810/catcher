//! catcher-http — Resilient HTTP client for catcher
//!
//! Features:
//! - HTTP transport with reqwest + middleware
//! - Retry with exponential backoff + jitter
//! - Circuit breaker with CLOSED → OPEN → HALF_OPEN state machine
//! - Adaptive timeout based on P90 RTT
//! - Priority-based request queue with concurrency control
//! - Network quality evaluation
//! - FFI C ABI for cross-language bindings
//! - SSE streaming client with auto-reconnect

pub mod ffi;
pub mod observability;
pub mod resilience;
pub mod scheduler;
pub mod sse;
pub mod transport;
pub mod types;

// Re-export key types
pub use catcher_core::types::sse::{SseClientConfig, SseMethod, SseReconnectConfig};
pub use observability::{MetricsCollector, MetricsSnapshot, NetworkQualityEvaluator};
pub use resilience::{build_retry_policy, retry_with_backoff, AdaptiveTimeout, CircuitBreaker};
pub use scheduler::{concurrency_for_quality, PriorityRequestQueue};
pub use sse::{SseClient, SseStream};
pub use transport::HttpTransport;
