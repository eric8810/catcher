# 08 — 可观测性层

> 对应源文件：`src/observability/`

---

## NetworkQualityEvaluator (`src/observability/network_quality.rs`)

```rust
use std::time::Instant;
use crate::error::CatcherError;
use crate::types::observability::*;

/// 网络质量评估器：HTTP HEAD RTT + 滑动窗口 + 综合评分
pub struct NetworkQualityEvaluator {
    http_client: reqwest::Client,
    sliding_window: Vec<u64>,
    window_size: usize,
}

impl NetworkQualityEvaluator {
    pub fn new(window_size: usize) -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
            sliding_window: Vec::with_capacity(window_size),
            window_size,
        }
    }

    /// 单次 HTTP HEAD RTT 测量
    pub async fn measure_http_rtt(&mut self, host: &str, path: &str) -> Result<u64, CatcherError> {
        let url = format!("{}{}", host, path);
        let start = Instant::now();
        let resp = self.http_client.head(&url).send().await.map_err(|e| {
            CatcherError::Internal(format!("HEAD {url}: {e}"))
        })?;
        if resp.status().is_success() || resp.status().as_u16() == 404 {
            let rtt = start.elapsed().as_millis() as u64;
            self.record_rtt(rtt);
            Ok(rtt)
        } else {
            Err(CatcherError::HttpError { status: resp.status().as_u16(), body: String::new() })
        }
    }

    /// 多次测量取平均值
    pub async fn measure_rtt_average(
        &mut self, host: &str, path: &str, count: u32,
    ) -> Result<u64, CatcherError> {
        let mut total = 0u64;
        let mut ok = 0u32;
        for _ in 0..count {
            if let Ok(rtt) = self.measure_http_rtt(host, path).await { total += rtt; ok += 1; }
        }
        if ok == 0 { return Err(CatcherError::Internal("all RTT measurements failed".into())); }
        Ok(total / ok as u64)
    }

    fn record_rtt(&mut self, rtt_ms: u64) {
        if self.sliding_window.len() >= self.window_size { self.sliding_window.remove(0); }
        self.sliding_window.push(rtt_ms);
    }

    /// RTT 滑动窗口统计
    pub fn rtt_snapshot(&self) -> RttSnapshot {
        if self.sliding_window.is_empty() {
            return RttSnapshot { avg_rtt_ms: 0, min_rtt_ms: 0, max_rtt_ms: 0, jitter_ms: 0, packet_loss_rate: 0.0, sample_count: 0 };
        }
        let mut sorted = self.sliding_window.clone();
        sorted.sort_unstable();
        let sum: u64 = sorted.iter().sum();
        let avg = sum / sorted.len() as u64;
        let jitter = if sorted.len() > 1 {
            sorted.iter().map(|&v| (v as i64 - avg as i64).unsigned_abs() as u64).sum::<u64>() / sorted.len() as u64
        } else { 0 };
        RttSnapshot {
            avg_rtt_ms: avg, min_rtt_ms: sorted[0], max_rtt_ms: sorted[sorted.len() - 1],
            jitter_ms: jitter, packet_loss_rate: 0.0, sample_count: sorted.len() as u32,
        }
    }

    /// 综合评分
    pub fn evaluate(&self, snapshot: &RttSnapshot) -> NetworkQualityLevel {
        if snapshot.sample_count == 0 { return NetworkQualityLevel::Unknown; }
        let rtt = snapshot.avg_rtt_ms;
        let jitter = snapshot.jitter_ms;
        if rtt < 50 && jitter < 20       { NetworkQualityLevel::Excellent }
        else if rtt < 100 && jitter < 50 { NetworkQualityLevel::Good }
        else if rtt < 200                { NetworkQualityLevel::Fair }
        else if rtt < 300                { NetworkQualityLevel::Poor }
        else                              { NetworkQualityLevel::Bad }
    }
}
```

---

## MetricsCollector (`src/observability/metrics.rs`)

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use parking_lot::RwLock;
use crate::types::observability::NetworkQualityLevel;

/// 线程安全的指标收集器
pub struct MetricsCollector {
    pub total_requests: AtomicU64,
    pub successful_requests: AtomicU64,
    pub total_retries: AtomicU64,
    pub circuit_breaker_trips: AtomicU64,
    pub current_quality: RwLock<NetworkQualityLevel>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            total_retries: AtomicU64::new(0),
            circuit_breaker_trips: AtomicU64::new(0),
            current_quality: RwLock::new(NetworkQualityLevel::Unknown),
        }
    }

    pub fn record_request(&self, success: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if success { self.successful_requests.fetch_add(1, Ordering::Relaxed); }
    }

    pub fn record_retry(&self) { self.total_retries.fetch_add(1, Ordering::Relaxed); }
    pub fn record_cb_trip(&self) { self.circuit_breaker_trips.fetch_add(1, Ordering::Relaxed); }

    pub fn update_quality(&self, level: NetworkQualityLevel) {
        *self.current_quality.write() = level;
    }

    pub fn success_rate(&self) -> f64 {
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 { return 1.0; }
        self.successful_requests.load(Ordering::Relaxed) as f64 / total as f64
    }
}
```
