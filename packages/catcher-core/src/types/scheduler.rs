use serde::{Deserialize, Serialize};

/// 并发控制模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConcurrencyMode {
    /// 固定并发数
    Fixed(usize),
    /// 自适应：根据网络质量动态调整
    Adaptive,
}

impl Default for ConcurrencyMode {
    fn default() -> Self {
        ConcurrencyMode::Fixed(50)
    }
}

/// 调度队列配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    /// 最大并发数
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,

    /// 队列容量
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: usize,

    /// 默认任务超时（毫秒）
    #[serde(default = "default_timeout")]
    pub default_timeout_ms: u64,

    /// 并发模式
    #[serde(default)]
    pub concurrency_mode: ConcurrencyMode,
}

fn default_max_concurrency() -> usize {
    50
}
fn default_queue_capacity() -> usize {
    1024
}
fn default_timeout() -> u64 {
    30_000
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            max_concurrency: default_max_concurrency(),
            queue_capacity: default_queue_capacity(),
            default_timeout_ms: default_timeout(),
            concurrency_mode: ConcurrencyMode::Fixed(50),
        }
    }
}
