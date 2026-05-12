# 04 — Phase 3: Resilience Layer

> 对应 arch 文档：`05-resilience.md`, `12-state-machines.md` (熔断器状态机)
> 工期：6 人天
> 目标：重试、熔断器、退避策略、自适应超时四块全部就绪，覆盖完整状态迁移

---

## 1. 模块概览

```
src/resilience/
├── mod.rs              # re-export
├── retry.rs            # retry_with_backoff: 非 HTTP 场景重试
├── circuit_breaker.rs  # CircuitBreaker: circuitbreaker-rs 封装
├── backoff.rs          # build_retry_policy: 统一退避构建器
└── timeout.rs          # AdaptiveTimeout: P90 RTT 自适应超时
```

**重要分工**：

| 场景 | 负责模块 | 库 |
|------|---------|-----|
| HTTP 请求重试 | `transport/http_client.rs` 中的 `reqwest-retry` 中间件 | `reqwest-retry` + `retry-policies` |
| WS 连接重试 | `resilience/retry.rs` → `retry_with_backoff()` | `backon` |
| DNS 解析重试 | `resilience/retry.rs` → `retry_with_backoff()` | `backon` |

---

## 2. 实现步骤

### Step 3.1 — `src/resilience/backoff.rs`

构建 `retry-policies` 的退避策略，供 `HttpTransport` 中 `reqwest-retry` 中间件使用：

```rust
use retry_policies::policies::{ExponentialBackoff, Jitter};
use retry_policies::Jitter as JitterTrait;
use crate::types::resilience::{RetryConfig, BackoffKind};
use std::time::Duration;

/// 将 RetryConfig 转换为 retry-policies 的 ExponentialBackoff
pub fn build_retry_policy(config: &RetryConfig) -> ExponentialBackoff {
    let min_backoff = Duration::from_millis(config.min_backoff_ms);
    let max_backoff = Duration::from_millis(config.max_backoff_ms);

    match config.backoff {
        BackoffKind::Fixed => {
            ExponentialBackoff::builder()
                .backoff_exponent(1.0) // no growth
                .build_with_min_and_max(min_backoff, min_backoff)
        }
        BackoffKind::Exponential => {
            ExponentialBackoff::builder()
                .backoff_exponent(2.0)
                .build_with_min_and_max(min_backoff, max_backoff)
        }
        BackoffKind::DecorrelatedJitter => {
            ExponentialBackoff::builder()
                .backoff_exponent(2.0)
                .jitter(Jitter::Full)
                .build_with_min_and_max(min_backoff, max_backoff)
        }
    }
}
```

### Step 3.2 — 升级 `HttpTransport` 使用 `reqwest-middleware`

**修改 `src/transport/http_client.rs`**：

将 `Client` 替换为 `ClientWithMiddleware`，在 Phase 2 代码中插入 retry middleware：

```rust
use reqwest_middleware::{ClientBuilder as MiddlewareBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;

impl HttpTransport {
    pub fn new(config: HttpClientConfig) -> Result<Self, CatcherError> {
        // ... Phase 2 的 reqwest Client 构建 ...
        let reqwest_client = reqwest_builder.build()?;

        // Phase 3: 包装 middleware
        let mut client_builder = MiddlewareBuilder::new(reqwest_client);

        if let Some(ref retry_config) = config.retry {
            let policy = build_retry_policy(retry_config);
            client_builder = client_builder
                .with(RetryTransientMiddleware::new_with_policy(
                    policy, "catcher-rs"
                ));
        }

        // 后续 Phase 3.3：熔断器 middleware
        // if let Some(ref cb_config) = config.circuit_breaker {
        //     client_builder = client_builder.with(CircuitBreakerMiddleware::new(cb_config));
        // }

        Ok(Self {
            client: client_builder.build(),
            config,
        })
    }
}
```

### Step 3.3 — `src/resilience/circuit_breaker.rs`

**参考**：`arch-rs/05-resilience.md`, `arch-rs/12-state-machines.md` (熔断器状态机)

