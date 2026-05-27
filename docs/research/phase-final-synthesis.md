# Catcher 网络韧性调研 — 最终综合报告

> 框架 v3 · 完整五环节循环
> 调研日期：2026-05
> 基于：Phase 0 发现报告 + Phase 1 正交矩阵 + 已有 standards/simulation/expandation 调研

---

## 执行摘要

本次调研按照框架 v3 的五环节循环模型执行，完成了完整的①发现→②分类→③溯源→④模拟→⑤验证→回到①的闭环。

### 核心数字

| 指标 | 数值 |
|------|:----:|
| Phase 0 数据源 | 15+ 个权威来源（Cloudflare、Google SRE、AWS、ACM SIGCOMM、各公司 Postmortem） |
| 识别故障模式 | 49 个（5 故障本质 × 5 网络位置 = 25 交叉格，共 49 项具体故障） |
| Catcher 已覆盖 | 11 项 (22%) |
| 部分覆盖 | 12 项 (24%) |
| 未覆盖 | 26 项 (53%) |
| P0 优先级缺口 | 7 项 |
| 竞品 Bug 模式 | 9 项（curl/reqwest/hyper/OkHttp） |
| 真实 Postmortem 案例 | 7 项（Meta/AWS/GitHub/Stripe/Fastly/Cloudflare/Slack） |
| 极端组合场景 | 7 项（从第一性原理推导） |

### 最重要的 3 个发现

1. **全球 20% 的 TCP 连接在数据交换前就终止了**（Cloudflare 测量）。连接失败是常态，不是异常。Catcher 的设计哲学应从"处理偶发故障"转变为"在故障常态中运行"。

2. **当前统一指数退避策略是错误的**。AWS 的经验表明瞬态错误（连接重置、DNS 瞬败）应快速重试（~50ms），而节流错误（429）应慢速重试（~1000ms）。Google SRE 强调必须有 retry budget 和全局 jitter 防止重试风暴。

3. **移动端不是边缘场景**。全球 41.3% 流量来自移动设备，且移动端有最极端的物理约束（Android Doze 2h 窗口、iOS 30s 后台、CGNAT 60s 超时）。当前 Catcher 的退避策略完全未感知平台约束。

---

## 一、调研循环执行记录

### 第 1 轮循环

```
①发现 → ②分类 → ③溯源检查 → ④模拟检查 → ⑤缺口评估 → 回到①
```

### ① 发现（Phase 0）

**产出**：`phase0-discovery-report.md`

**5 条发现路径**：

| 路径 | 方法 | 关键产出 |
|------|------|---------|
| 真实测量数据 | Cloudflare Radar 2024/2025、TCP Reset 研究、Starlink 实测 | 20% TCP 失败率、loaded latency p75 78.6ms、Starlink p99<65ms |
| Postmortem 挖掘 | 7 家公司公开 Postmortem 分析 | DNS SERVFAIL、BGP 黑洞、Retry Storm、Gray Failure、CDN 503 |
| 竞品 Bug 分析 | curl/reqwest/hyper/OkHttp GitHub Issues | 9 种 Bug 模式：H2 流耗尽、连接池返回死连接、错误分类遗漏等 |
| 行业方法论 | Google SRE Book、AWS Builders Library、AWS SDK Retry | 重试预算、自适应节流、瞬态 vs 节流区分、jitter 必要性 |
| 物理约束推导 | 第一性原理 + 代码分析 | 7 个极端组合场景 (E1-E7) |

### ② 分类（Phase 1）

**产出**：`phase1-orthogonal-matrix.md`

**分类体系**：5 种故障本质（时间/完整度/可达性/身份/策略）× 5 层网络位置（L1物理/L2拓扑/L3中间件/L4协议/L5环境）= 25 交叉格，共计 49 项具体故障。

**覆盖分布**：22% 已覆盖，24% 部分覆盖，53% 未覆盖。

### ③ 溯源（Phase 2 检查）

**已有产出**（v2 框架期间完成）：
- `standards/cellular-3gpp.md` ✅
- `standards/wifi-ieee80211.md` ✅
- `standards/protocol-behaviors.md` ✅
- `standards/os-hardware-quirks.md` ✅

**本轮新发现的缺失**：
- 所有 Profile 参数基于**标准设计值**（3GPP 理论值），缺少**实测统计分布**对标（Cloudflare Radar、CrUX、OpenSignal）
- `satellite-itu.md` 和 `wired-itu-ieee.md` 仍未产出
- 7 个 P0 缺口中 3 个涉及标准溯源不足（408、425、429 RFC 合规）

### ④ 模拟（Phase 3 检查）

**已有产出**：`simulation/tools-benchmark.md` ✅

**本轮新发现的缺失**：
- proxy.ts 从未与真实网络 trace 做过**保真度校验**（无法回答"模拟和真实差多少"）
- Phase 1 矩阵中 49 项故障，proxy.ts 仅能模拟约 18 项（37%）
- 关键缺失：BGP 路由黑洞快速检测、中间件 RST 注入、H2 GOAWAY、CGNAT rebinding

### ⑤ 验证（缺口评估）

7 个 P0 缺口（按影响面和修复代价排序）：

