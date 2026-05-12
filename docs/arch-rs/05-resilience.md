# 05 — 韧性层

> 对应源文件：`src/resilience/`

---

## RetryScheduler (`src/resilience/retry.rs`)

> **适用范围**：此函数用于**非 HTTP 场景**（WebSocket 连接重试、DNS 解析重试等）。
> HTTP 路径的重试由 `reqwest-retry` 中间件在 Transport 层处理（见 04-transport）。
> 两者互补，不可混用。

```rust
use backon::{Retryable, ExponentialBuilder, ConstantBuilder};
use std::time::Duration;
use crate::error::{CatcherError, ErrorCategory};
use crate::types::resilience::*;

/// 对异步操作执行重试，带指数退避 + jitter
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
    let mut attempt = 0u32;

    let backoff = match config.backoff {
        BackoffKind::Fixed => {
            let dur = Duration::from_millis(config.min_backoff_ms);
            // Use ConstantBuilder with fixed delay
            let mut builder = ConstantBuilder::default();
            builder.with_delay(dur);
            // Wrap in same interface: we use ExponentialBuilder with factor=1 for fixed
            ExponentialBuilder::default()
                .with_min_delay(dur)
                .with_max_delay(dur)
                .with_factor(1.0)
        }
        BackoffKind::Exponential => {
            ExponentialBuilder::default()
                .with_min_delay(Duration::from_millis(config.min_backoff_ms))
                .with_max_delay(Duration::from_millis(config.max_backoff_ms))
                .with_factor(2.0)
        }
        BackoffKind::DecorrelatedJitter => {
            ExponentialBuilder::default()
                .with_min_delay(Duration::from_millis(config.min_backoff_ms))
                .with_max_delay(Duration::from_millis(config.max_backoff_ms))
                .with_jitter()
        }
    };

    let action = || {
        attempt += 1;
        operation()
    };

    let result = action
        .retry(backoff)
        .when(|e: &CatcherError| {
            let should = e.category() == ErrorCategory::Retryable
                && retry_if(e)
                && attempt <= config.max_attempts;
            if should {
                on_retry(attempt, e);
            }
            should
        })
        .sleep(tokio::time::sleep)
        .await;

    result.map_err(|e| CatcherError::RetryExhausted {
        attempts: config.max_attempts,
        last_error: format!("{e}"),
    })
}
```

---

## CircuitBreaker (`src/resilience/circuit_breaker.rs`)

```rust
use circuitbreaker_rs::{CircuitBreaker as Cb, CircuitBreakerBuilder};
use std::time::Duration;
use crate::error::CatcherError;
use crate::types::resilience::CircuitBreakerConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbState {
    Closed,
    Open,
    HalfOpen,
}

/// 熔断器包装：circuitbreaker-rs + 状态查询
pub struct CircuitBreaker {
    inner: Cb,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        let inner = CircuitBreakerBuilder::new()
            .failure_threshold(config.failure_threshold as u64)
            .success_threshold(config.success_threshold as u64)
            .half_open_timeout(Duration::from_millis(config.reset_timeout_ms))
            .build();
        Self { inner, config }
    }

    /// 受熔断器保护的异步操作
    pub async fn call<T, F, Fut>(&self, operation: F) -> Result<T, CatcherError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, CatcherError>>,
    {
        self.inner.call(|| operation()).await.map_err(|e| {
            match e.downcast::<CatcherError>() {
                Ok(ce) => *ce,
                Err(other) => {
                    // If CB itself rejects (OPEN state), return CircuitBreakerOpen
                    CatcherError::CircuitBreakerOpen
                }
            }
        })
    }
}
```

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
