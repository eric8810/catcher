# v3 调研 → 验证体系闭环计划

> 来源：`docs/research/network-testing-verification-framework.md` §四-六 + 全部实验
> 当前状态：Q4 完成度仅 40%。10 个独立实验验证了关键假设，但 PBT 和 harness 对比未集成到项目测试体系
> 目标：将 Q0-Q4 追溯链的 Q4 从 ⬜ 全部填充为 ✅

---

## 一、Q4 现状总览

框架 v3 定义的 5 层验证，catcher 当前覆盖：

| 验证层 | 描述 | 当前状态 |
|:------:|------|:--------:|
| L1 静态验证 | 类型检查 / clippy / 编译 | ✅ 充分 |
| L2 功能验证 | 单元测试 / 集成测试 / FFI 边界 | ✅ 充分（Rust 141 + TS 323 passed） |
| L3 属性验证 | 不变量检查 / 统计对比 / 协议合规 | ❌ **缺失**（Exp 中 PBT 6/6 通过但未集成） |
| L4 场景验证 | E2E 场景 / 用户旅程 / 混沌实验 | ⚠️ S1-S16c 有但缺统计严谨性 |
| L5 生产验证 | RUM / 金丝雀 / A/B / SLO 监控 | ❌ 完全缺失 |

**核心缺口在 L3（属性验证）**— 这也是 Q4 "怎么验证 Catcher 表现得当"的主战场。

---

## 二、V1: PBT 属性基测试集成（🔴 最高优先）

### 背景

Exp 中已完成 6/6 不变量测试（纯计算），但没有集成到项目的 `cargo test` 或 `pnpm test` 中。需要将 PBT 作为 CI 的一部分运行。

### 工具选择

| 层 | 工具 | 说明 |
|----|------|------|
| Rust | `proptest` | 已在 `experiments/Cargo.toml` 中验证可用 |
| TS | `fast-check` | 已在 `experiments/` 中验证可用 |

### 不变量定义

#### V1.1: Retry 不变量

| # | 不变量 | 验证方法 | 工具 |
|----|--------|---------|:---:|
| R1 | 对任意损伤组合，实际尝试次数 ≤ `max_attempts` | 随机生成 `(max_attempts, 丢包率, 延迟)`，跑 1000 次 Monte Carlo，断言每次 `attempts ≤ max_attempts` | `proptest` |
| R2 | 单次成功不触发额外重试 | 模拟第一次请求成功 → 断言 `attempts == 1` | `proptest` |
| R3 | `base_delay × backoff_multiplier^attempt` ≤ `max_backoff` | 随机生成 `(base_delay, max_backoff, max_attempts)`，验证所有 attempt 的延迟计算 ≤ max_backoff | `proptest` |
| R4 | Jitter 产出在 `[base - jitter, base + jitter]` 范围内 | 随机生成参数，跑 1000 次采样，断言所有样本在范围内 | `proptest` |

#### V1.2: Circuit Breaker 不变量

| # | 不变量 | 验证方法 | 工具 |
|----|--------|---------|:---:|
| C1 | 状态转换图完整：Closed → Open → HalfOpen → Closed/Open，无非法转换 | 随机注入成功/失败序列，断言状态转换全部合法 | `proptest` |
| C2 | Count-based CB: 连续失败 < threshold 时保持 Closed | 随机生成连续失败序列，所有 `failures < threshold` 的 case → 断言状态保持 Closed | `proptest` |
| C3 | Rate-based CB: 滑动窗口内失败率 < threshold 时保持 Closed | 随机生成请求成功率序列，验证失败率计算精确 + 误触发率 = 0（对照 Exp8） | `proptest` |
| C4 | HalfOpen 状态最多执行 `half_open_max_requests` 个探测请求 | 随机生成 HalfOpen 期间的请求序列，断言 ≤ 配置上限 | `proptest` |

#### V1.3: Token Bucket 不变量

