pub mod application_compression;
pub mod compression;
pub mod heartbeat;
pub mod multi_endpoint;
pub mod reconnect;

pub use application_compression::{
    decode_application_compression_frame, encode_application_compression_frame,
    ApplicationCompressionFrame, APPLICATION_COMPRESSION_MAGIC,
};
pub use compression::build_ws_options;
pub use heartbeat::HeartbeatManager;
pub use multi_endpoint::EndpointRacer;
pub use reconnect::ReconnectManager;
