use serde::{Deserialize, Serialize};

/// 请求优先级：数字越小优先级越高
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Priority {
    Critical = 1,
    High = 2,
    Normal = 5,
    Low = 8,
    Background = 10,
}

/// 网络质量等级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkQualityLevel {
    Excellent,
    Good,
    Fair,
    Poor,
    Bad,
}

/// 连接类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType {
    Wifi,
    Cellular,
    Ethernet,
    Vpn,
    Unknown,
}

/// RTT 滑动窗口统计快照
#[derive(Debug, Clone, Default, Serialize)]
pub struct RttSnapshot {
    pub avg_rtt_ms: u64,
    pub min_rtt_ms: u64,
    pub max_rtt_ms: u64,
    pub jitter_ms: u64,
    pub packet_loss_rate: f64,
    pub sample_count: usize,
}

/// 网络质量综合评估结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkQualityResult {
    pub level: NetworkQualityLevel,
    pub avg_rtt_ms: u64,
    pub jitter_ms: u64,
    pub packet_loss_rate: f64,
    pub connection_type: ConnectionType,
}