```rust
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use crate::types::resilience::{CircuitBreakerConfig, CbState};

/// 熔断器状态机
/// 
/// 状态迁移：
/// CLOSED ──(consecutive_failures >= threshold)──▶ OPEN
/// OPEN   ──(after reset_timeout)───────────────▶ HALF_OPEN
/// HALF_OPEN ──(consecutive_success >= threshold)─▶ CLOSED
/// HALF_OPEN ──(any failure)─────────────────────▶ OPEN
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Mutex<CbState>,
    failure_count: AtomicU32,
    success_count: AtomicU32,
    opened_at: AtomicU64,         // UNIX millis when tripped to OPEN
    half_open_requests: AtomicU32, // count of requests in HALF_OPEN
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self;

    /// 请求前检查：返回 Err(CircuitBreakerOpen) 如果熔断
    pub fn before_request(&self) -> Result<(), CatcherError>;

    /// 请求成功后调用
    pub fn on_success(&self);

    /// 请求失败后调用
    pub fn on_failure(&self);

    /// 当前状态（用于 metrics）
    pub fn state(&self) -> CbState;
}
```

**完整的状态迁移逻辑**：

```rust
pub fn before_request(&self) -> Result<(), CatcherError> {
    let state = *self.state.lock().unwrap();
    match state {
        CbState::Open => {
            let opened_at = self.opened_at.load(Ordering::Relaxed);
            let elapsed = now_millis() - opened_at;
            if elapsed >= self.config.reset_timeout_ms {
                // 进入 HALF_OPEN
                *self.state.lock().unwrap() = CbState::HalfOpen;
                self.half_open_requests.store(0, Ordering::Relaxed);
                // 允许通过继续
            } else {
                return Err(CatcherError::CircuitBreakerOpen);
            }
        }
        CbState::HalfOpen => {
            // 限制半开状态下的最大试探请求数
            let count = self.half_open_requests.fetch_add(1, Ordering::Relaxed);
            if count >= self.config.half_open_max_requests {
                return Err(CatcherError::CircuitBreakerOpen);
            }
        }
        CbState::Closed => {} // 正常通过
    }
    Ok(())
}

pub fn on_success(&self) {
    let state = *self.state.lock().unwrap();
    match state {
        CbState::HalfOpen => {
            let count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.config.success_threshold {
                // HALF_OPEN → CLOSED
                *self.state.lock().unwrap() = CbState::Closed;
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
            }
        }
        CbState::Closed => {
            self.failure_count.store(0, Ordering::Relaxed);
        }
        CbState::Open => {} // 不应发生
    }
}

pub fn on_failure(&self) {
    let state = *self.state.lock().unwrap();
    match state {
        CbState::Closed => {
            let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
            if count >= self.config.failure_threshold {
                // CLOSED → OPEN
                *self.state.lock().unwrap() = CbState::Open;
                self.opened_at.store(now_millis(), Ordering::Relaxed);
            }
        }
        CbState::HalfOpen => {
            // 任何失败 → 回到 OPEN
            *self.state.lock().unwrap() = CbState::Open;
            self.opened_at.store(now_millis(), Ordering::Relaxed);
            self.success_count.store(0, Ordering::Relaxed);
        }
        CbState::Open => {} // 不应发生
    }
}
```

### Step 3.4 — 将 CircuitBreaker 集成到 HttpTransport

在 `HttpTransport::execute()` 方法开头插入熔断器检查：

```rust
pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
    // 熔断器检查
    if let Some(ref cb) = self.circuit_breaker {
        cb.before_request()?;
    }

    let result = self.do_execute(request).await;

    // 熔断器反馈
    match &result {
        Ok(_) => {
            if let Some(ref cb) = self.circuit_breaker {
                cb.on_success();
            }
        }
        Err(e) => {
            if let Some(ref cb) = self.circuit_breaker {
                cb.on_failure();
            }
        }
    }

    result
}
```

### Step 3.5 — `src/resilience/retry.rs`

**参考**：`arch-rs/05-resilience.md`

```rust
use backon::{Retryable, ExponentialBuilder, ConstantBuilder};
use crate::error::{CatcherError, ErrorCategory};
use crate::types::resilience::*;

/// 对异步操作执行重试，带指数退避 + jitter
/// 
/// 适用范围：非 HTTP 场景（WS 连接重试、DNS 解析重试）
/// HTTP 路径的重试由 reqwest-retry 中间件在 Transport 层处理
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
    // 实现详见 arch-rs/05-resilience.md
}
```

### Step 3.6 — `src/resilience/timeout.rs`

