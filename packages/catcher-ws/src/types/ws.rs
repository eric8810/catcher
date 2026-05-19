use catcher_core::types::default_true;
pub use catcher_dns::DnsConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// WebSocket 事件 — 通过回调推送给上层
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    Connected { url: String, latency_ms: u64 },
    Disconnected { code: u16, reason: String },
    Reconnecting { attempt: u32, delay_ms: u64 },
    Message { data: Vec<u8>, is_binary: bool },
    Error { message: String },
    HeartbeatRtt { rtt_ms: u64 },
}

impl WsEvent {
    /// 序列化为 FFI 回调 JSON。Message 变体的 data 字段使用 base64 编码，
    /// 避免 Vec<u8> 被展开为 JSON 数字数组（~5x 膨胀）。
    pub fn to_ffi_json(&self) -> String {
        match self {
            WsEvent::Message { data, is_binary } => {
                use base64::Engine;
                let data_b64 = base64::engine::general_purpose::STANDARD.encode(data);
                serde_json::json!({
                    "type": "Message",
                    "data_base64": data_b64,
                    "is_binary": is_binary,
                })
                .to_string()
            }
            _ => serde_json::to_string(self).unwrap_or_default(),
        }
    }
}

/// WebSocket 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WsState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// 重连配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    /// 初始退避延迟（毫秒）
    #[serde(alias = "initialDelayMs", default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    /// 最大退避延迟（毫秒）
    #[serde(alias = "maxDelayMs", default = "default_max_delay")]
    pub max_delay_ms: u64,
    /// 退避乘数（指数增长）
    #[serde(alias = "backoffMultiplier", default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// 最大重试次数
    #[serde(alias = "maxAttempts", default = "default_max_reconnect_attempts")]
    pub max_attempts: u32,
}

fn default_initial_delay() -> u64 {
    500
}
fn default_max_delay() -> u64 {
    30_000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}
fn default_max_reconnect_attempts() -> u32 {
    20
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            backoff_multiplier: default_backoff_multiplier(),
            max_attempts: default_max_reconnect_attempts(),
        }
    }
}

/// 心跳配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// 心跳间隔（毫秒）
    #[serde(alias = "intervalMs", default = "default_heartbeat_interval")]
    pub interval_ms: u64,
    /// 是否根据 RTT 自适应调整
    #[serde(default = "default_true")]
    pub adaptive: bool,
    /// pong 超时（毫秒）
    #[serde(alias = "pongTimeoutMs", default = "default_pong_timeout")]
    pub pong_timeout_ms: u64,
    /// 连续丢失多少个 pong 后判定断线
    #[serde(alias = "maxMissedPongs", default = "default_max_missed_pongs")]
    pub max_missed_pongs: u32,
}

fn default_heartbeat_interval() -> u64 {
    30_000
}
fn default_pong_timeout() -> u64 {
    10_000
}
fn default_max_missed_pongs() -> u32 {
    3
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_ms: default_heartbeat_interval(),
            adaptive: default_true(),
            pong_timeout_ms: default_pong_timeout(),
            max_missed_pongs: default_max_missed_pongs(),
        }
    }
}

/// WebSocket 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsClientConfig {
    /// 端点 URL 列表（多端点竞速）
    #[serde(default)]
    pub urls: Vec<String>,

    /// WebSocket 子协议
    #[serde(default)]
    pub protocols: Vec<String>,

    /// 自定义 headers
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// 是否启用 perMessageDeflate 压缩
    #[serde(alias = "perMessageDeflate", default)]
    pub per_message_deflate: bool,

    /// 压缩阈值（字节，大于此值的消息才压缩）
    #[serde(alias = "deflateThresholdBytes", default = "default_deflate_threshold")]
    pub deflate_threshold_bytes: u32,

    /// 握手超时（毫秒）
    #[serde(alias = "handshakeTimeoutMs", default = "default_handshake_timeout")]
    pub handshake_timeout_ms: u64,

    /// 最大 payload 大小（字节）
    #[serde(alias = "maxPayloadBytes", default = "default_max_payload")]
    pub max_payload_bytes: u64,

    /// 重连配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect: Option<ReconnectConfig>,

    /// 心跳配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<HeartbeatConfig>,

    /// 同时竞速的端点数
    #[serde(alias = "raceCount", default = "default_race_count")]
    pub race_count: u32,

    /// 启用 msgpack 编解码 — send 自动 JSON→msgpack, receive 自动 msgpack→JSON
    #[serde(default)]
    pub msgpack: bool,

    /// DNS 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsConfig>,
}

fn default_deflate_threshold() -> u32 {
    1024
}
fn default_handshake_timeout() -> u64 {
    15_000
}
fn default_max_payload() -> u64 {
    64 * 1024 * 1024
}
fn default_race_count() -> u32 {
    1
}

impl Default for WsClientConfig {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            protocols: Vec::new(),
            headers: HashMap::new(),
            per_message_deflate: false,
            deflate_threshold_bytes: default_deflate_threshold(),
            handshake_timeout_ms: default_handshake_timeout(),
            max_payload_bytes: default_max_payload(),
            reconnect: None,
            heartbeat: None,
            race_count: default_race_count(),
            msgpack: false,
            dns: None,
        }
    }
}
