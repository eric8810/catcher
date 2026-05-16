# Issues — E2E 测试发现的设计问题

> 来源：2026-05-11 端到端性能对比测试，8 个场景 × 多种网络条件，vanilla vs catcher 并发对比

## 问题清单

| # | Issue | 严重 | 症状 | 文件 |
|---|-------|:---:|------|------|
| 1 | Retry 建新连接 | 🔴 | 弱网下 catcher 比 vanilla 慢 4 倍 | [retry-reuses-bad-connection.md](./retry-reuses-bad-connection.md) |
| 2 | keepAlive 无健康检查 | 🔴 | S5/S8 弱网 catcher 成功率低于 vanilla | [keepalive-broken-connection.md](./keepalive-broken-connection.md) |
| 3 | Retry 触发过多 | 🟡 | 轻度弱网不必要的重试放大延迟 | [retry-over-triggers.md](./retry-over-triggers.md) |
| 4 | ~~Circuit breaker 未接入~~ | 🟡 | ✅ 已接入，CB 在 TS 和 Rust 层均正常工作 | [circuit-breaker-not-wired.md](./circuit-breaker-not-wired.md) |
| 5 | Retry 无跨请求记忆 | 🟡 | 连续失败仍从 1s 退避，但 CB 已覆盖此需求 | [retry-no-cross-request-memory.md](./retry-no-cross-request-memory.md) |
| 6 | 代理延迟在连接时固化 | 🔴 | keepAlive 连接跨测试复用导致弱网数据被污染 | [proxy-latency-captured-at-connect.md](./proxy-latency-captured-at-connect.md) |
| 7 | retry minTimeout 偏高 | 🟡 | 退避从 1s 起步，不必要时也白花等待 | [retry-min-timeout-too-high.md](./retry-min-timeout-too-high.md) |
| 8 | S5 大体积消息缺 retry | 🟡 | 弱网下 keepAlive 坏连接无 retry 保护 | [s5-missing-retry.md](./s5-missing-retry.md) |
| 9 | S7 metric 滥用 | 🟡 | msgFinishOrder 当延迟，出现 -2000% 假退化 | [s7-metric-abuse.md](./s7-metric-abuse.md) |
| 10 | chaos parseInt 下划线 | 🟡 | `600_000` → 600ms，混沌测试无效 | [chaos-parseint-underscore.md](./chaos-parseint-underscore.md) |
| 11 | reporter 统计缺陷 | 🟡 | 全失败假改善 + S7 拉低平均 + P95 百分比失真 | [reporter-stat-flaws.md](./reporter-stat-flaws.md) |
| 12 | 延迟对比跨重试次数混算 | 🟡 | 0-retry延迟与1-retry延迟混算P50/P95，retry代价被当退化 | [retry-bucketed-comparison.md](./retry-bucketed-comparison.md) |
| 12 | 延迟未按重试分桶 | 🟡 | catcher 1-retry 成功 vs vanilla 0-retry 成功，不公平 | [latency-buckets-by-retries.md](./latency-buckets-by-retries.md) |

> ⚠️ Issue #6 为测试基础设施 bug，可能导致 #1~#5 的 E2E 证据需要重新评估。

## API 功能补充 Issues

> 来源：[api-gap-analysis](../research/api-gap-analysis.md) 对照 + 代码审查

| # | Issue | 优先级 | 状态 | 文件 |
|---|-------|:------:|:----:|------|
| G1 | ~~请求取消 (AbortSignal)~~ | 🔴 P0 | ✅ 已实现 | — |
| G2 | 错误上下文丰富化 | 🔴 P0 | ✅ | 同上 |
| G3 | CORS / credentials / cookie | 🔴 P0 | ✅ | 同上 |
| G4 | 代理设置 (HTTP/SOCKS5) | 🟡 P1 | ✅ | 同上 |
| G5 | FormData / 文件上传 | 🟡 P1 | 🟡 TS✅ Rust❌ | 同上 |
| G6 | 重定向控制 | 🟡 P1 | 🔲 | 同上 |
| G7 | 自定义 Hostname 解析 | 🟡 P1 | ✅ | 同上 |
| G8 | HTTPS 配置增强 | 🟡 P1 | 🔲 | 同上 |
| G9 | Transport trait (Adapter) | 🟡 P1 | 🔲 | 同上 |
| G10 | 流式响应 | 🟡 P1 | ✅ | 同上 |
| G11 | 韧性运行时控制 | 🟡 P1 | ✅ | 同上 |
| G12 | 认证辅助 | 🟢 P2 | ✅ | 同上 |

详见 → [api-gap-features.md](./api-gap-features.md)

## FFI / 原生层能力缺口

