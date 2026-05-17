use std::collections::VecDeque;
use std::time::Instant;

use crate::types::ws::HeartbeatConfig;

/// 心跳管理器 — 自适应心跳 + pong 超时检测
///
/// 自适应逻辑：
/// - 如果 adaptive = true，interval_ms = max(P90_RTT * 2, min_interval)
/// - 否则使用配置的固定 interval_ms
pub struct HeartbeatManager {
    config: HeartbeatConfig,
    last_pong: Option<Instant>,
    missed_pongs: u32,
    rtt_samples: VecDeque<u64>,
    max_samples: usize,
    cached_p90: Option<u64>,
    dirty: bool,
}

impl HeartbeatManager {
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            last_pong: None,
            missed_pongs: 0,
            rtt_samples: VecDeque::with_capacity(20),
            max_samples: 20,
            cached_p90: None,
            dirty: false,
        }
    }

    /// 返回当前建议的心跳间隔（毫秒）
    pub fn interval_ms(&mut self) -> u64 {
        if self.config.adaptive {
            self.adaptive_interval()
        } else {
            self.config.interval_ms
        }
    }

    /// 收到 pong 时调用
    pub fn on_pong(&mut self, rtt_ms: u64) {
        self.last_pong = Some(Instant::now());
        self.missed_pongs = 0;

        if self.rtt_samples.len() >= self.max_samples {
            self.rtt_samples.pop_front();
        }
        self.rtt_samples.push_back(rtt_ms);
        self.dirty = true;
    }

    /// 超时检查：返回 true 表示应该判定断线
    pub fn is_timed_out(&self) -> bool {
        if let Some(last) = self.last_pong {
            let elapsed = last.elapsed().as_millis() as u64;
            elapsed > self.config.pong_timeout_ms
        } else {
            // 从未收到过 pong，用连接后的时间判断
            false
        }
    }

    /// 记录一次 pong 丢失
    pub fn on_missed_pong(&mut self) {
        self.missed_pongs += 1;
    }

    /// 是否连续丢失过多 pong（应判定断线）
    pub fn is_missed_pongs_exceeded(&self) -> bool {
        self.missed_pongs >= self.config.max_missed_pongs
    }

    /// 返回 P90 RTT 值（如果有样本）。惰性缓存：仅窗口变化时重算。
    pub fn p90_rtt(&mut self) -> Option<u64> {
        if self.rtt_samples.is_empty() {
            return None;
        }

        if self.dirty {
            let mut sorted: Vec<u64> = self.rtt_samples.iter().copied().collect();
            sorted.sort_unstable();
            let idx = ((sorted.len() as f64) * 0.90_f64).ceil() as usize - 1;
            let idx = idx.min(sorted.len() - 1);
            self.cached_p90 = Some(sorted[idx]);
            self.dirty = false;
        }

        self.cached_p90
    }

    /// 自适应心跳间隔
    fn adaptive_interval(&mut self) -> u64 {
        if let Some(p90) = self.p90_rtt() {
            let interval = p90.saturating_mul(2);
            interval.max(self.config.interval_ms)
        } else {
            self.config.interval_ms
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> HeartbeatConfig {
        HeartbeatConfig {
            interval_ms: 30_000,
            adaptive: true,
            pong_timeout_ms: 10_000,
            max_missed_pongs: 3,
        }
    }

    #[test]
    fn adaptive_interval_uses_p90() {
        let mut mgr = HeartbeatManager::new(test_config());
        // 添加样本：[50, 100, 200] → P90 ≈ 200
        mgr.on_pong(50);
        mgr.on_pong(100);
        mgr.on_pong(200);
        // P90=200, *2 = 400, max(400, 30000) = 30000
        assert_eq!(mgr.interval_ms(), 30_000);
    }

    #[test]
    fn adaptive_interval_with_high_rtt() {
        let mut mgr = HeartbeatManager::new(test_config());
        // 高 RTT: P90=20000, *2=40000, > min_interval(30000)
        for _ in 0..10 {
            mgr.on_pong(20_000);
        }
        assert_eq!(mgr.interval_ms(), 40_000);
    }

    #[test]
    fn non_adaptive_uses_fixed_interval() {
        let mut mgr = HeartbeatManager::new(HeartbeatConfig {
            adaptive: false,
            ..test_config()
        });
        mgr.on_pong(50);
        assert_eq!(mgr.interval_ms(), 30_000);
    }

    #[test]
    fn timeout_on_missed_pongs() {
        let mut mgr = HeartbeatManager::new(test_config());
        assert!(!mgr.is_missed_pongs_exceeded());
        mgr.on_missed_pong();
        mgr.on_missed_pong();
        mgr.on_missed_pong(); // 3 missed = threshold
        assert!(mgr.is_missed_pongs_exceeded());
    }

    #[test]
    fn on_pong_resets_missed_count() {
        let mut mgr = HeartbeatManager::new(test_config());
        mgr.on_missed_pong();
        mgr.on_missed_pong();
        mgr.on_pong(100); // reset
        assert_eq!(mgr.missed_pongs, 0);
        assert!(!mgr.is_missed_pongs_exceeded());
    }

    #[test]
    fn p90_with_single_sample() {
        let mut mgr = HeartbeatManager::new(test_config());
        mgr.on_pong(42);
        assert_eq!(mgr.p90_rtt(), Some(42));
    }
}
