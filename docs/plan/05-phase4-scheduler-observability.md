# 05 — Phase 4: Scheduler + Observability

> 对应 arch 文档：`06-scheduler.md`, `08-observability.md`
> 工期：5 人天
> 目标：优先级请求队列、动态并发控制、网络质量评估器、指标收集器全部就绪

---

## 1. 模块概览

```
src/scheduler/
├── mod.rs
├── priority_queue.rs   # PriorityRequestQueue: 双通道 mpsc + Semaphore
└── concurrency.rs      # concurrency_for_quality(): 网络质量 → 并发数

src/observability/
├── mod.rs
├── network_quality.rs  # NetworkQualityEvaluator: HTTP HEAD RTT 测量
└── metrics.rs          # MetricsCollector: 延迟/成功率/熔断状态收集
```

---

## 2. 实现步骤

### Step 4.1 — `src/scheduler/priority_queue.rs`

**参考**：`arch-rs/06-scheduler.md`

双通道 + biased select + Semaphore 并发控制：

```rust
use tokio::sync::{Semaphore, mpsc, oneshot};
use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use crate::error::CatcherError;
use crate::types::observability::Priority;
use crate::types::scheduler::QueueConfig;

type BoxedFuture<T> = Pin<Box<dyn Future<Output = Result<T, CatcherError>> + Send>>;

struct PrioritizedTask<T> {
    priority: Priority,
    timeout_ms: u64,
    future: BoxedFuture<T>,
    respond_to: oneshot::Sender<Result<T, CatcherError>>,
}

/// 优先级请求队列：高优 + 低优双通道，biased select 优先取高优
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
        let (low_tx, mut low_rx) =
            mpsc::channel::<PrioritizedTask<Vec<u8>>>(config.queue_capacity);
        let sem = Arc::new(Semaphore::new(config.max_concurrency));

        let sem_clone = sem.clone();
        let default_timeout = config.default_timeout_ms;

        tokio::spawn(async move {
            loop {
                // biased select: 总是先检查高优通道
                let task = tokio::select! {
                    biased;
                    t = high_rx.recv() => t,
                    t = low_rx.recv()  => t,
                };

                let Some(task) = task else { break; };

                let s = sem_clone.clone();
                tokio::spawn(async move {
                    let _permit = s.acquire().await;

                    let result = tokio::time::timeout(
                        std::time::Duration::from_millis(task.timeout_ms),
                        task.future,
                    ).await;

                    let response = match result {
                        Ok(Ok(val)) => Ok(val),
                        Ok(Err(e)) => Err(e),
                        Err(_) => Err(CatcherError::QueueTimeout(task.timeout_ms)),
                    };

                    let _ = task.respond_to.send(response);
                });
            }
        });

        Self { high_tx, low_tx, semaphore: sem, config }
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
        Fut: Future<Output = Result<Vec<u8>, CatcherError>> + Send + 'static,
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

        channel.send(task).await
            .map_err(|_| CatcherError::Internal("queue closed".into()))?;

        rx.await
            .map_err(|_| CatcherError::Internal("task cancelled".into()))?
    }

    /// 动态调整并发数（由 NetworkQualityEvaluator 驱动）
    pub fn set_concurrency(&self, max: usize) {
        // 通过 Semaphore 的 add_permits / forget_permits 调整
        let current = self.semaphore.available_permits();
        let target = max;
        // ... 调整逻辑
    }
}
```

**关键设计**：
- `biased` select 确保高优通道永远优先处理
- 每个任务带独立的 `timeout_ms`，超时返回 `QueueTimeout`
- Worker goroutine 使用 `tokio::spawn` 并发执行
- Semaphore 控制最大并发数

### Step 4.2 — `src/scheduler/concurrency.rs`

```rust
use crate::types::observability::NetworkQualityLevel;

/// 根据网络质量返回建议的并发数
pub fn concurrency_for_quality(level: NetworkQualityLevel) -> usize {
    match level {
        NetworkQualityLevel::Excellent => 50,
        NetworkQualityLevel::Good      => 25,
        NetworkQualityLevel::Fair      => 10,
        NetworkQualityLevel::Poor      => 5,
        NetworkQualityLevel::Bad       => 2,
    }
}
```

**测试**：`tests/scheduler/concurrency_test.rs`
- `excellent_returns_50`
- `bad_returns_2`
- `all_levels_have_non_zero_concurrency`

### Step 4.3 — `src/observability/network_quality.rs`

**参考**：`arch-rs/08-observability.md`

