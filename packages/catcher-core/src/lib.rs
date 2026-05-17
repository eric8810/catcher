//! catcher-core — Shared core types and errors for catcher
//!
//! Zero I/O dependencies. Pure data types only:
//! - `CatcherError` / `ErrorCategory` — unified error types
//! - `RetryConfig`, `CircuitBreakerConfig`, `BackoffKind` — resilience configuration
//! - `NetworkQualityLevel`, `RttSnapshot` — observability types
//! - `QueueConfig`, `ConcurrencyMode`, `Priority` — scheduler types
//! - `FfiResult`, `FfiString`, `FfiBytes`, `EventCallback` — FFI-safe types

pub mod error;
pub mod ffi_types;
pub mod handle_registry;
pub mod types;

// Re-export everything at crate root for convenience
pub use error::{CatcherError, ErrorCategory};
pub use ffi_types::{EventCallback, FfiBytes, FfiResult, FfiString};
pub use handle_registry::HandleRegistry;
pub use types::observability::{
    ConnectionType, NetworkQualityLevel, NetworkQualityResult, Priority, RttSnapshot,
};
pub use types::resilience::{BackoffKind, CbState, CircuitBreakerConfig, RetryConfig};
pub use types::scheduler::{ConcurrencyMode, QueueConfig};
pub use types::sse::{SseClientConfig, SseMethod, SseReconnectConfig};
