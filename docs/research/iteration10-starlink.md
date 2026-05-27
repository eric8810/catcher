# Iteration 10 — Starlink & 卫星网络 TCP 行为深度

> 框架 v3 · 第 10 轮循环
> 来源：Geoff Huston (APNIC) Starlink TCP 分析, 2024-05

---

## 核心发现：Starlink = "对 TCP 异常不友好的环境"

### 物理约束
- 轨道高度：550km
- 单向信号延迟：3.7ms（最小）
- RTT：25-65ms 典型，p99 <65ms
- 卫星速度：27,000 km/h（5 分钟内横跨天空）
- 信噪比持续变化（相位阵列天线，非抛物面）

### 15 秒周期性抖动 —— Catcher 的关键挑战

```
每 15 秒：卫星切换（handover）
  → RTT 短暂突增
  → 丢包率突增
  → 抖动在 15s 间隔内"相对较高"
```

**对 Catcher 的决定性影响**：
1. **CB（熔断器）误触发风险**：15s 周期的 RTT 突增会让 CB 以为"连接持续失败"→ 错误熔断
2. **退避同步风险**：如果 Catcher 的退避周期恰好与 15s 对其，会导致持续的"退避 → 重试 → 恰好遇到 handover → 再退避"的死循环
3. **jitter 不再是可选的**：对于 Starlink 用户，jitter 是**必须**的，且必须与 15s 周期脱钩

### BBR vs CUBIC

Huston 发现：TCP BBR 在 Starlink 上显著优于 CUBIC，因为 BBR 不将周期性延迟突增误解为拥塞。Catcher 的应用层可以在 `connect()` 前设置 `TCP_CONGESTION=bbr`（Linux）。

---

## Catcher 行动项

| # | 行动 | 依据 |
|---|------|------|
| S-1 | **Jitter 默认开启**（±25%），不可关闭 | Starlink 15s 周期 + Google SRE 要求 |
| S-2 | **CB time_window ≥ 60s**（覆盖 4 个 Starlink handover 周期） | 防止 15s 周期性抖动误触发 CB |
| S-3 | **`max_backoff` ≥ 30s** 用于高 RTT 场景 | 防止退避封顶导致与 handover 同步 |
| S-4 | **透明探测**：检测到周期性 RTT 模式时，通知应用层而非硬失败 | Starlink/GEO 用户不应被视为"弱网" |
| S-5 | 文档警告：Starlink 场景下 TCP 性能可能显著低于预期 | 管理用户预期 |
