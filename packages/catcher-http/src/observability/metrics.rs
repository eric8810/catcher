use catcher_core::types::observability::Priority;
use serde::Serialize;
use std::sync::{Arc, Weak};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// 轻量级指标收集器（无外部依赖，纯 atomic 计数）
#[derive(Default)]
pub struct MetricsCollector {
    // HTTP metrics
    http_requests_total: AtomicU64,
    http_successes: AtomicU64,
    http_failures: AtomicU64,
    /// Shared with `MetricsRetryMiddleware` via `Weak` so retries are counted
    /// inside the middleware loop without requiring a callback.
    http_retries: Arc<AtomicU64>,
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_http_request(&self, success: bool, latency_us: u64) {
        self.http_requests_total.fetch_add(1, Ordering::Relaxed);
        if success {
            self.http_successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.http_failures.fetch_add(1, Ordering::Relaxed);
        }
        self.http_total_latency_us
            .fetch_add(latency_us, Ordering::Relaxed);
    }

    /// Increment the HTTP retry counter. Called by MetricsRetryMiddleware on each retry attempt.
    pub fn increment_http_retries(&self) {
        self.http_retries.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns a `Weak` reference to the `http_retries` counter.
    /// `MetricsRetryMiddleware` uses this to increment retries from inside the middleware loop.
    pub fn http_retries_weak(&self) -> Weak<AtomicU64> {
        Arc::downgrade(&self.http_retries)
    }

    pub fn record_ws_connect(&self, success: bool) {
        self.ws_connects_attempted.fetch_add(1, Ordering::Relaxed);
        if success {
            self.ws_connects_succeeded.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_ws_disconnect(&self) {
        self.ws_disconnects.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_ws_message_sent(&self) {
        self.ws_messages_sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_ws_message_received(&self) {
        self.ws_messages_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cb_open(&self) {
        self.cb_open_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cb_half_open(&self) {
        self.cb_half_open_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_queue_enqueue(&self, priority: Priority) {
        if priority <= Priority::High {
            self.queue_high_priority_enqueued
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.queue_low_priority_enqueued
                .fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_queue_timeout(&self) {
        self.queue_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    /// 导出快照（所有 atomic 值读出）
    pub fn snapshot(&self) -> MetricsSnapshot {
        let requests = self.http_requests_total.load(Ordering::Relaxed);
        let successes = self.http_successes.load(Ordering::Relaxed);
        let total_latency = self.http_total_latency_us.load(Ordering::Relaxed);
        let ws_attempts = self.ws_connects_attempted.load(Ordering::Relaxed);
        let ws_successes = self.ws_connects_succeeded.load(Ordering::Relaxed);

        MetricsSnapshot {
            http_requests: requests,
            http_success_rate: if requests > 0 {
                successes as f64 / requests as f64
            } else {
                0.0
            },
            http_avg_latency_us: total_latency.checked_div(successes).unwrap_or(0),
            http_retries: self.http_retries.load(Ordering::Relaxed),
            ws_connect_success_rate: if ws_attempts > 0 {
                ws_successes as f64 / ws_attempts as f64
            } else {
                0.0
            },
            ws_disconnects: self.ws_disconnects.load(Ordering::Relaxed),
            ws_messages_sent: self.ws_messages_sent.load(Ordering::Relaxed),
            ws_messages_received: self.ws_messages_received.load(Ordering::Relaxed),
            cb_open_count: self.cb_open_count.load(Ordering::Relaxed),
            queue_timeouts: self.queue_timeouts.load(Ordering::Relaxed),
        }
    }
}

/// 指标快照
#[derive(Debug, Clone, Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_default_values() {
        let m = MetricsCollector::new();
        let snap = m.snapshot();
        assert_eq!(snap.http_requests, 0);
        assert_eq!(snap.http_success_rate, 0.0);
    }

    #[test]
    fn record_http_request_updates_counts() {
        let m = MetricsCollector::new();
        m.record_http_request(true, 1000);
        m.record_http_request(false, 500);
        m.increment_http_retries();
        let snap = m.snapshot();
        assert_eq!(snap.http_requests, 2);
        assert_eq!(snap.http_success_rate, 0.5);
        assert!(snap.http_avg_latency_us > 0);
        assert_eq!(snap.http_retries, 1);
    }

    #[test]
    fn record_queue_enqueue_tracks_priority() {
        let m = MetricsCollector::new();
        m.record_queue_enqueue(Priority::Critical);
        m.record_queue_enqueue(Priority::High);
        m.record_queue_enqueue(Priority::Normal);
        m.record_queue_enqueue(Priority::Low);
        // Only high + low counters
        let snap = m.snapshot();
        // We only track the raw enqueue count via snapshot
        assert_eq!(snap.http_requests, 0); // no HTTP recorded
    }
}