| # | 不变量 | 验证方法 | 工具 |
|----|--------|---------|:---:|
| T1 | 消耗 ≤ 容量 | 随机生成消耗序列，断言 tokens 永不为负 | `proptest` |
| T2 | refill 不超最大值 | 模拟时间流逝 + refill，断言 tokens ≤ capacity | `proptest` |
| T3 | 耗尽后的 `try_consume` 返回 false | 消耗到 0，再 consume → 断言 false | `proptest` |

#### V1.4: DNS 不变量

| # | 不变量 | 验证方法 | 工具 |
|----|--------|---------|:---:|
| D1 | NXDOMAIN 不触发 fallback | 注入 NXDOMAIN 响应，断言无后续解析器调用 | `proptest` |
| D2 | SERVFAIL 触发 fallback | 注入 SERVFAIL，断言调用了备用解析器 | `proptest` |

### 集成方式

```
packages/catcher-core/src/
├── ...
└── proptest/                    ← 新增目录
    ├── mod.rs
    ├── retry_proptest.rs        ← R1-R4
    ├── circuit_breaker_proptest.rs ← C1-C4
    ├── token_bucket_proptest.rs ← T1-T3
    └── dns_proptest.rs          ← D1-D2
```

Rust 侧：`cargo test --workspace` 自动包含 proptest。
TS 侧：`packages/catcher-core-ts/test/proptest/` + `pnpm test` 自动包含。

### 验收标准

- CI 中每次 push 均跑 PBT（不依赖外部网络）
- 每个不变量有明确的失败反例（故意注入错误配置验证测试会捕获）

| 项目 | 内容 |
|------|------|
| **改动量** | ~400 行（Rust ~250 行 + TS ~150 行） |
| **风险** | 低 — 纯计算，零 I/O 依赖 |
| **状态** | ⬜ |

---

## 三、V2: E2E Harness 统计严谨性升级（🟡 中优先）

### 当前问题

现有 E2E harness 缺少统计严谨性：
- 不知道每种场景需要多少轮重复才能检测到效果差异（无 power analysis）
- 报告 "100% vs 60%" 但不给出置信区间
- 无法区分 Catcher 策略的贡献 vs 随机噪声

### V2.1: Power Analysis

每个 E2E 场景需要预先计算所需样本量：

```typescript
// packages/catcher-ts/test/harness/power-analysis.ts

interface PowerAnalysisInput {
  expectedEffectSize: number   // Cohen's d 或百分比差异
  alpha: number                 // 显著性水平（默认 0.05）
  power: number                 // 统计功效（默认 0.8）
}

interface PowerAnalysisOutput {
  requiredIterations: number    // 每种条件下需要的重复轮数
  detectableDifference: number  // 在该样本量下可检测的最小差异
}
```

| 场景 | 预期效应量 | 推荐迭代数 |
|------|:--------:|:--------:|
| 弱网 retry (S2/S9) | 大（vanilla 60% vs catcher 100%） | 30-50 轮 |
| 中等网络 CB (S10/S13) | 中 | 100-200 轮 |
| 良好网络对比 (S1) | 小（ceiling effect） | 500+ 轮或无意义 |

### V2.2: 效应量 + 置信区间报告

`ComparisonReporter` 需增加：

| 指标 | 当前 | 需增加 |
|------|:---:|--------|
| 成功率 | 点估计（"100%"） | 95% CI（Wilson score interval） |
| 延迟 | 点估计（P50/P95/P99） | Bootstrap CI |
| 效应量 | 无 | Cohen's d / h 或 relative risk ratio |
| p 值 | 无 | 双尾检验（vanilla vs catcher） |

### V2.3: 方差归因

```typescript
// 区分三类方差来源
interface VarianceDecomposition {
  networkNoise: number      // proxy.ts 的随机损伤波动
  serverNoise: number       // 测试服务器的响应时间波动
  catcherEffect: number     // Catcher 策略的净贡献
}
```

### 验收标准

- 每个 E2E 场景在报告中包含 95% CI
- Power analysis 结果写入场景配置（标记"该场景须 ≥ N 轮"）
- `reporter` 输出方差归因表

| 项目 | 内容 |
|------|------|
| **改动量** | ~200 行（power analysis ~50 行 + reporter ~100 行 + 方差 ~50 行） |
| **风险** | 低 — 纯计算，不改变场景执行逻辑 |
| **状态** | ⬜ |

