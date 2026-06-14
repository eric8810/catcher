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

## 移动端代理 / VPN 接入问题

> 来源：客户在 iOS / Android 使用 Clash、VPN、本地代理时的失败反馈；结合 `echoo-flutter` 和 `klip-electron` 实际接入方式审查。

| # | Issue | 优先级 | 状态 | 文件 |
|---|-------|:------:|:----:|------|
| M-01 | Clash / VPN / 本地代理下请求失败 | 🔴 P0 | Catcher 已实现，待验证 | [026-mobile-proxy-vpn-clash.md](./026-mobile-proxy-vpn-clash.md) |
| M-02 | 代理 / VPN / 本地网络兼容范围 | 🔴 P0 | 已调研，待补验证项 | [027-proxy-vpn-network-compatibility-research.md](./027-proxy-vpn-network-compatibility-research.md) |


## PR #13~#15 架构 Review 发现（2026-06-13）

> 来源：对 PR #13（`networkChanged()`）、#14（显式代理）、#15（代理 DNS 行为）的严格架构 review。主线功能在 master 上工作正常且有测试兜底；以下为残留的小 bug 与隐性脆弱点。

| # | Issue | 严重 | 状态 | 修复方案 | 文件 |
|---|-------|:---:|:----:|:------:|------|
| 28 | `tlsSniOverride` 在 Rust 路径静默失效，与 TS 路径行为不一致 | 🟡 Med | ✅ Fixed | 原生路径显式报错 | [028-tls-sni-override-noop-rust-path.md](./028-tls-sni-override-noop-rust-path.md) |
| 29 | `networkPathId` 死字段，从未被读取 | 🟢 Low | ✅ Fixed | 移除字段（Rust + 全绑定） | [029-network-path-id-dead-field.md](./029-network-path-id-dead-field.md) |
| 30 | `networkChanged()` 不恢复在途请求，注释言过其实 | 🟡 Med | ✅ Fixed | 修注释 + 文档化 `+cancelAll()` | [030-network-changed-inflight-requests.md](./030-network-changed-inflight-requests.md) |
| 31 | 代理远端 DNS 正确性依赖 reqwest 未文档化行为，仅测试兜底 | 🟢 Low-Med | ✅ Fixed | 代码注释 + CI/arch 文档加固 | [031-proxy-dns-relies-on-reqwest-internal-behavior.md](./031-proxy-dns-relies-on-reqwest-internal-behavior.md) |
| 33 | `q05_subscribe_multiple` 时序 flake + CI 缺 `--no-fail-fast` 掩盖整套件 | 🟡 Med | ✅ Fixed | 改按生命周期断言 + CI 加 `--no-fail-fast` | [033-quality-test-flake-and-ci-fail-fast.md](./033-quality-test-flake-and-ci-fail-fast.md) |
| 34 | FFI 回调测试 UAF（整套并行 SIGSEGV）+ HTTP/SSE destroy 不抑制在途回调 | 🟡 Med | ✅ Fixed | 测试回调状态 `Box::leak`；HTTP/SSE destroy 加 `cancelled_ids` 与 WS 对齐 | [034-ffi-callback-uaf-test-and-destroy-asymmetry.md](./034-ffi-callback-uaf-test-and-destroy-asymmetry.md) |

**结论**：#28~#31、#33、#34 已全部修复（2026-06-13）。均为局部改动，无连接层重构。验证：`cargo clippy --workspace` 通过、相关单测与 `proxy_dns_behavior_test` 全绿、`quality_test` 5/5 稳定、`http_test` 并行 5/5 无 SIGSEGV、`catcher-ffi` 全套件绿、`pnpm typecheck` 通过；Dart 改动为机械字段移除（本机无 dart 工具链，未跑 analyze）。#34 产品侧已将 HTTP/SSE destroy 与 WS 对齐（destroy 后不再回调 user_data）。

> 排查记录：`connect_stream_uses_dns_host_mapping` 在本机 macOS 沙箱失败，经查 host_mapping 解析正确（连接到达正确 IP:端口），且在 Linux CI 绿色构建（`849187d`）上通过 → 环境特异性，非产品 bug。该测试此前在 CI 未被验证，根因正是 #33 的 fail-fast 截断。

### 由本轮 review 衍生的能力缺口（feature gap，非 bug）

| # | 能力缺口 | 优先级 | 状态 | 方案 | 文件 |
|---|---------|:------:|:----:|------|------|
| 32 | WS 尚未实现 `pin_sha256` 证书固定（HTTP 已支持） | 🟡 P1–P2 | 未实现 | A:类型/文档标注（小）· B:抽取共享 pinning（中） | [032-ws-pin-sha256-not-supported.md](./032-ws-pin-sha256-not-supported.md) |

> #32 是 WS 端的能力尚未实现（fail-closed 显式报错，无安全降级），性质与上方 bug 不同，单列。


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
