use backon::{ConstantBuilder, ExponentialBuilder, Retryable};
use catcher_core::types::resilience::{BackoffKind, RetryConfig};
use catcher_core::{CatcherError, ErrorCategory};
use std::cell::Cell;
use std::time::Duration;

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
            action
                .retry(backoff)
                .when(|e: &CatcherError| {
                    let a = attempt.get();
                    let should =
                        e.category() == ErrorCategory::Retryable && retry_if(e) && a < max_attempts;
                    if should {
                        on_retry(a, e);
                    }
                    should
                })
                .sleep(tokio::time::sleep)
                .await
        }
        BackoffKind::Exponential => {
            let backoff = ExponentialBuilder::default()
                .with_min_delay(min_delay)
                .with_max_delay(max_delay)
                .with_factor(2.0);
            action
                .retry(backoff)
                .when(|e: &CatcherError| {
                    let a = attempt.get();
                    let should =
                        e.category() == ErrorCategory::Retryable && retry_if(e) && a < max_attempts;
                    if should {
                        on_retry(a, e);
                    }
                    should
                })
                .sleep(tokio::time::sleep)
                .await
        }
        BackoffKind::DecorrelatedJitter => {
            let backoff = ExponentialBuilder::default()
                .with_min_delay(min_delay)
                .with_max_delay(max_delay)
                .with_factor(2.0)
                .with_jitter();
            action
                .retry(backoff)
                .when(|e: &CatcherError| {
                    let a = attempt.get();
                    let should =
                        e.category() == ErrorCategory::Retryable && retry_if(e) && a < max_attempts;
                    if should {
                        on_retry(a, e);
                    }
                    should
                })
                .sleep(tokio::time::sleep)
                .await
        }
    };

    // Only wrap as RetryExhausted if we actually attempted retries
    let final_attempt = attempt.get();
    result.map_err(|e| {
        if final_attempt > 1 {
            CatcherError::RetryExhausted {
                attempts: max_attempts,
                last_error: format!("{e}"),
            }
        } else {
            e
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn test_config() -> RetryConfig {
        RetryConfig {
            max_attempts: 3,
            backoff: BackoffKind::Exponential,
            min_backoff_ms: 10,
            max_backoff_ms: 100,
            jitter: false,
        }
    }

    #[tokio::test]
    async fn retry_succeeds_after_2_failures() {
        let calls = AtomicU32::new(0);
        let result = retry_with_backoff(
            &test_config(),
            || {
                let c = calls.fetch_add(1, Ordering::SeqCst);
                async move {
                    if c < 2 {
                        Err(CatcherError::ConnectionTimeout(100))
                    } else {
                        Ok(42u32)
                    }
                }
            },
            |_| true,
            |_, _| {},
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_exhausted_after_max_attempts() {
        let calls = AtomicU32::new(0);
        let result = retry_with_backoff(
            &test_config(),
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async move { Err::<u32, _>(CatcherError::ConnectionTimeout(100)) }
            },
            |_| true,
            |_, _| {},
        )
        .await;
        assert!(result.is_err());
        match result {
            Err(CatcherError::RetryExhausted { .. }) => {}
            other => panic!("expected RetryExhausted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn retry_fails_fast_on_non_retryable() {
        let calls = AtomicU32::new(0);
        let result = retry_with_backoff(
            &test_config(),
            || {
                calls.fetch_add(1, Ordering::SeqCst);
                async move { Err::<u32, _>(CatcherError::EncodeError("bad".into())) }
            },
            |_| true,
            |_, _| {},
        )
        .await;
        match result {
            Err(CatcherError::EncodeError(_)) => {}
            other => panic!("expected EncodeError, got {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn on_retry_callback_invoked() {
        let retry_count = AtomicU32::new(0);
        let _ = retry_with_backoff(
            &test_config(),
            || async { Err::<u32, _>(CatcherError::ConnectionTimeout(100)) },
            |_| true,
            |attempt, _| {
                retry_count.fetch_add(1, Ordering::SeqCst);
                assert!(attempt > 0);
            },
        )
        .await;
        assert!(retry_count.load(Ordering::SeqCst) > 0);
    }

    #[tokio::test]
    async fn no_retry_on_first_success() {
        let retry_count = AtomicU32::new(0);
        let result = retry_with_backoff(
            &test_config(),
            || async { Ok(1u32) },
            |_| true,
            |_, _| {
                retry_count.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(retry_count.load(Ordering::SeqCst), 0);
    }
}
