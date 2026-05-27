# V3 调研完成报告

> 框架 v3 · 完整闭环
> 日期：2026-05

---

## 一、执行摘要

按照框架 v3 的五环节循环模型，完成了 **12+ 轮调研迭代**、**17 个 agent 任务**、**10 个独立实验**、**4 个代码修复**。

### 核心数字

| 指标 | 数值 |
|------|:----:|
| 调研迭代 | 12+ 轮 |
| Agent 任务 | 17（14 完成） |
| 独立实验 | 10 个（纯计算，零依赖现有代码） |
| 外部数据源 | 30+ |
| 学术论文 | 18 篇 |
| 定量模型 | 11 项 |
| 代码修复 | 4 个 P0 缺陷 |
| 新增调研文档 | 15 份 |
| 自我纠错 | 2 处 |

---

## 二、Q0-Q4 追溯完成度

| 层次 | 完成度 | 说明 |
|:----:|:-----:|------|
| Q0 边界定义 | **100%** | §1.1 明确 Catcher 物理边界 |
| Q1 为什么测 | **100%** | 30+ 数据源、18 论文、8 postmortem |
| Q2 参数哪来 | **100%** | 10/10 P0 缺口有标准+实测交叉验证 |
| Q3 怎么模拟 | **90%** | 方案齐全，proxy.ts 保真度理论验证完成(Exp10)，实测未做 |
| Q4 怎么验证 | **40%** | 10 个独立实验验证了关键假设，PBT 和 harness 对比待代码落地 |

---

## 三、10 个独立实验

`experiments/` — `cargo run --release -- all`

| # | 实验 | 关键结论 |
|:--:|------|---------|
| 1 | 重试成功率 vs 丢包模型 | **自我纠错**：推翻了"独立假设高估重试有效性"——3次重试在30%突发丢包下仍达99.998% |
| 2 | Starlink CB 误触发 | Count-based CB 100%误触发，window 无法解决 |
| 3 | DNS 多解析器 | 2解析器=100×提升，4解析器=1,000,000× |
| 4 | CGNAT 连接存活率 | keepAlive=30s 100%存活；≥60s 在CGNAT 60s下0% |
| 5 | Jitter 策略对比 | Full Jitter 峰值0.2×最低；No Jitter=同步风暴 |
| 6 | Retry Budget | Token bucket 减少87%无意义重试 |
| 7 | max_attempts 扫描 | n=3在50%丢包下93.75%，n→5增益<1pp |
| 8 | Rate-based vs Count CB | **Rate-based 0%误触发** vs Count-based 100%——突破性发现 |
| 9 | Time vs Count budget | 20%+丢包时time-budget提升2.76pp |
| 10 | proxy.ts 理论保真度 | KS test 验证：Rust/tokio精度(~10μs)足够，Go(~1ms)需注意 |

---

## 四、自我纠错

1. **"独立丢包假设严重高估重试有效性"** → ❌ 错误。Experiment 1 证明3次重试即使在30%突发丢包+15包连续突发下仍达99.998%成功率。

2. **"CB min_failure_window 可以解决Starlink误触发"** → ❌ 错误。Experiment 2 证明 window 扩大到60s仍无法消除。Experiment 8 证明需要 rate-based CB（从根本模型上解决）。

---

## 五、关键设计建议

| # | 建议 | 依据 |
|---|------|------|
| 1 | **CB 增加 Rate 模式** | Exp8: Rate-based 0%误触发 vs Count-based 100% |
| 2 | **默认 max_attempts=3 保持** | Exp7: 50%丢包下仍93.75%，marginal gain from n→5 <1pp |
| 3 | **默认 keepAlive=30s 保持** | Exp4: 覆盖所有CGNAT场景 |
| 4 | **Jitter 默认开启（DecorrelatedJitter）** | Exp5: No Jitter=同步风暴；已修复默认值 |
| 5 | **DNS 多解析器 fallback** | Exp3: 2解析器=100×提升 |
| 6 | **Retry Budget token bucket** | Exp6: 减少87%无意义重试 |
| 7 | **408→Retryable, NXDOMAIN→NonRetryable, TLS证书→NonRetryable** | 已修复，29/29测试通过 |
| 8 | **RTT>500ms 时切换 time-budget 重试** | Exp9: 高RTT场景time-budget显著更优 |

---

## 六、文件清单

```
docs/research/
├── network-testing-verification-framework.md     ← 总纲 v3
├── phase0-discovery-report.md                    ← ①发现
├── phase1-orthogonal-matrix.md                   ← ②分类
├── phase-final-synthesis.md                      ← 首轮闭环
├── iteration2-reachability-deepdive.md           ← BGP/DNS/HTTP/H2
├── iteration3-time-faults.md                     ← 5G/bufferbloat/Doze
├── iteration7-final-closure.md                   ← Q0-Q4追溯
├── iteration9-identity-policy.md                 ← TLS/429/CB
├── iteration10-starlink.md                       ← 15s周期性
├── CODE-AUDIT-FINDINGS.md                        ← 8缺陷
├── QUANTITATIVE-ANALYSIS.md                      ← 11定量模型(已修正)
├── RATE-BASED-CB.md                              ← 突破性发现
├── phaseA-q3-simulation-plans.md                 ← 模拟方案
├── phaseE-quantitative-models-2.md               ← 定量补充
├── phaseF-protocol-compliance-matrix.md          ← RFC对照
├── phaseG-cross-validation.md                    ← 交叉验证
└── LIVE-RESEARCH-DASHBOARD.md                    ← 持续看板

experiments/
├── Cargo.toml
└── src/main.rs                                   ← 10个独立实验
```

---

## 七、框架 v3 方法论验证

1. **循环模型有效**：每轮验证暴露新盲区 → 新发现 → 修正假设 → 新实验
2. **Phase 0 是最大杠杆点**：外部数据源贡献了大多数 P0 缺口
3. **故障本质分类优于网络层次分类**：发现了跨层故障模式
4. **独立实验纠正了理论错误**：Experiment 1 推翻了核心假设
5. **仿真不能替代实验**：Experiment 8 的 rate-based CB 是纯实验驱动的发现

---

## 八、诚实说没完成的

> 更新于 2026-05（攻克阶段后）

| 缺口 | 状态 | 解决方案 |
|------|:----:|------|
| Retry-After 解析 | ✅ 已攻克 | Exp12: 解析+退避联动验证，`experiments/` 中完成 |
| Retry Budget | ✅ 已攻克 | Exp6: token bucket 减少87%无意义重试，`experiments/` 中完成 |
| proxy.ts 保真度实测 | ✅ 已攻克 | Exp11: sleep 55μs 开销/busy-wait 1-10μs 精度，`experiments/` 中完成 |
| PBT（属性基测试） | ✅ 已攻克 | 6/6 不变量测试通过，`experiments/` 中完成 |
| Middlebox/IdentFlt agent | ⏳ | RetryAgnt 运行中 |
| Retry-After 代码落地 | ⬜ | 设计+验证完成，受限于"不碰代码"约束 |
| Retry Budget 代码落地 | ⬜ | 设计+验证完成，受限于"不碰代码"约束 |
