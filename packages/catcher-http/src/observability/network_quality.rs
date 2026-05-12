use std::time::Instant;

use catcher_core::CatcherError;
use catcher_core::types::observability::*;

/// 网络质量评估器：HTTP HEAD RTT + 滑动窗口 + 综合评分
pub struct NetworkQualityEvaluator {
    http_client: reqwest::Client,
    sliding_window: Vec<u64>,
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
            sliding_window: Vec::with_capacity(window_size),
            window_size,
            connection_type: ConnectionType::Unknown,
        }
    }

    /// 带已有 client 创建（复用连接池）
    pub fn with_client(client: reqwest::Client, window_size: usize) -> Self {
        Self {
            http_client: client,
            sliding_window: Vec::with_capacity(window_size),
            window_size,
            connection_type: ConnectionType::Unknown,
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
    }

    /// RTT 滑动窗口统计快照
    pub fn rtt_snapshot(&self) -> RttSnapshot {
        if self.sliding_window.is_empty() {
            return RttSnapshot::default();
        }
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
        RttSnapshot {
            avg_rtt_ms: avg,
            min_rtt_ms: sorted[0],
            max_rtt_ms: sorted[sorted.len() - 1],
            jitter_ms: jitter,
            packet_loss_rate: 0.0,
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
        let eval = NetworkQualityEvaluator::new(10);
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
}