```rust
use std::time::Instant;
use std::collections::VecDeque;
use crate::error::CatcherError;
use crate::types::observability::*;

/// 网络质量评估器：HTTP HEAD RTT + 滑动窗口 + 综合评分
pub struct NetworkQualityEvaluator {
    http_client: reqwest::Client,
    sliding_window: VecDeque<u64>,
    window_size: usize,
    connection_type: ConnectionType,
}

impl NetworkQualityEvaluator {
    pub fn new(window_size: usize) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
            sliding_window: VecDeque::with_capacity(window_size),
            window_size,
            connection_type: ConnectionType::Unknown,
        }
    }

    /// 单次 HTTP HEAD RTT 测量
    pub async fn measure_http_rtt(
        &mut self, host: &str, path: &str,
    ) -> Result<u64, CatcherError> {
        let url = format!("{}{}", host, path);
        let start = Instant::now();
        let resp = self.http_client.head(&url).send().await.map_err(|e| {
            CatcherError::Internal(format!("HEAD {url}: {e}"))
        })?;
        // 2xx 或 404 都算成功（404 说明服务可达）
        if resp.status().is_success() || resp.status().as_u16() == 404 {
            let rtt = start.elapsed().as_millis() as u64;
            self.record_rtt(rtt);
            Ok(rtt)
        } else {
            Err(CatcherError::HttpError {
                status: resp.status().as_u16(),
                body: String::new(),
            })
        }
    }

    /// 多次测量取平均值
    pub async fn measure_rtt_average(
        &mut self, host: &str, path: &str, count: u32,
    ) -> Result<u64, CatcherError> {
        let mut total = 0u64;
        let mut ok = 0u32;
        for _ in 0..count {
            if let Ok(rtt) = self.measure_http_rtt(host, path).await {
                total += rtt;
                ok += 1;
            }
        }
        if ok == 0 {
            return Err(CatcherError::Internal(
                "all RTT measurements failed".into()
            ));
        }
        Ok(total / ok as u64)
    }

    fn record_rtt(&mut self, rtt_ms: u64) {
        if self.sliding_window.len() >= self.window_size {
            self.sliding_window.pop_front();
        }
        self.sliding_window.push_back(rtt_ms);
    }

    /// RTT 滑动窗口统计快照
    pub fn rtt_snapshot(&self) -> RttSnapshot {
        if self.sliding_window.is_empty() {
            return RttSnapshot {
                avg_rtt_ms: 0, min_rtt_ms: 0, max_rtt_ms: 0,
                jitter_ms: 0, packet_loss_rate: 0.0, sample_count: 0,
            };
        }
        let mut sorted: Vec<u64> = self.sliding_window.iter().copied().collect();
        sorted.sort_unstable();
        let sum: u64 = sorted.iter().sum();
        let avg = sum / sorted.len() as u64;
        let jitter = if sorted.len() > 1 {
            sorted.iter()
                .map(|&v| (v as i64 - avg as i64).unsigned_abs() as u64)
                .sum::<u64>()
                / sorted.len() as u64
        } else {
            0
        };
        RttSnapshot {
            avg_rtt_ms: avg,
            min_rtt_ms: sorted[0],
            max_rtt_ms: sorted[sorted.len() - 1],
            jitter_ms: jitter,
            packet_loss_rate: 0.0, // 需要额外的丢包计数
            sample_count: sorted.len(),
        }
    }

    /// 综合评估网络质量等级
    pub fn evaluate(&self) -> NetworkQualityResult {
        let snapshot = self.rtt_snapshot();
        let level = classify_quality(&snapshot);
        NetworkQualityResult {
            level,
            avg_rtt_ms: snapshot.avg_rtt_ms,
            jitter_ms: snapshot.jitter_ms,
            packet_loss_rate: snapshot.packet_loss_rate,
            connection_type: self.connection_type,
        }
    }
}

/// RTT + jitter → NetworkQualityLevel
fn classify_quality(snapshot: &RttSnapshot) -> NetworkQualityLevel {
    let rtt = snapshot.avg_rtt_ms;
    let jitter = snapshot.jitter_ms;
    match (rtt, jitter) {
        (r, _) if r < 80   => NetworkQualityLevel::Excellent,
        (r, _) if r < 200  => NetworkQualityLevel::Good,
        (r, j) if r < 500  && j < 150 => NetworkQualityLevel::Fair,
        (r, _) if r < 1000 => NetworkQualityLevel::Poor,
        _                   => NetworkQualityLevel::Bad,
    }
}
```

### Step 4.4 — `src/observability/metrics.rs`

