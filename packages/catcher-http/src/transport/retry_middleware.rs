//! Custom retry middleware that increments `MetricsCollector::http_retries` on each retry attempt.
//!
//! Replaces `reqwest_retry::RetryTransientMiddleware` which has no callback mechanism.

use std::sync::Weak;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use http::Extensions;
use reqwest::{Request, Response};
use reqwest_middleware::{Error, Middleware, Next, Result};
use reqwest_retry::{
    DefaultRetryableStrategy, Retryable, RetryableStrategy, RetryError,
};
use retry_policies::RetryPolicy;

/// Retry middleware that notifies `MetricsCollector` on each retry attempt.
///
/// Functionally equivalent to `reqwest_retry::RetryTransientMiddleware` but calls
/// `MetricsCollector.increment_http_retries()` (via the shared `Arc<AtomicU64>`) for
/// every retry performed.
pub struct MetricsRetryMiddleware<
    T: RetryPolicy + Send + Sync + 'static,
    R: RetryableStrategy + Send + Sync + 'static = DefaultRetryableStrategy,
> {
    retry_policy: T,
    retryable_strategy: R,
    retries_counter: Weak<AtomicU64>,
}

impl<T: RetryPolicy + Send + Sync> MetricsRetryMiddleware<T, DefaultRetryableStrategy> {
    /// Construct with a retry policy and a weak reference to the `http_retries` counter.
    pub fn new(retry_policy: T, retries_counter: Weak<AtomicU64>) -> Self {
        Self {
            retry_policy,
            retryable_strategy: DefaultRetryableStrategy,
            retries_counter,
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<T, R> Middleware for MetricsRetryMiddleware<T, R>
where
    T: RetryPolicy + Send + Sync,
    R: RetryableStrategy + Send + Sync + 'static,
{
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        self.execute_with_retry(req, next, extensions).await
    }
}

impl<T, R> MetricsRetryMiddleware<T, R>
where
    T: RetryPolicy + Send + Sync,
    R: RetryableStrategy + Send + Sync,
{
    async fn execute_with_retry<'a>(
        &'a self,
        req: Request,
        next: Next<'a>,
        ext: &'a mut Extensions,
    ) -> Result<Response> {
        let mut n_past_retries: u32 = 0;
        let start_time = SystemTime::now();

        loop {
            let duplicate_request = req.try_clone().ok_or_else(|| {
                Error::middleware(NoCloneError)
            })?;

            let result = next.clone().run(duplicate_request, ext).await;

            if let Some(Retryable::Transient) = self.retryable_strategy.handle(&result) {
                let retry_decision = self.retry_policy.should_retry(start_time, n_past_retries);
                if let retry_policies::RetryDecision::Retry { execute_after } = retry_decision {
                    let duration = execute_after
                        .duration_since(SystemTime::now())
                        .unwrap_or_else(|_| Duration::default());

                    #[cfg(not(target_arch = "wasm32"))]
                    tokio::time::sleep(duration).await;
                    #[cfg(target_arch = "wasm32")]
                    wasmtimer::tokio::sleep(duration).await;

                    n_past_retries += 1;

                    // Increment the metrics counter if the Arc is still alive.
                    if let Some(counter) = self.retries_counter.upgrade() {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }

                    continue;
                }
            };

            break if n_past_retries > 0 {
                result.map_err(|err| {
                    Error::middleware(RetryError::WithRetries {
                        retries: n_past_retries,
                        err,
                    })
                })
            } else {
                result.map_err(|err| Error::middleware(RetryError::Error(err)))
            };
        }
    }
}

/// Error returned when a request body cannot be cloned for retry.
#[derive(Debug)]
struct NoCloneError;

impl std::fmt::Display for NoCloneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Request object is not cloneable. Are you passing a streaming body?")
    }
}

impl std::error::Error for NoCloneError {}
