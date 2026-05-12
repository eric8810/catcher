//! catcher-ws — Resilient WebSocket client for catcher
//!
//! Features:
//! - Exponential backoff + jitter reconnection
//! - Adaptive heartbeat with RTT tracking
//! - Multi-endpoint racing (connect to fastest)
//! - Per-message deflate compression
//! - Built-in msgpack codec (pack / unpack)
//! - FFI C ABI for cross-language bindings

pub mod types;
pub mod transport;
pub mod ws;
pub mod codec;
pub mod ffi;

// Re-export key types
pub use transport::ws_client::{WsHandle, WsTransport};
pub use types::ws::{
    HeartbeatConfig, ReconnectConfig, WsClientConfig, WsEvent, WsState,
};
pub use ws::build_ws_config;
pub use ws::{EndpointRacer, HeartbeatManager, ReconnectManager};
pub use codec::{pack, unpack, unpack_value};
