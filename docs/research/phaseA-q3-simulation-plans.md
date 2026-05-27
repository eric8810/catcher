# Phase A — Q3 模拟方案完整设计

> 为 10 个 P0 缺口设计 proxy.ts 模拟方案 + 保真度校验方法

---

## Q3 模拟方案矩阵

| # | P0 缺口 | 模拟方案 | 保真度校验 |
|---|---------|---------|-----------|
| 1 | DNS SERVFAIL | proxy.ts DNS mock: 返回 SERVFAIL 5%概率 | 对照 RIPE Atlas DNS 故障率数据 |
| 2 | HTTP 408 | proxy.ts: 连接空闲 N 秒后 RST，然后新连接上 408 返回 | 对照 nginx 75s keepalive_timeout 行为 |
| 3 | HTTP 429 | proxy.ts: 注入 429 + Retry-After header | 对照真实 API 的 429 频率分布 |
| 4 | Retry budget | proxy.ts: 多客户端并发测试，注入持续 5xx → 观察 token bucket 效果 | 对照 AWS SDK token bucket 行为 |
| 5 | connect_timeout | proxy.ts: SYN 丢弃 100% → 测量 Catcher 实际超时 | 对照 OS tcp_syn_retries 默认行为 |
| 6 | Doze/iOS | proxy.ts: 模拟长时间 IDLE → 断开→ 恢复 → 测量重连时间 | 对照 Android Doze 文档描述的恢复行为 |
| 7 | GEO退避 | proxy.ts: 固定 600ms RTT，max_attempts=3 → 测量总等待时间 | 对照理论计算: 100+200+400+...+10000ms |
| 8 | H2 GOAWAY | proxy.ts: H2 server 发送单次 GOAWAY → 验证 Catcher 正确处理 | 对照 RFC 7540 §6.8 要求 |
| 9 | Starlink CB | proxy.ts: 15s 周期 delay spike + 持续请求 → 测量 CB 误触发率 | 对照 Starlink WetLinks 实测 RTT 数据 |
| 10 | TLS 425 | proxy.ts: TLS server 拒绝 0-RTT → 返回 425 → 验证自动重试 | 对照 RFC 8470 要求 |

---

## 保真度校验方法论（对标 Fidelity agent 3 层模型）

### Tier 1: 单元级（损伤参数精度）
- 方法: 硬件时间戳打点，对比 proxy.ts 注入延迟 vs 实测延迟
- 指标: MAE(延迟), RMSE(丢包率), σ error(抖动)
- 对标: tc netem 350µs 基线

### Tier 2: 统计级（分布保真度）
- 方法: KS test + Q-Q plot，对比 proxy.ts 输出 vs 真实 trace (MAWI/RIPE Atlas)
- 指标: KS statistic, Wasserstein distance, percentile error (p50/p95/p99)

### Tier 3: 应用级（端到端效果）
- 方法: 相同 impairment 下，对比 Catcher 通过 proxy.ts vs 真实网络的行为
- 指标: 请求成功率偏差、P99 延迟偏差、CB 触发一致性