| # | 缺口 | 影响 | 代价 |
|---|------|------|:--:|
| 1 | DNS SERVFAIL vs NXDOMAIN 不区分 | 解析器故障时无法切换到备用 DNS | 中 |
| 2 | HTTP 408 归类为 NonRetryable | keepAlive race 场景下永久失败 | 低 |
| 3 | HTTP 429 Retry-After 全仓零命中 | 被限流后无法正确退避 | 中 |
| 4 | 无 retry budget / 全局限流 | 大规模重试风暴风险 | 高 |
| 5 | BGP 路由黑洞依赖 OS 127s 超时 | 全网中断时 2 分钟+ 才感知 | 中 |
| 6 | Android Doze / iOS 后台退避不感知 | 移动端长连接不可恢复 | 高 |
| 7 | GEO 卫星退避封顶 10s 不够 | 高延迟下过早放弃 | 低 |

---

## 二、全量调研产出清单

```
docs/research/
├── network-testing-verification-framework.md    ← 总纲 v3
├── phase0-discovery-report.md                   ← 🆕 环节①发现
├── phase1-orthogonal-matrix.md                  ← 🆕 环节②分类
│
├── exploratory/
│   └── industry-methodology-survey.md
│
├── standards/
│   ├── cellular-3gpp.md
│   ├── wifi-ieee80211.md
│   ├── protocol-behaviors.md
│   └── os-hardware-quirks.md
│
├── simulation/
│   └── tools-benchmark.md
│
└── expandation/
    ├── 00-summary.md
    ├── 01-protocols.md
    ├── 02-network-env.md
    ├── 03-hardware.md
    ├── 04-software-env.md
    ├── 05-user-interaction.md
    ├── 06-security.md
    └── handoff.md
```

---

## 三、循环反馈：推翻的假设与修正的分类

### 假设 1：retry 应该统一使用指数退避
**推翻**：AWS 的经验表明应按错误类型区分——瞬态错误快速重试 (50ms)，节流错误慢速重试 (1000ms)。
**修正**：在正交矩阵中，"策略故障 × L3 中间件"（429）与"可达性故障 × L4 协议"（连接重置）应使用不同退避策略。

### 假设 2：连接失败是罕见事件
**推翻**：全球 20% TCP 连接在数据交换前终止。加上 5xx 错误、DNS 失败、超时等，总体请求失败率远高于直觉。
**修正**：Catcher 的设计应从"兜底保护"转向"在故障常态中运行"。

### 假设 3：Profile 按网络技术分类就够了
**推翻**：游戏行业按**使用场景**分类（桌面 vs 移动端），不同行业的"韧性"定义互相冲突。
**修正**：正交矩阵中引入"策略故障"作为独立维度——不同应用类型的策略不同（FPS 宁可丢包，API 宁可慢）。

### 假设 4：超时策略可以独立于 RTT
**推翻**：E3 场景显示 max_backoff=10s + max_attempts=3 → 22s 总等待，而 TCP 层在同等 RTT 下需要数分钟。
**修正**：退避参数应与 RTT 联动——`max_backoff ≥ RTT_p90 × 4`。

---

## 四、下一步行动建议

### 立即（0-2 周）

| 行动 | 对应缺口 | 改动量 |
|------|:------:|:-----:|
| `ErrorCategory::category()` 将 408 归为 Retryable | #2 | 1 行 |
| `RetryConfig` 增加 `respect_retry_after: bool` + 解析逻辑 | #3 | ~30 行 |
| `DnsError` 区分 NXDOMAIN (NonRetryable) 和 SERVFAIL (Retryable) | #1 | ~20 行 |
| 退避基线按错误类型区分 | #1, #3 | ~50 行 |

### 短期（2-4 周）

| 行动 | 对应缺口 | 改动量 |
|------|:------:|:-----:|
| 引入 retry budget（token bucket，默认 500 tokens） | #4 | ~100 行 |
| `RetryConfig.max_backoff_ms` 下限设为 RTT_p90 × 4 | #7 | ~10 行 |
| 连接超时独立于 OS 默认（`tcp_syn_retries`），默认 15s | #5 | ~15 行 |

### 中期（1-2 月）

| 行动 | 对应缺口 | 改动量 |
|------|:------:|:-----:|
| 移动端平台感知退避（`DozeDetector` / `NetworkCallback`） | #6 | 高 |
| proxy.ts 保真度校验（vs 真实网络 trace） | Phase 3 | 高 |
| Phase 4 协议合规矩阵（408/425/429/GOAWAY） | Phase 4 | 中 |

---

## 五、框架 v3 方法论验证

本次调研验证了框架 v3 的核心改进：

1. **循环模型有效**：Phase 0 发现 → Phase 1 分类 → Phase 2-4 溯源/模拟/合规检查 → 发现新的缺口 → 回到分类修正。实际执行中确实出现了"验证结果推翻分类假设"的情况（如假设 1-4 被推翻）。

2. **故障本质分类优于网络层次分类**：正交矩阵的 25 个交叉格产生了 49 项具体故障，而旧的 L1-L5 树形分类只能产生约 20 项。关键差异在于"策略故障 × L3"（如连接篡改、429 限流）和"可达性故障 × L5"（如端口耗尽）这些跨层故障在旧体系中会被遗漏。

3. **Phase 0 是最大杠杆点**：本轮新增的 7 个 P0 缺口中，5 个来自 Phase 0 的外部数据源（Cloudflare、AWS、Google SRE、Postmortem），仅 2 个来自内部代码审计。这验证了"发现未知"机制的必要性。

4. **仍需加强**：①真实测量数据的系统化收集（Cloudflare Radar API、CrUX BigQuery）；②proxy.ts 保真度校验（需要搭建真实网络 trace vs 模拟的统计对比 pipeline）；③协议合规矩阵的完整产出。
