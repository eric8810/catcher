# 05 — 韧性层

> 对应源文件：`catcher-http/src/resilience/retry.rs`、`circuit_breaker.rs`
> 更新于 2026-05 — v3 调研闭环后同步实际实现

---

## RetryScheduler (`catcher-http/src/resilience/retry.rs`)

> 依赖 `backon` crate 提供退避策略（ConstantBuilder / ExponentialBuilder）。
> 按 `BackoffKind` 分三路分支：Fixed / Exponential / DecorrelatedJitter。

```rust
use catcher_core::{CatcherError, ErrorCategory};
use catcher_core::types::resilience::{BackoffKind, RetryConfig};
use backon::{ConstantBuilder, ExponentialBuilder, Retryable};
use std::cell::Cell;
use std::time::Duration;

pub async fn retry_with_backoff<T, F, Fut>(
    config: &RetryConfig,
    mut operation: F,
    retry_if: impl Fn(&CatcherError) -> bool,
    mut on_retry: impl FnMut(u32, &CatcherError),
) -> Result<T, CatcherError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, CatcherError>>,
{
    let max_attempts = config.max_attempts;
    let min_delay = Duration::from_millis(config.min_backoff_ms);
    let max_delay = Duration::from_millis(config.max_backoff_ms);
    let attempt = Cell::new(0u32);

    let action = || {
        let a = attempt.get() + 1;
        attempt.set(a);
        operation()
    };

    let result = match config.backoff {
        BackoffKind::Fixed => {
            let backoff = ConstantBuilder::default().with_delay(min_delay);
            action.retry(backoff)
                .when(|e: &CatcherError| {
                    let a = attempt.get();
                    let should = e.category() == ErrorCategory::Retryable
                        && retry_if(e) && a < max_attempts;
                    if should { on_retry(a, e); }
                    should
                })
                .sleep(tokio::time::sleep).await
        }
        BackoffKind::Exponential => {
            let backoff = ExponentialBuilder::default()
                .with_min_delay(min_delay)
                .with_max_delay(max_delay)
                .with_factor(2.0);
            action.retry(backoff)
                .when(|e: &CatcherError| {
                    let a = attempt.get();
                    let should = e.category() == ErrorCategory::Retryable
                        && retry_if(e) && a < max_attempts;
                    if should { on_retry(a, e); }
                    should
                })
                .sleep(tokio::time::sleep).await
        }
        BackoffKind::DecorrelatedJitter => {
            let backoff = ExponentialBuilder::default()
                .with_min_delay(min_delay)
                .with_max_delay(max_delay)
                .with_factor(2.0)
                .with_jitter();
            action.retry(backoff)
                .when(|e: &CatcherError| {
                    let a = attempt.get();
                    let should = e.category() == ErrorCategory::Retryable
                        && retry_if(e) && a < max_attempts;
                    if should { on_retry(a, e); }
                    should
                })
                .sleep(tokio::time::sleep).await
        }
    };

    result.map_err(|e| CatcherError::RetryExhausted {
        attempts: config.max_attempts,
        last_error: format!("{e}"),
    })
}
```

### 设计要点

| 要点 | 说明 |
|------|------|
| **Cell<u32> 计数** | 使用 `std::cell::Cell` 而非局部变量 mut borrow，使 `action` 闭包可多次调用 |
| **attempt < max_attempts** | max_attempts=3 意味着最多 3 次尝试（1 次原始 + 2 次重试） |
| **category() 预过滤** | 只在 `Retryable` 时才检查 `retry_if`，NonRetryable 错误不会进入重试判断 |
| **DecorrelatedJitter** | 默认策略。`ExponentialBuilder.with_jitter()` 提供 decorrelated jitter |

---

## CircuitBreaker (`catcher-http/src/resilience/circuit_breaker.rs`)

> 自研实现，不依赖外部 CB crate。手写状态机 + `parking_lot::Mutex` + Atomic 计数器。

