use catcher_core::types::observability::RttSnapshot;
use std::time::Duration;

/// 基于 P90 RTT 的自适应超时计算器
///
/// timeout = max(min_timeout, P90_RTT * multiplier)，不超 max_timeout
pub struct AdaptiveTimeout {
    rtt_window: Vec<u64>,
    window_size: usize,
    min_timeout_ms: u64,
    max_timeout_ms: u64,
    multiplier: f64,
    cached_p90: Option<u64>,
    cached_snapshot: Option<RttSnapshot>,
    dirty: bool,
}

impl AdaptiveTimeout {
    pub fn new(
        min_timeout_ms: u64,
        max_timeout_ms: u64,
        multiplier: f64,
        window_size: usize,
    ) -> Self {
        Self {
            rtt_window: Vec::with_capacity(window_size),
            window_size,
            min_timeout_ms,
            max_timeout_ms,
            multiplier,
            cached_p90: None,
            cached_snapshot: None,
            dirty: false,
        }
    }

    /// 记录新 RTT 样本
    pub fn record(&mut self, rtt_ms: u64) {
        if self.rtt_window.len() >= self.window_size {
            self.rtt_window.remove(0);
        }
        self.rtt_window.push(rtt_ms);
        self.dirty = true;
    }

    /// 计算当前应使用的超时（惰性缓存：仅窗口变化时重算）
    pub fn timeout_ms(&mut self) -> u64 {
        if self.rtt_window.is_empty() {
            return self.min_timeout_ms;
        }

        if self.dirty {
            let p90_idx = ((self.rtt_window.len() as f64) * 0.90).ceil() as usize - 1;
            let p90_idx = p90_idx.min(self.rtt_window.len() - 1);
            let mut sorted = self.rtt_window.clone();
            sorted.sort_unstable();
            self.cached_p90 = Some(sorted[p90_idx]);
            self.dirty = false;
        }

        let p90 = self.cached_p90.unwrap_or(self.min_timeout_ms);
        let timeout = (p90 as f64 * self.multiplier) as u64;
        timeout.clamp(self.min_timeout_ms, self.max_timeout_ms)
    }

    /// 计算当前应使用的超时 (Duration)
    pub fn compute(&mut self) -> Duration {
        Duration::from_millis(self.timeout_ms())
    }

    /// 返回滑动窗口快照（惰性缓存：仅窗口变化时重算）
    pub fn snapshot(&mut self) -> RttSnapshot {
        if self.rtt_window.is_empty() {
            return RttSnapshot::default();
        }

        if self.dirty {
            let mut sorted = self.rtt_window.clone();
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
            // 同时更新 cached_p90
            let p90_idx = ((sorted.len() as f64) * 0.90).ceil() as usize - 1;
            let p90_idx = p90_idx.min(sorted.len() - 1);
            self.cached_p90 = Some(sorted[p90_idx]);
            self.dirty = false;
        }

        self.cached_snapshot.clone().unwrap_or_default()
    }

    /// 从快照静态计算超时
    pub fn from_snapshot(snapshot: &RttSnapshot, multiplier: f64) -> Duration {
        let ms = ((snapshot.avg_rtt_ms as f64) * multiplier) as u64;
        Duration::from_millis(ms.max(5000))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_uses_p90() {
        let mut t = AdaptiveTimeout::new(100, 5000, 2.0, 10);
        t.record(100);
        t.record(200);
        t.record(300);
        // P90 of [100,200,300] ≈ 300, *2 = 600
        let ms = t.timeout_ms();
        assert!(ms >= 300);
    }

    #[test]
    fn timeout_clamps_to_min() {
        let mut t = AdaptiveTimeout::new(1000, 5000, 2.0, 10);
        t.record(10); // P90=10, *2=20, clamped to min=1000
        assert_eq!(t.timeout_ms(), 1000);
    }

    #[test]
    fn timeout_clamps_to_max() {
        let mut t = AdaptiveTimeout::new(100, 500, 10.0, 10);
        t.record(1000); // P90=1000, *10=10000, clamped to max=500
        assert_eq!(t.timeout_ms(), 500);
    }

    #[test]
    fn timeout_defaults_to_min_when_empty() {
        let mut t = AdaptiveTimeout::new(1000, 5000, 2.0, 10);
        assert_eq!(t.timeout_ms(), 1000);
    }

    #[test]
    fn record_updates_window() {
        let mut t = AdaptiveTimeout::new(100, 5000, 2.0, 10);
        t.record(100);
        assert_eq!(t.snapshot().sample_count, 1);
        t.record(200);
        assert_eq!(t.snapshot().sample_count, 2);
    }
}