> 来源：对照 `ffi-uniffi-capability-gaps.md` 已修复项目，逐项审查 Rust 原生层 vs TS 层对等能力

| # | Issue | 优先级 | 状态 | 文件 |
|---|-------|:------:|:----:|------|
| N-01 | Multipart/FormData 文件上传 | 🟢 P2 | 📐 设计中 | [native-layer-capability-gaps.md](./native-layer-capability-gaps.md) |
| N-02 | 流式文件下载 (`responseType: stream`) | 🟡 P1 | ✅ 已实现 | 同上 |
| N-03 | 单请求级 cancel（非 `cancelAll`） | 🟡 P1 | ✅ 已实现 | 同上 |
| N-04 | 网络质量实时事件推送 | 🟢 P2 | ✅ 已实现 | 同上 |

详见 → [native-layer-capability-gaps.md](./native-layer-capability-gaps.md)

## 间题之间的关联

```
                 keepAlive 坏连接
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
    retry 复用坏连接   retry 触发过多   无熔断保护
         │             │             │
         └─────────────┼─────────────┘
                       ▼
              请求放大效应
         (catcher 比 vanilla 慢/成功率更低)
                       │
                       ▼
              CB 接入即可解决 ── 不需要退火
```

**核心因果链**：
keepAlive 池中坏连接 → retry 对这个坏连接反复重试 → 重试次数过多放大延迟 → 无 circuit breaker 保护 → 雪崩

## 测试数据支撑

| 指标 | 证据 |
|------|------|
| retry 放大延迟 | S3 🟡弱网: vanilla P50=2s, catcher P50=8s（双方 100% 成功） |
| keepAlive 降低成功率 | S5 🟡弱网: vanilla 80% vs catcher 60% |
| keepAlive 降低成功率 | S8 🟡弱网: vanilla 60% vs catcher 40% |
| circuit breaker 缺失 | 代码审查：`cockatiel` 已安装但未在 HTTP 路径使用 |

## 已验证有效的能力

以上问题不代表 catcher 无效。以下能力在测试中**明确证明有价值**：

| 能力 | 证据 |
|------|------|
| keepAlive 减少连接数 | S1: 连接数 3→1 (-67%) |
| retry 提升极端弱网成功率 | S2 🔴极弱网: 20% → 100% |
| retry 提升偏远地区成功率 | S2 🏔️偏远3G: 80% → 100% |
| DNS 缓存减少重复解析 | DNS 集成测试: 后续请求仅首次的 9% |
| msgpackr 减少带宽 | S5: catcher bytes < vanilla bytes |

## 架构差距审计（2026-05-15）

> 全面对照设计文档 vs 实际源码，覆盖 Rust / TS / Dart / napi / UniFFI

详见 → [arch-gap-audit-2026.md](./arch-gap-audit-2026.md)

### 发现摘要

| 类别 | 数量 | 说明 |
|------|:----:|------|
| A. 代码已实现但未接入管线 | 2 | WS deflate / Transport trait (A-01 已通过 Semaphore 接入, A-03 host_mapping 已接入) |
| B. 设计有方案但代码未开始 | 2 | Transport trait / Multipart (B-03 circuitBreakerChange + networkQualityChange 已补全) |
| C. 文档标记 🔲 但代码实际已完成 | 8+ | **文档严重滞后** — G2/G3/G4/G5/G10/G12 等已完成 |
| D. 已发现但未修复的 Bug | 0 | D-01~05 全部修复 ✅ |
| E. 类型定义存在但从未使用 | 4 | TransportAdapter / beforeRedirect / TLS / DNS nameservers |
| F. 缺失的测试 | 8 | TEST-02~10 均未完成 |
| H. 平台绑定层缺口 | 0 | H-01 roundtrip ✅ (CI已接入), H-02 UniFFI 导出完整 ✅, H-03 napi binary ✅, H-04 stream ✅, H-05 cancel ✅ |
| I. 规划中功能 | 1 | I-01 catcher-tus (I-02 proxy.ts 已完成) |

### 优先修复

1. **🔴 D-01~05**: review-2026 发现的未修复 Bug（每项 1-10 行改动）
2. **🟡 A-01**: PriorityRequestQueue 接入 HttpTransport
3. **🟡 C**: 更新文档状态（G2~G12 大部分已完成）
4. **🟡 A-03**: DNS 自定义解析接入 reqwest
5. **🟡 B-03**: 韧性事件推送补全

### ⚠️ 重要更正

Issue #4 "Circuit breaker 未接入" 和 api-gap-features.md 中 G2/G3/G4/G5/G10/G12 标记为 🔲 的功能，经 2026-05-15 源码审查确认**已在代码中实现**。详见 [arch-gap-audit-2026.md](./arch-gap-audit-2026.md) 第四部分。
