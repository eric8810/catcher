use catcher_core::types::resilience::{CbState, CircuitBreakerConfig};
use catcher_core::CatcherError;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 熔断器状态机
///
/// 状态迁移：
/// ```text
/// CLOSED ──(consecutive_failures >= threshold)──▶ OPEN
/// OPEN   ──(after reset_timeout)───────────────▶ HALF_OPEN
/// HALF_OPEN ──(consecutive_success >= threshold)─▶ CLOSED
/// HALF_OPEN ──(any failure)─────────────────────▶ OPEN
/// ```
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
    pub fn before_request(&self) -> Result<(), CatcherError> {
        let mut state = self.state.lock();
        match *state {
            CbState::Closed => {
                // 正常通过
                Ok(())
            }
            CbState::Open => {
                let opened_at = self.opened_at_ms.load(Ordering::Relaxed);
                let now_ms = now_millis();
                if now_ms - opened_at >= self.config.reset_timeout_ms {
                    // 进入 HALF_OPEN
                    *state = CbState::HalfOpen;
                    self.half_open_requests.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                    Ok(())
                } else {
                    Err(CatcherError::CircuitBreakerOpen)
                }
            }
            CbState::HalfOpen => {
                let count = self.half_open_requests.fetch_add(1, Ordering::Relaxed);
                if count >= self.config.half_open_max_requests {
                    Err(CatcherError::CircuitBreakerOpen)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// 请求成功后调用
    pub fn on_success(&self) {
        let mut state = self.state.lock();
        match *state {
            CbState::Closed => {
                self.failure_count.store(0, Ordering::Relaxed);
            }
            CbState::HalfOpen => {
                let count = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if count >= self.config.success_threshold {
                    *state = CbState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    self.success_count.store(0, Ordering::Relaxed);
                }
            }
            CbState::Open => {}
        }
    }

    /// 请求失败后调用
    pub fn on_failure(&self) {
        let mut state = self.state.lock();
        match *state {
            CbState::Closed => {
                let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
                if count >= self.config.failure_threshold {
                    *state = CbState::Open;
                    self.opened_at_ms.store(now_millis(), Ordering::Relaxed);
                }
            }
            CbState::HalfOpen => {
                *state = CbState::Open;
                self.opened_at_ms.store(now_millis(), Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
            }
            CbState::Open => {}
        }
    }

    /// 当前状态（用于 metrics）
    pub fn state(&self) -> CbState {
        *self.state.lock()
    }

    /// 重置熔断器到 CLOSED 状态
    pub fn reset(&self) {
        *self.state.lock() = CbState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        self.half_open_requests.store(0, Ordering::Relaxed);
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            reset_timeout_ms: 100,
            half_open_max_requests: 5,
        }
    }

    #[test]
    fn cb_closed_by_default() {
        let cb = CircuitBreaker::new(test_config());
        assert_eq!(cb.state(), CbState::Closed);
        assert!(cb.before_request().is_ok());
    }

    #[test]
    fn cb_opens_after_threshold_failures() {
        let cb = CircuitBreaker::new(test_config());
        cb.on_failure();
        cb.on_failure();
        assert_eq!(cb.state(), CbState::Closed);
        cb.on_failure(); // 3rd failure → OPEN
        assert_eq!(cb.state(), CbState::Open);
    }

    #[test]
    fn cb_before_request_rejects_when_open() {
        let cb = CircuitBreaker::new(test_config());
        // Force OPEN
        for _ in 0..3 {
            cb.on_failure();
        }
        assert!(cb.before_request().is_err());
    }

    #[test]
    fn cb_transitions_to_half_open_after_timeout() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_ms: 0, // immediate
            ..test_config()
        });
        cb.on_failure(); // OPEN
                         // reset_timeout is 0, so should immediately transition to HALF_OPEN
        let result = cb.before_request();
        assert!(result.is_ok());
        assert_eq!(cb.state(), CbState::HalfOpen);
    }

    #[test]
    fn cb_half_open_to_closed_on_success_threshold() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_ms: 0,
            ..test_config()
        });
        cb.on_failure(); // OPEN
        let _ = cb.before_request(); // → HALF_OPEN
        cb.on_success();
        cb.on_success(); // 2 successes → CLOSED
        assert_eq!(cb.state(), CbState::Closed);
    }

    #[test]
    fn cb_half_open_to_open_on_failure() {
        let cb = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 1,
            reset_timeout_ms: 0,
            ..test_config()
        });
        cb.on_failure(); // OPEN
        let _ = cb.before_request(); // → HALF_OPEN
        cb.on_failure(); // failure in HALF_OPEN → back to OPEN
        assert_eq!(cb.state(), CbState::Open);
    }

    #[test]
    fn cb_success_resets_failure_count() {
        let cb = CircuitBreaker::new(test_config());
        cb.on_failure();
        cb.on_failure();
        cb.on_success(); // reset failure count
        assert_eq!(cb.state(), CbState::Closed);
        // Need 3 more failures to trip again
        cb.on_failure();
        cb.on_failure();
        assert_eq!(cb.state(), CbState::Closed); // still closed
        cb.on_failure(); // 3rd since last success
        assert_eq!(cb.state(), CbState::Open);
    }
}