---

## 四、V3: Proxy 损伤模型补齐（🟡 中优先）

### 当前覆盖 vs 缺失

| 损伤 | 当前 | 目标 | 实验验证 |
|------|:---:|------|:-------:|
| 延迟抖动 (jitter) | ❌ 仅有固定延迟 | uniform/normal 分布 | Exp10: Rust/tokio 精度 ~10μs 足够 |
| 突发丢包 (burst loss) | ⚠️ 仅有 2-state Markov | 4-state Markov（区分轻度/严重拥塞） | Exp1: GE 模型已验证 |
| 路由黑洞 (blackhole) | ❌ 无 | 静默丢弃所有包，无 RST | Exp1: 重试在 30% 突发丢包下 99.998% |
| 上下行不对称 | ❌ | upload/download 独立配置 | test-strategy-gaps.md §3.3 |
| 带宽波动 | ❌ | 周期性随机波动 | — |

### 实现优先级

| 阶段 | 改进 | 改动量 | 支撑场景 |
|:----:|------|:------:|------|
| 0 | **blackhole 模式** | ~15 行 | S12a/b/c |
| 1 | Jitter 参数（uniform/normal） | ~10 行 | S14 |
| 2 | Gilbert-Elliott 4-state burst loss | ~60 行 | S10 |
| 3 | 上下行分离 (upload/download) | ~40 行 | S9/S11 |
| 4 | 带宽波动 | ~20 行 | — |

### 详细设计

#### V3.0: Blackhole 模式

```typescript
// proxy.ts — 在 createThrottledPipe 的 data handler 最前面
source.on('data', async (chunk: Buffer) => {
  if (conditions.blackhole?.enabled) return  // 静默丢弃
  // ... existing logic ...
})
```

#### V3.1: Jitter 参数

```typescript
interface NetworkConditions {
  latency?: number
  jitter?: number              // ±抖动范围 (uniform)，默认 latency × 0.25
  jitterDistribution?: 'uniform' | 'normal'
  jitterStdDev?: number        // 正态分布用标准差
}
// actualLatency = latency + random(-jitter, +jitter)，clamp 到 0 以上
```

#### V3.2: 4-state Markov

```
状态:  GOOD → LIGHT_CONGESTION → HEAVY_CONGESTION → OFF
       ↑________________________________________________↓

每个状态有独立的丢包率 + 转移概率矩阵
```

### 验收标准

- Blackhole 模式 30s 内 Catcher CB 触发（不再 hang 到 OS 默认 127s）
- Jitter 场景下延迟尖刺能触发自适应超时调整
- Gilbert-Elliott 4-state 的统计分布与 Exp1 的 Monte Carlo 结果一致

| 项目 | 内容 |
|------|------|
| **改动量** | ~145 行（proxy.ts ~100 行 + presets.ts ~45 行） |
| **风险** | 低 — 仅 proxy 层改动，不影响生产代码 |
| **状态** | ⬜ |

---

## 五、V4: 新增 E2E 测试场景（🟡 中优先）

### 背景

test-strategy-gaps.md 列出了 S9-S16 共 8 个缺失场景。以下选取 P0 优先级的 3 个：

### V4.1: S12 路由黑洞

| 子场景 | 操作 | 验证点 |
|--------|------|--------|
| S12a | 黑洞 30s，期间发 200 请求 | Catcher 是否比 vanilla 更早检测到不可用？CB 是否在 30s 内进入 OPEN？ |
| S12b | 黑洞 30s → 恢复 → 再发 200 | 僵尸 keepAlive 连接是否被清理？恢复后请求是否成功？ |
| S12c | 10s 黑洞 → 5s 正常 × 5 轮 | CB 状态转换正确性：Closed ↔ Open ↔ HalfOpen |

**依赖**：V3.0 blackhole 模式。

### V4.2: S10 突发丢包风暴

