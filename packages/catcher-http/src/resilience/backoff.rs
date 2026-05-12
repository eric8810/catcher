use catcher_core::types::resilience::{BackoffKind, RetryConfig};
use std::time::Duration;

/// 将 RetryConfig 转换为 retry-policies 的 ExponentialBackoff
///
/// 供 HttpTransport 中 reqwest-retry 中间件使用。
pub fn build_retry_policy(config: &RetryConfig) -> retry_policies::policies::ExponentialBackoff {
    let min_backoff = Duration::from_millis(config.min_backoff_ms);
    let max_backoff = Duration::from_millis(config.max_backoff_ms);

    match config.backoff {
        BackoffKind::Fixed => retry_policies::policies::ExponentialBackoff::builder()
            .retry_bounds(min_backoff, min_backoff)
            .jitter(retry_policies::Jitter::None)
            .build_with_max_retries(config.max_attempts),
        BackoffKind::Exponential => retry_policies::policies::ExponentialBackoff::builder()
            .retry_bounds(min_backoff, max_backoff)
            .jitter(retry_policies::Jitter::None)
            .build_with_max_retries(config.max_attempts),
        BackoffKind::DecorrelatedJitter => retry_policies::policies::ExponentialBackoff::builder()
            .retry_bounds(min_backoff, max_backoff)
            .jitter(retry_policies::Jitter::Full)
            .build_with_max_retries(config.max_attempts),
    }
}
