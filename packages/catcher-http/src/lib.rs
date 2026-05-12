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

pub mod types;
pub mod transport;
pub mod resilience;
pub mod scheduler;
pub mod observability;
pub mod ffi;

// Re-export key types
pub use transport::HttpTransport;
pub use resilience::{
    build_retry_policy, retry_with_backoff, AdaptiveTimeout, CircuitBreaker,
};
pub use scheduler::{concurrency_for_quality, PriorityRequestQueue};
pub use observability::{MetricsCollector, MetricsSnapshot, NetworkQualityEvaluator};
