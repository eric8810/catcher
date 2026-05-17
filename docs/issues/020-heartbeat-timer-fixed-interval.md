# Bug: 心跳 timer 间隔固化，自适应心跳仍未生效

**严重程度**: 🟡 Medium — `HeartbeatManager::interval_ms()` 有完整逻辑但从未被调用

**状态**: Open

**位置**: `packages/catcher-ws/src/transport/ws_client.rs:270`

---

## 当前代码

```rust
// 心跳定时器任务 — 每次重连后创建，间隔固定
let (ping_tx, mut ping_rx) = mpsc::unbounded_channel::<()>();
if config.heartbeat.is_some() {
    let interval_ms = config.heartbeat.as_ref().unwrap().interval_ms;  // ← 静态值
    let tx = ping_tx.clone();
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_millis(interval_ms));
        timer.tick().await;
        loop {
            timer.tick().await;
            if tx.send(()).is_err() { break; }
        }
    });
}
```

## 问题

`HeartbeatManager` 完整实现了自适应心跳逻辑：

```rust
// heartbeat.rs
pub fn interval_ms(&mut self) -> u64 {
    if self.config.adaptive {
        self.adaptive_interval()       // ← 基于 P90 RTT 计算
    } else {
        self.config.interval_ms
    }
}

fn adaptive_interval(&mut self) -> u64 {
    if let Some(p90) = self.p90_rtt() {
        let interval = p90.saturating_mul(2);
        interval.max(self.config.interval_ms)
    } else {
        self.config.interval_ms
    }
}
```

但 `interval_ms()` **从未被调用**。timer 在创建时读取 `config.heartbeat.interval_ms` 作为固定间隔，之后永不更新。

**链路故障**：
1. ✅ Ping 发送时记录 `ping_sent_at`（#009 修复）
2. ✅ Pong 收到时计算 RTT → `mgr.on_pong(rtt_ms)`（#009 修复）
3. ✅ `mgr.p90_rtt()` 能正确返回 P90（#003 修复）
4. ❌ **timer 间隔从未更新** — 始终用初始的静态 `interval_ms`
5. ❌ **自适应心跳从未生效** — 即使 RTT 从 10ms 涨到 5000ms，心跳依然按固定间隔发送

## 修复

### 方案 A：动态 reschedule timer

```rust
// 使用 tokio::time::interval 但不固化间隔
// 改为每次 sleep 后检查 HeartbeatManager 建议的间隔
let mgr_interval = state.mgr.interval_ms();
tokio::time::sleep(Duration::from_millis(mgr_interval)).await;
```

### 方案 B：timer channel 携带间隔更新

Pong 处理时通过 channel 通知 timer task 更新间隔。

## 关联

- #009 修复了 RTT 测量，但间隔仍未适配
- #003 修复了 P90 惰性缓存，但 `interval_ms()` 无调用方
- `HeartbeatConfig.adaptive = true` 目前是无效配置