```rust
use std::sync::atomic::{AtomicU64, AtomicU32, Ordering};
use std::time::Instant;

/// 轻量级指标收集器（无外部依赖，纯 atomic 计数）
#[derive(Default)]
pub struct MetricsCollector {
    // HTTP metrics
    http_requests_total: AtomicU64,
    http_successes: AtomicU64,
    http_failures: AtomicU64,
    http_retries: AtomicU64,
    http_total_latency_us: AtomicU64,

    // WS metrics
    ws_connects_attempted: AtomicU64,
    ws_connects_succeeded: AtomicU64,
    ws_disconnects: AtomicU64,
    ws_messages_sent: AtomicU64,
    ws_messages_received: AtomicU64,

    // Circuit breaker
    cb_open_count: AtomicU64,
    cb_half_open_count: AtomicU64,

    // Queue metrics
    queue_high_priority_enqueued: AtomicU64,
    queue_low_priority_enqueued: AtomicU64,
    queue_timeouts: AtomicU32,
}

impl MetricsCollector {
    pub fn record_http_request(&self, success: bool, latency_us: u64, retried: bool);
    pub fn record_ws_connect(&self, success: bool);
    pub fn record_ws_disconnect(&self);
    pub fn record_ws_message_sent(&self);
    pub fn record_ws_message_received(&self);
    pub fn record_cb_open(&self);
    pub fn record_cb_half_open(&self);
    pub fn record_queue_enqueue(&self, priority: Priority);
    pub fn record_queue_timeout(&self);

    /// 导出快照（所有 atomic 值读出）
    pub fn snapshot(&self) -> MetricsSnapshot;
}

#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub http_requests: u64,
    pub http_success_rate: f64,
    pub http_avg_latency_us: u64,
    pub http_retries: u64,
    pub ws_connect_success_rate: f64,
    pub ws_disconnects: u64,
    pub ws_messages_sent: u64,
    pub ws_messages_received: u64,
    pub cb_open_count: u64,
    pub queue_timeouts: u32,
}
```

---

## 3. 集成到公共 API（`src/lib.rs`）

Phase 4 结束时 `lib.rs`：

```rust
pub mod error;
pub mod config;
pub mod types;
pub mod codec;
pub mod transport;
pub mod resilience;
pub mod ws;
pub mod scheduler;
pub mod observability;

pub use error::{CatcherError, ErrorCategory};
pub use config::CatcherConfig;
pub use transport::http_client::HttpTransport;
pub use transport::ws_client::WsTransport;
pub use scheduler::priority_queue::PriorityRequestQueue;
pub use observability::network_quality::NetworkQualityEvaluator;
pub use observability::metrics::MetricsCollector;
```

---

## 4. 测试清单

### 4.1 优先级队列测试（`tests/scheduler/priority_queue_test.rs`）

| 测试 | 描述 |
|------|------|
| `high_priority_before_low` | 先提交低优再提交高优 → 高优先执行 |
| `semaphore_limits_concurrency` | max_concurrency=2 → 同时最多 2 个任务执行 |
| `queue_timeout` | 任务超时 → QueueTimeout |
| `submit_returns_correct_result` | 提交任务 → 获得正确返回值 |
| `queue_closed_error` | channel 关闭 → Internal error |
| `biased_select_priority` | biased 确保 high 优先于 low |
| `multiple_high_priority_order` | 多个高优按 FIFO 顺序 |

### 4.2 网络质量评估器测试

| 测试 | 描述 |
|------|------|
| `measure_rtt_returns_value` | HEAD 请求返回 RTT > 0 |
| `record_rtt_updates_window` | record() 后 snapshot().sample_count += 1 |
| `rtt_snapshot_calculates_stats` | 多个样本，avg/min/max 正确 |
| `evaluate_excellent_for_low_rtt` | RTT < 80ms → Excellent |
| `evaluate_bad_for_high_rtt` | RTT > 1000ms → Bad |
| `empty_window_returns_zero_snapshot` | 无样本 → 全 0 |
| `concurrency_for_quality_mapping` | 每个质量等级 → 对应并发数 |

---

## 5. Phase 4 完成标准

- [ ] `cargo test` 全部通过（queue 7 + quality 7 = ≥14 个新测试）
- [ ] PriorityRequestQueue 高优优先调度通过测试
- [ ] Semaphore 并发控制生效
- [ ] NetworkQualityEvaluator RTT 测量 + 质量分类通过测试
- [ ] MetricsCollector 所有 atomic 计数器可正确 snapshot
- [ ] `cargo clippy -- -D warnings` 零警告