| 操作 | 验证点 |
|------|--------|
| Gilbert-Elliott 4-state 模型，30s 坏状态（80% 丢包）后恢复 | retry + CB 在坏状态期间的行为：是否触发 CB？是否超过 retry budget？恢复后是否正常？ |

**依赖**：V3.2 Gilbert-Elliott 4-state + V1.2 CB 不变量。

### V4.3: S9 GPRS 极端弱网

| 操作 | 验证点 |
|------|--------|
| 500ms RTT, 50kbps 下行, 20kbps 上行，5% 丢包 | retry 极限条件下 `max_attempts=3` 是否仍然有效？退避是否过早封顶？ |

**依赖**：V3.3 上下行分离。

### 验收标准

- 全部 P0 场景（S9/S10/S12）通过 CI E2E 测试
- 每个场景的 Harness 输出满足 V2 的统计严谨性要求（至少报告 95% CI）

| 项目 | 内容 |
|------|------|
| **改动量** | ~300 行（场景定义 ~100 行 + Harness 适配 ~200 行） |
| **风险** | 中 — S12 场景需要精确的时序控制，对 proxy.ts 的黑洞模式实现质量敏感 |
| **状态** | ⬜ |

---

## 六、V5: 协议合规自动化测试（🟢 低优先）

### 背景

phaseF-protocol-compliance-matrix.md 已产出合规矩阵框架，但未系统化为自动化测试。

### 检查项

| # | 协议行为 | RFC | 当前状态 | 需测试 |
|----|---------|-----|:---:|:---:|
| P1 | 408 → 新连接重试 | RFC 9110 §15.5.7 | 代码修复后 ✅ | 自动化验证：创建 keepAlive 连接 → 服务端超时返回 408 → Catcher 是否在新连接上重试 |
| P2 | 429 Retry-After 解析 | RFC 6585 | C4 修复后 ✅ | 自动化验证：注入 429 + Retry-After: 5 → 断言退避延迟 = 5000ms |
| P3 | H2 GOAWAY 重试 | RFC 7540 §6.8 | ❌ | 自动化验证：注入 GOAWAY(last_stream_id=5) → 断言 stream_id>5 的请求被重试 |
| P4 | WS Close 双向确认 | RFC 6455 §7 | ⚠️ | 自动化验证：Catcher 发送 Close 后等待对端 Close 再断开 TCP |
| P5 | SSE Last-Event-ID | WHATWG HTML §9.2 | ⚠️ | 自动化验证：重连后发送的 HTTP 请求包含 `Last-Event-ID` header |

### 实现方式

- **P1/P2/P3**：在现有 E2E 测试服务器中增加端点，返回特定协议行为
- **P4**：在 WS 测试服务器中增加 Close 序列验证
- **P5**：在 SSE 测试服务器中增加 Last-Event-ID 检查

### 验收标准

- CI 中增加 `pnpm test:compliance` 单独运行协议合规套件
- 每个 RFC 至少 1 个自动化测试用例

| 项目 | 内容 |
|------|------|
| **改动量** | ~200 行（测试服务器 ~80 行 + 测试用例 ~120 行） |
| **风险** | 低 — 不改变生产代码 |
| **状态** | ⬜ |

---

## 七、V6: SLO 定义与 Burn Rate 告警埋点（🟢 低优先，远期）

### 背景

框架 v3 §4.3 和 BEYOND-RETRY.md §五 指出 Catcher 缺少 SLO 定义和运维层面的验证。

### 设计方向

```rust
// catcher-core 暴露指标供外部监控系统消费
pub struct CatcherMetrics {
    /// 请求成功率（5xx + 超时 + 连接失败 = 失败）
    pub request_success_rate: f64,
    /// P50/P95/P99 延迟
    pub latency_percentiles: LatencyDistribution,
    /// CB 状态变化次数（用于 burn rate 告警）
    pub cb_state_changes: u64,
    /// retry budget 消耗率
    pub retry_budget_consumption_rate: f64,
    /// keepAlive 连接存活率
    pub keepalive_survival_rate: f64,
}
```

### Google SRE 风格的告警阈值

```
1h burn rate > 14.4× → critical（2% error budget consumed）
6h burn rate > 6×    → warning（5% error budget consumed）
```

