use crate::types::ws::WsClientConfig;

/// 将 WsClientConfig 的压缩设置转换为 tungstenite WebSocketConfig
pub fn build_ws_config(
    config: &WsClientConfig,
) -> tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
    let cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(config.max_payload_bytes as usize))
        .max_frame_size(Some(config.max_payload_bytes as usize));

    // tungstenite 0.29 仍未支持 permessage-deflate (RFC 7692)。
    // 该字段保留为跨平台 API 兼容；Rust 侧压缩仍需等待上游或 fork。
    let _ = config.per_message_deflate;

    cfg
}
