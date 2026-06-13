//! catcher-ws — Resilient WebSocket client for catcher
//!
//! Features:
//! - Exponential backoff + jitter reconnection
//! - Adaptive heartbeat with RTT tracking
//! - Multi-endpoint racing (connect to fastest)
//! - Per-message deflate compression
//! - Built-in msgpack codec (pack / unpack)
//! - FFI C ABI for cross-language bindings

pub mod codec;
pub mod ffi;
pub mod transport;
pub mod types;
pub mod ws;

// Re-export key types
pub use codec::{pack, unpack, unpack_value};
pub use transport::ws_client::{WsHandle, WsTransport};
pub use types::ws::{
    ApplicationCompressionAlgorithm, ApplicationCompressionConfig, DnsConfig, HeartbeatConfig,
    ProxyConfig, ReconnectConfig, TlsConfig, TlsVersion, WsClientConfig, WsEvent, WsState,
};
pub use ws::{
    build_ws_config, decode_application_compression_frame, encode_application_compression_frame,
    ApplicationCompressionFrame, APPLICATION_COMPRESSION_MAGIC,
};
pub use ws::{EndpointRacer, HeartbeatManager, ReconnectManager};
