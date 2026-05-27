# Phase E — 定量模型补全

> 已有 6 项模型 (QUANTITATIVE-ANALYSIS.md)，补全剩余可量化场景

---

## E7. Starlink CB 误触发率

```
给定:
  - Starlink 周期 T = 15s
  - RTT 突增持续 D = 2s/周期
  - CB failure_threshold = 5
  - 请求速率 R req/s

CB 在周期内触发条件: 在 D=2s 内发送 ≥5 个请求 → R ≥ 2.5 req/s

P(误触发) = 0                            当 R < 2.5 req/s
P(误触发) = 1                            当 R ≥ 2.5 req/s 且请求集中在突增窗口

有效缓解: min_failure_window_ms ≥ 30,000 (覆盖 2 个完整周期)
         → 需要 2 个连续突增窗口内全部失败才触发 CB
         → P(误触发) → 接近 0
```

---

## E8. HTTP/2 连接复用 vs 独立连接可靠性

```
场景: 100 个并发请求

方案 A: 全部走 1 个 H2 连接
  连接故障率: P(conn_fail) = 20% (Cloudflare 数据)
  所有请求失败概率: 20%

方案 B: 分到 4 个 H2 连接
  P(全部失败) = 0.20⁴ = 0.16%
  至少 1 个存活概率: 99.84%

→ H2 连接复用是双刃剑: 性能好但单点故障风险高
→ Catcher 应在高可靠性场景下限制单连接请求数
```

---

## E9. keepAlive 最优间隔推导

```
给定:
  - CGNAT TCP idle timeout T_nat ∈ [60, 120]s
  - 服务器 keepalive_timeout T_srv ∈ [5, 75]s (Apache 5s, nginx 75s)
  - 移动端电量成本 per keepalive: E_ping

约束:
  keepAlive_interval < min(T_nat, T_srv) - RTT  (防 race condition)
  keepAlive_interval > RTT × 2                 (给 PING/PONG 往返留时间)

对于典型最坏情况:
  T_nat = 60s, T_srv = 65s (nginx), RTT = 200ms
  → keepAlive_interval < min(60, 65) - 0.2 = 59.8s
  → keepAlive_interval > 0.4s
  → 最优区间: [1s, 55s], 默认 30s 处于区间中心 ✅
```

---

## E10. 多 endpoint 竞速可靠性提升

```
给定:
  - 单 endpoint 故障率 P(fail) = 20%
  - N 个独立 endpoint

P(全部故障) = 0.20^N

N=1: 80% 成功率
N=2: 96% 成功率 (1 - 0.2²)
N=3: 99.2% 成功率
N=4: 99.84% 成功率

→ 3 个独立 endpoint 即可将成功率从 80% 提升到 99.2%
→ 但前提是 endpoint 真正独立 (不同 AS, 不同 region)
→ 同一 CDN 的多个 POP 不算独立 (Fastly 2021 全球故障)
```

---

## E11. 重试超时预算 (time-budget vs count-based)

```
当前: max_attempts=3, max_backoff=10,000ms
  总等待 ≈ 100 + 200 + 400 + ... = ~22s (达到封顶前)

问题: GEO 卫星 RTT=600ms 下，TCP 层需要数分钟
  应用层 22s 放弃 vs TCP 层仍在重传 → 浪费

建议: time-budget 模式
  total_timeout = max(RTT_p90 × 10, 60,000ms)
  在 total_timeout 内不限次数重试，而非固定 3 次

GEO 卫星: total_timeout = max(600 × 10, 60000) = 60,000ms
  → 约 100 次重试机会 (60,000 / 600)
  → 远超当前 3 次限制
```
