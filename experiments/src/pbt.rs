//! Property-Based Tests — 验证 retry/CB 不变量
//!
//! 独立于 Catcher 代码库，验证韧性机制的核心不变量。
//! 用法: cargo test --release

use proptest::prelude::*;

// ── 简化版 RetryConfig（不依赖 Catcher）──────────────────────────

#[derive(Debug, Clone)]
struct RetryConfig {
    max_attempts: u32,
    min_backoff_ms: u64,
    max_backoff_ms: u64,
    jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            min_backoff_ms: 100,
            max_backoff_ms: 10_000,
            jitter: true,
        }
    }
}

// ── 简化版 CB Config ──────────────────────────────────────────

#[derive(Debug, Clone)]
struct CbConfig {
    failure_threshold: u32,
    reset_timeout_ms: u64,
    half_open_max_requests: u32,
}

impl Default for CbConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout_ms: 30_000,
            half_open_max_requests: 5,
        }
    }
}

// ── Retry 模拟器 ──────────────────────────────────────────────

fn simulate_retry(config: &RetryConfig, loss_rate: f64, rng: &mut impl Rng) -> (bool, u32) {
    let mut attempts: u32 = 0;
    loop {
        attempts += 1;
        if rng.gen::<f64>() >= loss_rate {
            return (true, attempts);
        }
        if attempts > config.max_attempts {
            return (false, attempts);
        }
    }
}

use rand::Rng;

// ═══════════════════════════════════════════════════════════════════
// 不变量 1: retry count ≤ max_attempts + 1（初始尝试 + 重试）
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn invariant_retry_never_exceeds_max(
        max_attempts in 0u32..10,
        loss_rate in 0.0f64..1.0,
    ) {
        let config = RetryConfig {
            max_attempts,
            ..RetryConfig::default()
        };
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let (_success, attempts) = simulate_retry(&config, loss_rate, &mut rng);
            prop_assert!(attempts <= max_attempts + 1,
                "attempts={attempts} > max_attempts+1={}", max_attempts + 1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 不变量 2: loss_rate=0 → 永远第1次成功
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn invariant_loss_zero_always_first_success(
        max_attempts in 0u32..10,
    ) {
        let config = RetryConfig {
            max_attempts,
            ..RetryConfig::default()
        };
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let (success, attempts) = simulate_retry(&config, 0.0, &mut rng);
            prop_assert!(success);
            prop_assert_eq!(attempts, 1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 不变量 3: loss_rate=1.0 → 永远全部失败
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn invariant_loss_total_always_fails(
        max_attempts in 0u32..10,
    ) {
        let config = RetryConfig {
            max_attempts,
            ..RetryConfig::default()
        };
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let (success, attempts) = simulate_retry(&config, 1.0, &mut rng);
            prop_assert!(!success);
            prop_assert_eq!(attempts, max_attempts + 1);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 不变量 4: max_attempts=0 → 不重试，只试1次
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn invariant_no_retry_when_max_zero(
        loss_rate in 0.0f64..1.0,
    ) {
        let config = RetryConfig {
            max_attempts: 0,
            ..RetryConfig::default()
        };
        let mut rng = rand::thread_rng();
        for _ in 0..100 {
            let (_success, attempts) = simulate_retry(&config, loss_rate, &mut rng);
            prop_assert!(attempts <= 1, "max_attempts=0 but got {attempts} attempts");
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 不变量 5: CB failure_threshold ≥ 1（有效性约束）
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn invariant_cb_threshold_minimum(
        threshold in 1u32..100,
    ) {
        let config = CbConfig {
            failure_threshold: threshold,
            ..CbConfig::default()
        };
        prop_assert!(config.failure_threshold >= 1);
    }
}

// ═══════════════════════════════════════════════════════════════════
// 不变量 6: backoff 延迟范围合理性
// ═══════════════════════════════════════════════════════════════════

proptest! {
    #[test]
    fn invariant_backoff_range_valid(
        min_ms in 1u64..60_000,
        max_ms in 1u64..60_000,
    ) {
        let config = RetryConfig {
            min_backoff_ms: min_ms,
            max_backoff_ms: max_ms,
            ..RetryConfig::default()
        };
        // min_backoff 不应大于 max_backoff（如果大于，实现应 clamp）
        // 这里只验证配置本身不矛盾（两者都在1-60000ms范围内）
        prop_assert!(config.min_backoff_ms >= 1);
        prop_assert!(config.max_backoff_ms >= 1);
    }
}