```rust
use catcher_core::CatcherError;
use catcher_core::types::resilience::{CbState, CircuitBreakerConfig};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 熔断器状态机
///
/// 状态迁移：
/// CLOSED    ──(consecutive_failures >= threshold)──▶ OPEN
/// OPEN      ──(after reset_timeout)────────────────▶ HALF_OPEN
/// HALF_OPEN ──(consecutive_success >= threshold)────▶ CLOSED
/// HALF_OPEN ──(any failure)─────────────────────────▶ OPEN
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Mutex<CbState>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    opened_at_ms: AtomicU64,
    half_open_requests: AtomicU32,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: Mutex::new(CbState::Closed),
            failure_count: AtomicU32::new(0),
            success_count: AtomicU32::new(0),
            opened_at_ms: AtomicU64::new(0),
            half_open_requests: AtomicU32::new(0),
        }
    }

    /// 请求前检查：返回 Err(CircuitBreakerOpen) 如果熔断
    pub fn before_request(&self) -> Result<(), CatcherError> { ... }
    /// 请求成功后调用
    pub fn on_success(&self) { ... }
    /// 请求失败后调用
    pub fn on_failure(&self) { ... }
    /// 当前状态（用于 metrics）
    pub fn state(&self) -> CbState { ... }
    /// 强制重置到 CLOSED 状态
    pub fn reset(&self) { ... }
}
```

### 使用模式（与 `retry_with_backoff` 配合）

```rust
// 1. 请求前检查
cb.before_request()?;

// 2. 执行请求
let result = retry_with_backoff(&retry_config, || do_request(), ...).await;

// 3. 上报结果（请求级别的成败，不是单次尝试）
match &result {
    Ok(_) => cb.on_success(),
    Err(_) => cb.on_failure(),
}
```

### 并发安全

- `state` 使用 `parking_lot::Mutex`（同步锁），临界区极短（~几十 ns）
- `AtomicU32`/`AtomicU64` 统计计数器用 `Relaxed` 排序（纯统计，无 synchronizes-with 关系）

---

## AdaptiveTimeout (`src/resilience/timeout.rs`)

```rust
use std::time::Duration;
use crate::types::observability::RttSnapshot;

/// 基于 P90 RTT 的自适应超时计算器
///
/// timeout = max(min_timeout, P90_RTT * multiplier)，不超 max_timeout
pub struct AdaptiveTimeout {
    rtt_window: Vec<u64>,
    window_size: usize,
    min_timeout_ms: u64,
    max_timeout_ms: u64,
    multiplier: f64,
}

impl AdaptiveTimeout {
    pub fn new(
        min_timeout_ms: u64,
        max_timeout_ms: u64,
        multiplier: f64,
        window_size: usize,
    ) -> Self {
        Self {
            rtt_window: Vec::with_capacity(window_size),
            window_size,
            min_timeout_ms,
            max_timeout_ms,
            multiplier,
        }
    }

    /// 记录新 RTT 样本
    pub fn record_rtt(&mut self, rtt_ms: u64) {
        if self.rtt_window.len() >= self.window_size {
            self.rtt_window.remove(0);
        }
        self.rtt_window.push(rtt_ms);
    }

    /// 计算当前应使用的超时
    pub fn compute(&self) -> Duration {
        if self.rtt_window.is_empty() {
            return Duration::from_millis(self.min_timeout_ms);
        }

        let p90_idx = (self.rtt_window.len() as f64 * 0.9).ceil() as usize - 1;
        let mut sorted = self.rtt_window.clone();
        sorted.sort_unstable();
        let p90 = sorted[p90_idx.min(sorted.len() - 1)];

        let timeout_ms = ((p90 as f64) * self.multiplier) as u64;
        let clamped = timeout_ms.clamp(self.min_timeout_ms, self.max_timeout_ms);
        Duration::from_millis(clamped)
    }

    /// 从快照静态计算
    pub fn from_snapshot(snapshot: &RttSnapshot, multiplier: f64) -> Duration {
        let ms = ((snapshot.avg_rtt_ms as f64) * multiplier) as u64;
        Duration::from_millis(ms.max(5000))
    }
}
```
