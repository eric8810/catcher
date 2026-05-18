use std::time::Instant;

use catcher_core::CatcherError;
use catcher_core::types::observability::*;

/// 网络质量评估器：HTTP HEAD RTT + 滑动窗口 + 综合评分
pub struct NetworkQualityEvaluator {
    http_client: reqwest::Client,
    sliding_window: Vec<u64>,
    window_size: usize,
    connection_type: ConnectionType,
    cached_snapshot: Option<RttSnapshot>,
    dirty: bool,
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
            connection_type: ConnectionType::Unknown,
            cached_snapshot: None,
            dirty: false,
        }
    }

    /// 带已有 client 创建（复用连接池）
    pub fn with_client(client: reqwest::Client, window_size: usize) -> Self {
        Self {
            http_client: client,
            sliding_window: Vec::with_capacity(window_size),
            window_size,
            connection_type: ConnectionType::Unknown,
            cached_snapshot: None,
            dirty: false,
        }
    }

    /// 设置连接类型
    pub fn set_connection_type(&mut self, ct: ConnectionType) {
        self.connection_type = ct;
    }

    /// 单次 HTTP HEAD RTT 测量
    pub async fn measure_http_rtt(&mut self, host: &str, path: &str) -> Result<u64, CatcherError> {
        let url = format!("{host}{path}");
        let start = Instant::now();
        let resp = self
            .http_client
            .head(&url)
            .send()
            .await
            .map_err(|e| CatcherError::Internal(format!("HEAD {url}: {e}")))?;

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

    fn record_rtt(&mut self, rtt_ms: u64) {
        if self.sliding_window.len() >= self.window_size {
            self.sliding_window.remove(0);
        }
        self.sliding_window.push(rtt_ms);
        self.dirty = true;
    }

    /// RTT 滑动窗口统计快照（惰性缓存：仅窗口变化时重算）
    pub fn rtt_snapshot(&mut self) -> RttSnapshot {
        if self.sliding_window.is_empty() {
            return RttSnapshot::default();
        }

        if self.dirty {
            let mut sorted = self.sliding_window.clone();
            sorted.sort_unstable();
            let sum: u64 = sorted.iter().sum();
            let avg = sum / sorted.len() as u64;
            let jitter = if sorted.len() > 1 {
                sorted
                    .iter()
                    .map(|&v| (v as i64 - avg as i64).unsigned_abs())
                    .sum::<u64>()
                    / sorted.len() as u64
            } else {
                0
            };
            self.cached_snapshot = Some(RttSnapshot {
                avg_rtt_ms: avg,
                min_rtt_ms: sorted[0],
                max_rtt_ms: sorted[sorted.len() - 1],
                jitter_ms: jitter,
                packet_loss_rate: 0.0,
                sample_count: sorted.len(),
            });
            self.dirty = false;
        }

        self.cached_snapshot.clone().unwrap_or_default()
    }

    /// 综合评估网络质量等级
    pub fn evaluate(&mut self) -> NetworkQualityResult {
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
    if snapshot.sample_count == 0 {
        return NetworkQualityLevel::Bad; // 无数据时保守估计
    }
    let rtt = snapshot.avg_rtt_ms;
    let jitter = snapshot.jitter_ms;
    match (rtt, jitter) {
        (r, _) if r < 80 => NetworkQualityLevel::Excellent,
        (r, _) if r < 200 => NetworkQualityLevel::Good,
        (r, j) if r < 500 && j < 150 => NetworkQualityLevel::Fair,
        (r, _) if r < 1000 => NetworkQualityLevel::Poor,
        _ => NetworkQualityLevel::Bad,
    }
}

/// 质量订阅 — N-04 实时推送
pub struct QualitySubscription {
    cancel_tx: tokio::sync::watch::Sender<bool>,
    _task: tokio::task::JoinHandle<()>,
}

impl QualitySubscription {
    /// 启动后台质量监测任务，每 `interval_ms` 测量一次。
    /// 仅在质量等级变化时触发 callback。
    pub fn start(
        host: String,
        interval_ms: u64,
        callback: catcher_core::EventCallback,
        user_data: usize,
    ) -> Self {
        let (cancel_tx, mut cancel_rx) = tokio::sync::watch::channel(false);
        let mut evaluator = NetworkQualityEvaluator::new(50);
        let host_clone = host.clone();

        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(interval_ms));
            let mut previous_level: Option<NetworkQualityLevel> = None;

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if evaluator.measure_http_rtt(&host_clone, "/").await.is_ok() {
                            let result = evaluator.evaluate();
                            let level = result.level;
                            if previous_level != Some(level) {
                                let trend = match previous_level {
                                    None => "unknown",
                                    Some(prev) => {
                                        if level < prev { "improving" }
                                        else if level > prev { "degrading" }
                                        else { "stable" }
                                    }
                                };
                                let json = serde_json::json!({
                                    "level": format!("{:?}", level),
                                    "previous_level": previous_level.map(|p| format!("{:?}", p)),
                                    "trend": trend,
                                    "avg_rtt_ms": result.avg_rtt_ms,
                                    "jitter_ms": result.jitter_ms,
                                    "sample_count": evaluator.rtt_snapshot().sample_count,
                                }).to_string();
                                let c_event = std::ffi::CString::new("quality_change").unwrap_or_default();
                                let c_json = std::ffi::CString::new(json.replace('\0', "")).unwrap_or_default();
                                let json_len = c_json.as_bytes().len();
                                callback(
                                    c_event.into_raw(),
                                    c_json.into_raw() as *const u8,
                                    json_len,
                                    user_data as *mut std::ffi::c_void,
                                );
                                previous_level = Some(level);
                            }
                        }
                    }
                    _ = cancel_rx.changed() => {
                        if *cancel_rx.borrow() { break; }
                    }
                }
            }
        });

        Self { cancel_tx, _task: task }
    }

    /// 取消订阅，停止后台 task
    pub fn unsubscribe(self) {
        let _ = self.cancel_tx.send(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_excellent_for_low_rtt() {
        let mut eval = NetworkQualityEvaluator::new(10);
        eval.record_rtt(30);
        eval.record_rtt(40);
        eval.record_rtt(50);
        assert_eq!(eval.evaluate().level, NetworkQualityLevel::Excellent);
    }

    #[test]
    fn evaluate_bad_for_high_rtt() {
        let mut eval = NetworkQualityEvaluator::new(10);
        eval.record_rtt(1200);
        eval.record_rtt(1500);
        assert_eq!(eval.evaluate().level, NetworkQualityLevel::Bad);
    }

    #[test]
    fn empty_window_returns_bad() {
        let mut eval = NetworkQualityEvaluator::new(10);
        assert_eq!(eval.evaluate().level, NetworkQualityLevel::Bad);
    }

    #[test]
    fn rtt_snapshot_calculates_stats() {
        let mut eval = NetworkQualityEvaluator::new(10);
        eval.record_rtt(100);
        eval.record_rtt(200);
        eval.record_rtt(300);
        let snap = eval.rtt_snapshot();
        assert_eq!(snap.min_rtt_ms, 100);
        assert_eq!(snap.max_rtt_ms, 300);
        assert!(snap.avg_rtt_ms > 0);
        assert_eq!(snap.sample_count, 3);
    }

    // ── N-04: QualitySubscription tests ──

    #[test]
    fn ns04_trend_computation() {
        // Excellent < Good < Fair < Poor < Bad (ordinal comparison)
        // improving: level < previous_level
        // degrading: level > previous_level
        // stable: level == previous_level
        use std::cmp::Ordering;
        fn compute_trend(level: NetworkQualityLevel, prev: Option<NetworkQualityLevel>) -> &'static str {
            match prev {
                None => "unknown",
                Some(p) => match level.cmp(&p) {
                    Ordering::Less => "improving",
                    Ordering::Greater => "degrading",
                    Ordering::Equal => "stable",
                },
            }
        }
        // Bad > Poor > Fair > Good > Excellent
        assert_eq!(compute_trend(NetworkQualityLevel::Excellent, None), "unknown");
        assert_eq!(compute_trend(NetworkQualityLevel::Good, Some(NetworkQualityLevel::Excellent)), "degrading");
        assert_eq!(compute_trend(NetworkQualityLevel::Poor, Some(NetworkQualityLevel::Bad)), "improving");
        assert_eq!(compute_trend(NetworkQualityLevel::Good, Some(NetworkQualityLevel::Good)), "stable");
        assert_eq!(compute_trend(NetworkQualityLevel::Fair, Some(NetworkQualityLevel::Poor)), "improving");
    }

    #[test]
    fn ns04_classify_quality_levels() {
        let mut eval = NetworkQualityEvaluator::new(10);
        // Excellent: avg_rtt < 80
        eval.record_rtt(30); eval.record_rtt(40); eval.record_rtt(50);
        assert_eq!(eval.evaluate().level, NetworkQualityLevel::Excellent);

        let mut eval = NetworkQualityEvaluator::new(10);
        // Good: avg_rtt < 200
        eval.record_rtt(100); eval.record_rtt(150);
        assert_eq!(eval.evaluate().level, NetworkQualityLevel::Good);

        let mut eval = NetworkQualityEvaluator::new(10);
        // Bad: avg_rtt >= 1000
        eval.record_rtt(1200); eval.record_rtt(1500);
        assert_eq!(eval.evaluate().level, NetworkQualityLevel::Bad);
    }

    #[tokio::test]
    async fn ns04_subscription_starts_and_unsubscribes() {
        extern "C" fn noop_callback(
            _et: *const std::ffi::c_char, _ed: *const u8, _el: usize, _ud: *mut std::ffi::c_void,
        ) {}
        let sub = QualitySubscription::start(
            "http://127.0.0.1:1".to_string(),
            500,
            noop_callback,
            0,
        );
        sub.unsubscribe();
    }

    #[tokio::test]
    async fn ns04_subscription_measurement_failure_no_crash() {
        extern "C" fn noop_callback(
            _et: *const std::ffi::c_char, _ed: *const u8, _el: usize, _ud: *mut std::ffi::c_void,
        ) {}
        let sub = QualitySubscription::start(
            "http://127.0.0.1:1".to_string(),
            100,
            noop_callback,
            0,
        );
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        sub.unsubscribe();
    }

    #[test]
    fn ns04_no_callback_on_same_level() {
        let mut eval = NetworkQualityEvaluator::new(10);
        eval.record_rtt(50);
        let r1 = eval.evaluate();
        eval.record_rtt(60);
        let r2 = eval.evaluate();
        assert_eq!(r1.level, NetworkQualityLevel::Excellent);
        assert_eq!(r2.level, NetworkQualityLevel::Excellent);
    }

    #[tokio::test]
    async fn ns04_multiple_subscribers_independent() {
        extern "C" fn noop_callback(
            _et: *const std::ffi::c_char, _ed: *const u8, _el: usize, _ud: *mut std::ffi::c_void,
        ) {}
        let sub1 = QualitySubscription::start("http://127.0.0.1:1".to_string(), 500, noop_callback, 0);
        let sub2 = QualitySubscription::start("http://127.0.0.1:1".to_string(), 500, noop_callback, 0);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        sub1.unsubscribe();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        sub2.unsubscribe();
    }
}
