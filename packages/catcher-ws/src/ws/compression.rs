use crate::types::ws::WsClientConfig;

/// 将 WsClientConfig 的压缩设置转换为 yawc WebSocket Options。
pub fn build_ws_options(config: &WsClientConfig) -> yawc::Options {
    let max_payload = config.max_payload_bytes as usize;
    let mut options = yawc::Options::default()
        .with_max_payload_read(max_payload)
        .with_max_read_buffer(max_payload.saturating_mul(2))
        .with_utf8();

    if config.per_message_deflate {
        options = options.with_compression_level(yawc::CompressionLevel::new(6));
    } else {
        options = options.without_compression();
    }

    options
}
