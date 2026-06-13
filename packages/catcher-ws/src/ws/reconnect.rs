use crate::types::ws::{ReconnectConfig, WsState};

/// 重连管理器 — 维护重连状态机
///
/// 状态迁移：
/// ```text
/// DISCONNECTED → CONNECTING → (成功) → CONNECTED → (断开) → RECONNECTING
/// RECONNECTING → CONNECTING → ... → (exhausted) → DISCONNECTED
/// ```
pub struct ReconnectManager {
    config: ReconnectConfig,
    state: WsState,
    attempt: u32,
    current_delay_ms: u64,
}

impl ReconnectManager {
    pub fn new(config: ReconnectConfig) -> Self {
        Self {
            config,
            state: WsState::Disconnected,
            attempt: 0,
            current_delay_ms: 0,
        }
    }

    /// 连接尝试开始时调用
    pub fn on_connecting(&mut self) {
        self.state = WsState::Connecting;
    }

    /// 连接失败时调用，返回需要等待的延迟（毫秒）
    /// 返回 None 表示重试次数已耗尽
    pub fn on_disconnect(&mut self) -> Option<u64> {
        self.attempt += 1;
        if self.attempt > self.config.max_attempts {
            self.state = WsState::Disconnected;
            return None;
        }

        self.state = WsState::Reconnecting;

        // 指数退避：initial_delay * backoff_multiplier^(attempt-1)
        if self.attempt == 1 {
            self.current_delay_ms = self.config.initial_delay_ms;
        } else {
            let multiplier = self
                .config
                .backoff_multiplier
                .powi((self.attempt - 1) as i32);
            self.current_delay_ms = (self.config.initial_delay_ms as f64 * multiplier) as u64;
        }

        // clamp to max_delay
        if self.current_delay_ms > self.config.max_delay_ms {
            self.current_delay_ms = self.config.max_delay_ms;
        }

        Some(self.current_delay_ms)
    }

    /// 连接成功时调用，重置状态
    pub fn on_connected(&mut self) {
        self.state = WsState::Connected;
        self.attempt = 0;
        self.current_delay_ms = 0;
    }

    /// 重置重试计数与退避延迟，保留当前状态。
    ///
    /// 网络环境变化（WiFi 切换 / VPN 换节点）时调用：之前的失败是旧网络
    /// 造成的，新网络应该从头开始计数，避免带着累积的退避延迟和已耗尽
    /// 的次数进入新环境。
    pub fn reset(&mut self) {
        self.attempt = 0;
        self.current_delay_ms = 0;
    }

    /// 当前状态
    pub fn state(&self) -> WsState {
        self.state
    }

    /// 是否已耗尽重试次数
    pub fn is_exhausted(&self) -> bool {
        self.state == WsState::Disconnected && self.attempt > 0
    }

    /// 当前建议的延迟（毫秒）
    pub fn delay_ms(&self) -> u64 {
        self.current_delay_ms
    }

    /// 当前重连尝试次数
    pub fn attempt(&self) -> u32 {
        self.attempt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ReconnectConfig {
        ReconnectConfig {
            initial_delay_ms: 500,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
            max_attempts: 5,
        }
    }

    #[test]
    fn first_disconnect_returns_initial_delay() {
        let mut mgr = ReconnectManager::new(test_config());
        let delay = mgr.on_disconnect();
        assert!(delay.is_some());
        assert_eq!(delay.unwrap(), 500);
        assert_eq!(mgr.state(), WsState::Reconnecting);
    }

    #[test]
    fn second_disconnect_doubles_delay() {
        let mut mgr = ReconnectManager::new(test_config());
        mgr.on_disconnect(); // attempt 1
        let delay = mgr.on_disconnect(); // attempt 2
        assert!(delay.is_some());
        assert_eq!(delay.unwrap(), 1000);
    }

    #[test]
    fn delay_clamped_to_max() {
        let mut mgr = ReconnectManager::new(ReconnectConfig {
            initial_delay_ms: 500,
            max_delay_ms: 2000,
            backoff_multiplier: 10.0,
            max_attempts: 5,
        });
        mgr.on_disconnect(); // attempt 1: 500
        let delay = mgr.on_disconnect(); // attempt 2: 5000 → clamped to 2000
        assert_eq!(delay.unwrap(), 2000);
    }

    #[test]
    fn exhausted_after_max_attempts() {
        let mut mgr = ReconnectManager::new(test_config());
        for _ in 0..5 {
            assert!(mgr.on_disconnect().is_some());
        }
        // 第 6 次 = 耗尽
        assert!(mgr.on_disconnect().is_none());
        assert!(mgr.is_exhausted());
    }

    #[test]
    fn on_connected_resets_state() {
        let mut mgr = ReconnectManager::new(test_config());
        mgr.on_disconnect();
        assert_eq!(mgr.state(), WsState::Reconnecting);
        mgr.on_connected();
        assert_eq!(mgr.state(), WsState::Connected);
        assert!(!mgr.is_exhausted());
    }

    #[test]
    fn reset_clears_attempt_and_delay() {
        let mut mgr = ReconnectManager::new(test_config());
        for _ in 0..5 {
            mgr.on_disconnect();
        }
        // 已用尽 5 次，reset 后重新可用且从 initial_delay 开始
        mgr.reset();
        let delay = mgr.on_disconnect();
        assert_eq!(delay, Some(500));
        assert_eq!(mgr.attempt(), 1);
    }

    #[test]
    fn on_connected_after_reconnect_resets_attempt() {
        let mut mgr = ReconnectManager::new(test_config());
        mgr.on_disconnect(); // attempt 1
        mgr.on_disconnect(); // attempt 2
        mgr.on_connected();
        // 重置后第一次断开应该重新从 initial_delay 开始
        let delay = mgr.on_disconnect();
        assert_eq!(delay.unwrap(), 500);
    }
}