### 行动

- Rust 侧暴露 `CatcherMetrics` 结构体（已部分存在 `MetricsCollector`）
- napi-rs 层暴露 JS 可读的 metrics 对象
- **不自行实现告警** — 用户接入自己的 Prometheus / Datadog / CloudWatch

| 项目 | 内容 |
|------|------|
| **改动量** | ~100 行（`CatcherMetrics` 结构体 + 指标采集点） |
| **风险** | 低 — 纯暴露数据 |
| **状态** | ⬜ |

---

## 八、Q0-Q4 追溯链完整度提升路径

执行以上 V1-V6 后，7 个 P0 缺口的 Q4 从 ⬜ 变为：

| # | 缺口 | Q0 | Q1 | Q2 | Q3 | Q4 (前) | Q4 (后) |
|---|------|:--:|:--:|:--:|:--:|:------:|:------:|
| 1 | DNS SERVFAIL 区分 | ✅ | ✅ | ✅ | ⚠️ | ⬜ | ✅ V1.4 PBT |
| 2 | HTTP 408 Retryable | ✅ | ✅ | ✅ | ⚠️ | ⬜ | ✅ V5 P1 合规 |
| 3 | HTTP 429 Retry-After | ✅ | ✅ | ✅ | ⚠️ | ⬜ | ✅ V5 P2 合规 |
| 4 | Retry budget | ✅ | ✅ | ✅ | ❌ | ⬜ | ✅ V1.3 PBT |
| 5 | connect_timeout=15s | ✅ | ✅ | ✅ | ⚠️ | ⬜ | ✅ V4.1 S12 黑洞 |
| 6 | Doze/iOS 退避 | ⚠️ | ✅ | ✅ | ❌ | ⬜ | ⬜ 留待移动端集成 |
| 7 | GEO 退避 RTT 联动 | ✅ | ✅ | ✅ | ✅ | ⬜ | ✅ V1.1 PBT |

**执行后**：6/7 缺口 Q4 闭合（#6 因依赖移动端集成推迟）。

---

## 九、执行路线图

```
第 1-2 周 (V1 — 零 I/O，可立即开始):
  ├── V1.1: Retry PBT (Rust proptest) → ~80 行
  ├── V1.2: CB PBT → ~80 行
  ├── V1.3: Token Bucket PBT → ~50 行
  └── V1.4: DNS PBT → ~40 行

第 2-3 周 (V3 — proxy 改进):
  ├── V3.0: Blackhole 模式 → ~15 行
  ├── V3.1: Jitter 参数 → ~10 行
  └── V3.2: Gilbert-Elliott 4-state → ~60 行

第 3-4 周 (V4 — 新 E2E 场景):
  ├── V4.1: S12 路由黑洞 → ~100 行
  ├── V4.2: S10 突发丢包风暴 → ~100 行
  └── V4.3: S9 GPRS 极端弱网 → ~100 行

第 4-6 周 (V2 + V5):
  ├── V2: Harness 统计严谨性 → ~200 行
  └── V5: 协议合规自动化 → ~200 行

远期:
  └── V6: SLO 埋点 → ~100 行
```

**总计**：~1,035 行测试/验证代码，可在 4-6 周内完成。

---

## 十、关联文档

| 文档 | 关系 |
|------|------|
| `docs/research/network-testing-verification-framework.md` | 验证框架总纲 §四-六 |
| `docs/research/V3-COMPLETION.md` | Q4 当前完成度 + 10 个实验结论 |
| `docs/research/test-strategy-gaps.md` | S9-S16 场景定义 |
| `docs/research/phaseF-protocol-compliance-matrix.md` | V5 协议合规依据 |
| `docs/research/phaseG-cross-validation.md` | 交叉验证一致性检查 |
| `docs/plan/v3-code-fixes.md` | C1-C9 修复完成后 V1/V5 的测试对象就绪 |
| `docs/plan/v3-architecture-changes.md` | A1/A2/A3 落地后 V1.2/V1.4 验证 |
| `docs/plan/07-test-reuse.md` | E2E 测试复用方案 |
