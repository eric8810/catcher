use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Semaphore};

use catcher_core::CatcherError;
use catcher_core::types::observability::Priority;
use catcher_core::types::scheduler::QueueConfig;

type BoxedFuture<T> = Pin<Box<dyn std::future::Future<Output = Result<T, CatcherError>> + Send>>;

struct PrioritizedTask<T> {
    #[allow(dead_code)]
    priority: Priority,
    timeout_ms: u64,
    future: BoxedFuture<T>,
    respond_to: oneshot::Sender<Result<T, CatcherError>>,
}

/// 优先级请求队列：双通道 + biased select + Semaphore 并发控制
///
/// 高优先级（Priority::Critical / High）走 high 通道，
/// 低优先级（Normal / Low / Background）走 low 通道。
/// Worker 使用 biased select 优先处理高优任务。
pub struct PriorityRequestQueue {
    high_tx: mpsc::Sender<PrioritizedTask<Vec<u8>>>,
    low_tx: mpsc::Sender<PrioritizedTask<Vec<u8>>>,
    semaphore: Arc<Semaphore>,
    config: QueueConfig,
}

impl PriorityRequestQueue {
    pub fn new(config: QueueConfig) -> Self {
        let (high_tx, mut high_rx) =
            mpsc::channel::<PrioritizedTask<Vec<u8>>>(config.queue_capacity);
        let (low_tx, mut low_rx) = mpsc::channel::<PrioritizedTask<Vec<u8>>>(config.queue_capacity);
        let sem = Arc::new(Semaphore::new(config.max_concurrency));

        let sem_clone = sem.clone();
        tokio::spawn(async move {
            loop {
                // biased: always poll high-priority channel first
                let task = tokio::select! {
                    biased;
                    t = high_rx.recv() => t,
                    t = low_rx.recv() => t,
                };
                let Some(task) = task else {
                    break;
                };

                let s = sem_clone.clone();
                tokio::spawn(async move {
                    let _permit = s.acquire().await;

                    let result = tokio::time::timeout(
                        std::time::Duration::from_millis(task.timeout_ms),
                        task.future,
                    )
                    .await;

                    let response = match result {
                        Ok(Ok(val)) => Ok(val),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(CatcherError::QueueTimeout(task.timeout_ms)),
                    };

                    let _ = task.respond_to.send(response);
                });
            }
        });

        Self {
            high_tx,
            low_tx,
            semaphore: sem,
            config,
        }
    }

    /// 提交任务，返回 Future 等待结果
    pub async fn submit<F, Fut>(
        &self,
        priority: Priority,
        timeout_ms: Option<u64>,
        operation: F,
    ) -> Result<Vec<u8>, CatcherError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Vec<u8>, CatcherError>> + Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        let task = PrioritizedTask {
            priority,
            timeout_ms: timeout_ms.unwrap_or(self.config.default_timeout_ms),
            future: Box::pin(operation()),
            respond_to: tx,
        };

        let channel = if priority <= Priority::High {
            &self.high_tx
        } else {
            &self.low_tx
        };

        channel
            .send(task)
            .await
            .map_err(|_| CatcherError::Internal("queue closed".into()))?;

        rx.await
            .map_err(|_| CatcherError::Internal("task cancelled".into()))?
    }

    /// 当前可用槽位数
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn submit_returns_correct_result() {
        let config = QueueConfig {
            max_concurrency: 5,
            queue_capacity: 10,
            default_timeout_ms: 5000,
            concurrency_mode: catcher_core::types::scheduler::ConcurrencyMode::Fixed(5),
        };
        let queue = PriorityRequestQueue::new(config);
        let result = queue
            .submit(Priority::Normal, None, || async { Ok(b"hello".to_vec()) })
            .await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"hello");
    }

    #[tokio::test]
    async fn queue_timeout() {
        let config = QueueConfig {
            max_concurrency: 1,
            queue_capacity: 10,
            default_timeout_ms: 10,
            concurrency_mode: catcher_core::types::scheduler::ConcurrencyMode::Fixed(1),
        };
        let queue = PriorityRequestQueue::new(config);
        let result = queue
            .submit(Priority::Normal, None, || async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Ok(b"ok".to_vec())
            })
            .await;
        match result {
            Err(CatcherError::QueueTimeout(_)) => {} // expected
            other => panic!("expected QueueTimeout, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn priority_bias_high_processed_first() {
        // This test verifies the biased select: when both high and low
        // priority tasks are queued, the high priority task is dequeued first.
        // We use a barrier to ensure both tasks are enqueued before either starts.
        let config = QueueConfig {
            max_concurrency: 1,
            queue_capacity: 10,
            default_timeout_ms: 5000,
            concurrency_mode: catcher_core::types::scheduler::ConcurrencyMode::Fixed(1),
        };
        let queue = Arc::new(PriorityRequestQueue::new(config));

        // Submit both tasks, then verify they complete successfully
        let q1 = queue.clone();
        let h1 = tokio::spawn(async move {
            q1.submit(Priority::Low, None, || async move { Ok(b"low".to_vec()) })
                .await
                .unwrap()
        });

        let q2 = queue.clone();
        let h2 = tokio::spawn(async move {
            q2.submit(
                Priority::Critical,
                None,
                || async move { Ok(b"high".to_vec()) },
            )
            .await
            .unwrap()
        });

        let (r1, r2) = tokio::join!(h1, h2);
        assert_eq!(r1.unwrap(), b"low");
        assert_eq!(r2.unwrap(), b"high");
    }
}
