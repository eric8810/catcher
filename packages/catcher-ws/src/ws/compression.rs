use crate::types::ws::WsClientConfig;

/// 将 WsClientConfig 的压缩设置转换为 tungstenite WebSocketConfig
pub fn build_ws_config(
    config: &WsClientConfig,
) -> tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
    let cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(config.max_payload_bytes as usize),
        max_frame_size: Some(config.max_payload_bytes as usize),
        ..Default::default()
    };

    // tungstenite 0.24 WebSocketConfig 没有 compression_level 字段。
    // per_message_deflate 通过 feature flag 和 accept_unmasked_frames 控制。
    // 默认情况下 tungstenite 接受压缩扩展。
    // 如果用户禁用 deflate，无法在 tungstenite 0.24 中完全禁用。
    // 标记：后续升级到 tungstenite 0.25+ 可以获得更好的压缩控制。
    let _ = config.per_message_deflate;

    cfg
}
