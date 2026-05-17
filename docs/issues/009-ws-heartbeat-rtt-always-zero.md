# Bug: WebSocket 心跳 RTT 始终为 0，自适应心跳未生效

**严重程度**: 🟡 Medium — 功能缺陷：HeartbeatManager 维护了 `rtt_samples` 但从无有效数据

**状态**: Open

**位置**: `packages/catcher-ws/src/transport/ws_client.rs:362-367`

---

## 当前代码

```rust
// ws_client.rs — connection_manager select loop
Some(Ok(tokio_tungstenite::tungstenite::Message::Pong(_))) => {
    if let Some(ref mut state) = hb_state {
        state.mgr.on_pong(0);          // ← rtt_ms 硬编码 0
        state.waiting_for_pong = false;
    }
    let _ = event_tx.send(WsEvent::HeartbeatRtt { rtt_ms: 0 });  // ← 始终为 0
}
```

## 问题

收到 Pong 时传入 `rtt_ms = 0`：

1. `HeartbeatManager.on_pong(0)` 将 0 推入 `rtt_samples`，P90 始终为 0
2. `adaptive_interval()` 使用 `p90.saturating_mul(2)` → `0 * 2 = 0` → `max(0, config.interval_ms)` → 固定间隔
3. 自适应心跳功能**实际上从未生效**——始终退化为固定间隔
4. `WsEvent::HeartbeatRtt { rtt_ms: 0 }` 对外暴露的 RTT 始终为 0，调用方无法观测真实延迟

## 修复

在 Ping 发送时记录时间戳，Pong 收到时计算 RTT：

```rust
// HeartbeatState 增加字段
struct HeartbeatState {
    mgr: HeartbeatManager,
    waiting_for_pong: bool,
    ping_sent_at: Option<Instant>,   // ← 新增
}

// Ping 发送时
state.ping_sent_at = Some(Instant::now());
let _ = writer.send(
    tokio_tungstenite::tungstenite::Message::Ping(Vec::new().into()),
).await;

// Pong 收到时
Some(Ok(Message::Pong(_))) => {
    if let Some(ref mut state) = hb_state {
        let rtt_ms = state.ping_sent_at
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);
        state.mgr.on_pong(rtt_ms);
        state.waiting_for_pong = false;
        state.ping_sent_at = None;
        let _ = event_tx.send(WsEvent::HeartbeatRtt { rtt_ms });
    }
}
```

## 影响

- 自适应心跳功能完全失效 — `HeartbeatManager.adaptive_interval()` 逻辑正确但数据源错误
- 调用方收不到有效 RTT 数据
- `HeartbeatManager::p90_rtt()` 返回 `Some(0)`，`adaptive_interval()` 退化为 `interval_ms.max(0)` = `interval_ms`
