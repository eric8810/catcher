use serde::{Deserialize, Serialize};

/// 退避策略种类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BackoffKind {
    /// 固定延迟 (默认)
    #[default]
    Fixed,
    /// 指数退避 (delay * 2^attempt)
    Exponential,
    /// 去相关抖动退避 (decorrelated jitter)
    DecorrelatedJitter,
}

/// 重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// 退避策略
    #[serde(default)]
    pub backoff: BackoffKind,

    /// 最小退避延迟（毫秒）
    #[serde(default = "default_min_backoff")]
    pub min_backoff_ms: u64,

    /// 最大退避延迟（毫秒）
    #[serde(default = "default_max_backoff")]
    pub max_backoff_ms: u64,

    /// 是否添加抖动 (jitter)
    #[serde(default = "default_true")]
    pub jitter: bool,
}

fn default_max_attempts() -> u32 {
    3
}
fn default_min_backoff() -> u64 {
    100
}
fn default_max_backoff() -> u64 {
    10_000
}
fn default_true() -> bool {
    true
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: default_max_attempts(),
            backoff: BackoffKind::Exponential,
            min_backoff_ms: default_min_backoff(),
            max_backoff_ms: default_max_backoff(),
            jitter: default_true(),
        }
    }
}

/// 熔断器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerConfig {
    /// 连续失败多少次触发熔断
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// HALF_OPEN 状态下连续成功多少次恢复
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,

    /// 熔断后多久进入 HALF_OPEN（毫秒）
    #[serde(default = "default_reset_timeout")]
    pub reset_timeout_ms: u64,

    /// HALF_OPEN 期间允许的最大试探请求数
    #[serde(default = "default_half_open_max")]
    pub half_open_max_requests: u32,
}

fn default_failure_threshold() -> u32 {
    5
}
fn default_success_threshold() -> u32 {
    2
}
fn default_reset_timeout() -> u64 {
    30_000
}
fn default_half_open_max() -> u32 {
    5
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
            reset_timeout_ms: default_reset_timeout(),
            half_open_max_requests: default_half_open_max(),
        }
    }
}

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbState {
    /// 正常状态，请求通过
    Closed,
    /// 熔断状态，请求被拒绝
    Open,
    /// 半开状态，试探性放行少量请求
    HalfOpen,
}