```rust
use std::collections::VecDeque;
use std::time::Duration;
use crate::types::observability::RttSnapshot;

/// 自适应超时：基于 P90 RTT 动态计算超时时间
pub struct AdaptiveTimeout {
    rtt_window: VecDeque<u64>,
    window_size: usize,
    multiplier: f64,          // 默认 5x P90
    min_timeout_ms: u64,
    max_timeout_ms: u64,
}

impl AdaptiveTimeout {
    pub fn new(window_size: usize, multiplier: f64, min_ms: u64, max_ms: u64) -> Self;

    /// 记录一次 RTT 样本
    pub fn record(&mut self, rtt_ms: u64);

    /// 返回当前建议的超时时间（ms）
    pub fn timeout_ms(&self) -> u64;

    /// 返回滑动窗口快照
    pub fn snapshot(&self) -> RttSnapshot;
}

impl AdaptiveTimeout {
    pub fn timeout_ms(&self) -> u64 {
        if self.rtt_window.is_empty() {
            return self.max_timeout_ms;
        }
        let mut sorted: Vec<u64> = self.rtt_window.iter().copied().collect();
        sorted.sort_unstable();
        let p90_idx = ((sorted.len() as f64) * 0.90).ceil() as usize - 1;
        let p90 = sorted[p90_idx.min(sorted.len() - 1)];
        let timeout = (p90 as f64 * self.multiplier) as u64;
        timeout.clamp(self.min_timeout_ms, self.max_timeout_ms)
    }
}
```

---

## 3. 测试清单

### 3.1 重试测试（`tests/resilience/retry_test.rs`）

| 测试 | 描述 |
|------|------|
| `retry_succeeds_after_2_failures` | 前 2 次 Timeout，第 3 次成功 |
| `retry_exhausted_after_max_attempts` | 全部失败 → RetryExhausted |
| `retry_fails_fast_on_non_retryable` | EncodeError → 不重试，直接返回 |
| `retry_fails_fast_on_4xx` | HttpError(403) → 不重试 |
| `retry_uses_exponential_backoff` | 延迟逐次翻倍 |
| `on_retry_callback_invoked` | 每次重试触发回调 |
| `no_retry_on_first_success` | 首次成功 → 不调用 on_retry |

### 3.2 熔断器测试（`tests/resilience/circuit_breaker_test.rs`）

| 测试 | 描述 |
|------|------|
| `cb_closed_by_default` | 初始状态为 Closed |
| `cb_opens_after_threshold_failures` | 5 次连续失败 → OPEN |
| `cb_before_request_rejects_when_open` | OPEN 时 before_request → Err(CircuitBreakerOpen) |
| `cb_transitions_to_half_open_after_timeout` | reset_timeout 后 before_request → 进入 HALF_OPEN |
| `cb_half_open_to_closed_on_success_threshold` | HALF_OPEN 中连续成功 → CLOSED |
| `cb_half_open_to_open_on_any_failure` | HALF_OPEN 中失败 → 回到 OPEN |
| `cb_success_resets_failure_count` | CLOSED 中成功 → failure_count 归零 |
| `cb_half_open_limits_probe_requests` | 超过 half_open_max_requests → 拒绝 |

### 3.3 自适应超时测试

| 测试 | 描述 |
|------|------|
| `timeout_uses_p90` | [100, 200, 300] → P90=300 → timeout=1500ms |
| `timeout_clamps_to_min` | P90 * multiplier < min → return min |
| `timeout_clamps_to_max` | P90 * multiplier > max → return max |
| `timeout_defaults_to_max_when_empty` | 无样本 → return max_timeout_ms |
| `record_updates_window` | record() 后 snapshot().sample_count 递增 |

---

## 4. Phase 3 完成标准

- [ ] `cargo test` 全部通过（retry 7 + cb 8 + timeout 5 = ≥20 个新测试）
- [ ] `HttpTransport` 使用 `reqwest-middleware` + `RetryTransientMiddleware`
- [ ] `CircuitBreaker` 完整状态迁移通过测试
- [ ] `retry_with_backoff` 支持 Fixed / Exponential / DecorrelatedJitter 三种退避
- [ ] `AdaptiveTimeout` P90 计算正确
- [ ] `cargo clippy -- -D warnings` 零警告
