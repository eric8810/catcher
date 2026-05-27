# Catcher 网络韧性调研 — 持续进展看板

> 框架 v3 · 10+ 轮迭代 · 持续更新
> 最后更新：2026-05

---

## 调研进度

| 迭代 | 状态 | 文件 | 关键发现数 |
|:----:|:----:|------|:--------:|
| 1 | ✅ | phase0-discovery-report.md | 15+ 数据源 |
| 1 | ✅ | phase1-orthogonal-matrix.md | 49 故障, 7 P0 |
| 2 | ✅ | iteration2-reachability-deepdive.md | BGP ~230/天, DNS SERVFAIL 1%, H2 GOAWAY bug |
| 3 | ✅ | iteration3-time-faults.md | Bufferbloat +118ms, Doze 2h, 5G <7ms OWL |
| 9 | ✅ | iteration9-identity-policy.md | TLS 88%自签名, 429 Retry-After 分布 |
| 10 | ✅ | iteration10-starlink.md | Starlink 15s周期性, TCP hostile |
| 7 | ✅ | iteration7-final-closure.md | Q0-Q4 闭环追溯表 |
| 8 | ⏳ | Fidelity agent running | 模拟保真度方法论 |
| 11 | ⏳ | Integrity agent running | 丢包/损坏/乱序数据 |
| 12 | ⏳ | RealTrace agent running | 真实trace数据集 |
| 13 | ⏳ | Academia agent running | 学术论文最新研究 |

---

## 累计数据资产

### 外部权威数据源（20+）

| 来源 | 关键数据 |
|------|---------|
| Cloudflare Radar 2024/2025 | 20% TCP异常终止, loaded latency p75 78.6ms |
| Cloudflare TCP Reset (SIGCOMM 2023) | 19 种篡改签名, Post-SYN/ACK/PSH 细分 |
| Cloudflare 1.1.1.1 Postmortem (2024) | BGP 劫持影响 300+ 网络, 70 国 |
| Cloudflare Orpheus | 路径不可达的普遍性验证 |
| AWS SDK Retry (2026) | 瞬态50ms / 节流1000ms 区分 |
| AWS Builders Library | 超时+重试+jitter 最佳实践 |
| Google SRE Book Ch.21-22 | Retry budget, 级联故障, 自适应节流 |
| MANRS Observatory | Q1 2022: 18,000+ 劫持, 3,000+ 泄露 |
| isbgpsafeyet.com | "No." — BGP 仍然不安全 |
| Hubble (NSDI 2008) | 中位可达性事件 2.75h |
| Rohde & Schwarz 5G 实测 | 5G <7ms OWL, 移动异常值 >100ms |
| Geoff Huston Starlink TCP | 15s 周期性抖动, "TCP 异常不友好" |
| APNIC DNS TTL 研究 | 50% TTL ≤ 60s, 75% ≤ 300s |
| RFC 9520 (2023) | DNS 负缓存新标准 |
| Akamai connect timeout | 5s 默认 |
| Discord Postmortem (2026) | 17% sessions 同时断开 → 重连风暴 |
| IMC 2016 TLS 研究 | 88% 无效证书为自签名 |
| 竞品 Bug (curl/hyper/reqwest/OkHttp) | 9 模式 |
| 生产 Postmortem (Meta/AWS/GitHub/Stripe 等) | 7 案例 |

### 累积 P0 缺口

| # | 缺口 | Q0-Q4 状态 |
|---|------|:--------:|
| 1 | DNS SERVFAIL 区分 | Q0✅ Q1✅ Q2✅ Q3⚠️ Q4⬜ |
| 2 | HTTP 408 Retryable | Q0✅ Q1✅ Q2✅ Q3⚠️ Q4⬜ |
| 3 | HTTP 429 Retry-After | Q0✅ Q1✅ Q2✅ Q3⚠️ Q4⬜ |
| 4 | Retry budget | Q0✅ Q1✅ Q2✅ Q3❌ Q4⬜ |
| 5 | connect_timeout=15s | Q0✅ Q1✅ Q2✅ Q3⚠️ Q4⬜ |
| 6 | Doze/iOS 退避 | Q0⚠️ Q1✅ Q2✅ Q3❌ Q4⬜ |
| 7 | GEO 退避 RTT 联动 | Q0✅ Q1✅ Q2✅ Q3✅ Q4⬜ |
| 8 | Starlink 15s 周期性 CB 误触发 | Q0✅ Q1✅ Q2✅ Q3❌ Q4⬜ |
| 9 | TLS 425 Too Early | Q0✅ Q1✅ Q2✅ Q3⚠️ Q4⬜ |
| 10 | H2 GOAWAY nginx 单次 | Q0✅ Q1✅ Q2✅ Q3❌ Q4⬜ |

> 待 4 个 agent 完成后更新
