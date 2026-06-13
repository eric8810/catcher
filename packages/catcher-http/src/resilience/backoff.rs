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

#[cfg(test)]
mod tests {
    use super::*;
    use retry_policies::RetryPolicy;
    use std::time::SystemTime;

    fn test_config(backoff: BackoffKind) -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            backoff,
            min_backoff_ms: 100,
            max_backoff_ms: 10_000,
            jitter: false,
        }
    }

    /// Extract delay as millis (f64) from RetryDecision
    fn delay_ms(
        policy: &retry_policies::policies::ExponentialBackoff,
        start: SystemTime,
        n: u32,
    ) -> Option<f64> {
        match policy.should_retry(start, n) {
            retry_policies::RetryDecision::Retry { execute_after } => {
                Some(execute_after.duration_since(start).unwrap().as_secs_f64() * 1000.0)
            }
            retry_policies::RetryDecision::DoNotRetry => None,
        }
    }

    #[test]
    fn rr1_fixed_strategy_min_equals_max() {
        let config = test_config(BackoffKind::Fixed);
        let policy = build_retry_policy(&config);
        let start = SystemTime::now();

        // Fixed backoff: retry_bounds sets both min and max to min_backoff (100ms)
        // The library may add sub-millisecond overhead, so allow ±1ms tolerance
        let d0 = delay_ms(&policy, start, 0).unwrap();
        let d1 = delay_ms(&policy, start, 1).unwrap();
        let d2 = delay_ms(&policy, start, 2).unwrap();

        for (i, d) in [(0, d0), (1, d1), (2, d2)] {
            assert!(
                (d - 100.0).abs() < 2.0,
                "attempt {i}: expected ~100ms, got {d:.3}ms"
            );
        }
    }

    #[test]
    fn rr2_exponential_strategy_retry_bounds() {
        let config = test_config(BackoffKind::Exponential);
        let policy = build_retry_policy(&config);
        let start = SystemTime::now();

        // Exponential: starts at 100ms, doubles each attempt, capped at 10_000ms
        let d0 = delay_ms(&policy, start, 0).unwrap();
        let d1 = delay_ms(&policy, start, 1).unwrap();
        let d2 = delay_ms(&policy, start, 2).unwrap();

        assert!(
            (d0 - 100.0).abs() < 2.0,
            "attempt 0: expected ~100ms, got {d0:.3}ms"
        );
        assert!(
            (d1 - 200.0).abs() < 2.0,
            "attempt 1: expected ~200ms, got {d1:.3}ms"
        );
        assert!(
            (d2 - 400.0).abs() < 2.0,
            "attempt 2: expected ~400ms, got {d2:.3}ms"
        );
    }

    #[test]
    fn rr3_decorrelated_jitter_jitter_enabled() {
        let config = test_config(BackoffKind::DecorrelatedJitter);
        let policy = build_retry_policy(&config);
        let start = SystemTime::now();

        // DecorrelatedJitter uses Full jitter: delay ∈ [0, calculated_backoff].
        // The calculated backoff grows exponentially but jitter may push it below min_backoff.
        // Verify delays are within a reasonable range [0, max_backoff + overhead].
        for attempt in 0..3 {
            let d = delay_ms(&policy, start, attempt).expect("expected Retry decision");
            assert!(
                (0.0..=10_002.0).contains(&d),
                "attempt {attempt}: delay {d:.3}ms out of [0ms, 10000ms] range"
            );
        }
    }
}
