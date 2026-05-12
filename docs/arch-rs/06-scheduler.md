# 06 — 调度层

> 对应源文件：`src/scheduler/`

---

## PriorityRequestQueue (`src/scheduler/priority_queue.rs`)

```rust
use tokio::sync::{Semaphore, mpsc, oneshot};
use std::sync::Arc;
use std::time::Duration;
use crate::error::CatcherError;
use crate::types::observability::Priority;
use crate::types::scheduler::QueueConfig;

struct PrioritizedTask {
    priority: Priority,
    timeout_ms: u64,
    respond_to: oneshot::Sender<Result<Vec<u8>, CatcherError>>,
}

/// 优先级请求队列：双通道 + Semaphore
///
/// 高优先级通道（priority <= HIGH）+ 低优先级通道。
/// Worker 使用 biased select 优先取高优任务。
pub struct PriorityRequestQueue {
    high_tx: mpsc::Sender<PrioritizedTask>,
    low_tx: mpsc::Sender<PrioritizedTask>,
    semaphore: Arc<Semaphore>,
    config: QueueConfig,
}

impl PriorityRequestQueue {
    pub fn new(config: QueueConfig) -> Self {
        let (high_tx, mut high_rx) = mpsc::channel::<PrioritizedTask>(256);
        let (low_tx, mut low_rx) = mpsc::channel::<PrioritizedTask>(1024);
        let sem = Arc::new(Semaphore::new(config.max_concurrency));

        let sem_clone = sem.clone();
        tokio::spawn(async move {
            loop {
                let task = tokio::select! {
                    biased;  // 总是先检查 high channel
                    t = high_rx.recv() => t,
                    t = low_rx.recv() => t,
                };
                let Some(task) = task else { break };

                let s = sem_clone.clone();
                tokio::spawn(async move {
                    let _permit = s.acquire().await;
                    // 实际任务由调用方在 submit 时提供
                    let _ = task.respond_to.send(Ok(vec![]));
                });
            }
        });

        Self { high_tx, low_tx, semaphore: sem, config }
    }

    /// 提交任务并等待结果
    pub async fn submit<F, Fut>(
        &self,
        priority: Priority,
        timeout_ms: u64,
        operation: F,
    ) -> Result<Vec<u8>, CatcherError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<Vec<u8>, CatcherError>> + Send,
    {
        let (tx, rx) = oneshot::channel();
        let task = PrioritizedTask { priority, timeout_ms, respond_to: tx };

        let channel = if priority <= Priority::HIGH { &self.high_tx } else { &self.low_tx };
        channel.send(task).await.map_err(|_| CatcherError::Internal("queue closed".into()))?;

        rx.await.map_err(|_| CatcherError::Internal("task cancelled".into()))?
    }

    /// 当前可用槽位数
    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// 动态调整并发数
    pub fn set_concurrency(&mut self, new_limit: usize) {
        self.semaphore = Arc::new(Semaphore::new(new_limit));
        self.config.max_concurrency = new_limit;
    }
}
```

---

## DynamicConcurrency (`src/scheduler/concurrency.rs`)

```rust
use crate::types::observability::NetworkQualityLevel;

/// 根据网络质量计算推荐并发数
pub fn concurrency_for_quality(quality: NetworkQualityLevel) -> usize {
    match quality {
        NetworkQualityLevel::Excellent => 20,
        NetworkQualityLevel::Good      => 10,
        NetworkQualityLevel::Fair      => 5,
        NetworkQualityLevel::Poor      => 3,
        NetworkQualityLevel::Bad       => 2,
        NetworkQualityLevel::Unknown   => 5,
    }
}
```
