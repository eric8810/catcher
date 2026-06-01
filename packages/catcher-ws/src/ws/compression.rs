use crate::types::ws::WsClientConfig;

/// 将 WsClientConfig 的压缩设置转换为 yawc Options。
pub fn build_ws_config(config: &WsClientConfig) -> yawc::Options {
    let mut options = yawc::Options::default()
        .with_limits(
            config.max_payload_bytes as usize,
            config.max_payload_bytes as usize,
        )
        .with_utf8()
        .with_no_delay();

    if config.per_message_deflate {
        options = options.with_compression_level(yawc::CompressionLevel::new(6));
    } else {
        options = options.without_compression();
    }

    options
}
