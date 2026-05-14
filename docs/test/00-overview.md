# 00 — 测试框架概览

> 代码位置：`packages/test/`

---

## 目标

catcher 的测试框架不是传统的单元测试，而是一个**网络韧性验证系统**。核心目标：

1. **对比验证** — 同等网络条件下，catcher 是否确实优于 vanilla？
2. **极限探索** — catcher 的各种韧性机制（retry / CB / keepAlive）的边界在哪里？
3. **回归防护** — 新代码是否破坏了已有的韧性保证？

---

## 测试分层

```
Layer 4: 场景测试 (e2e/)
         S1-S16, 模拟真实用户旅程
         网络：proxy 模拟损伤 + profile 预设
         指标：成功率、p50/p95/p99、retry 次数

Layer 3: 基准测试 (benchmark/)
         高并发吞吐、连接效率
         网络：直连（测极限）+ proxy（测弱网）
         指标：req/sec、p50/p95/p99、连接数

Layer 2: 集成测试 (integration/)
         单个特性验证（retry、keepAlive、DNS cache）
         网络：proxy 模拟损伤
         指标：功能正确性 + 基础对比

Layer 1: 单元测试 (packages/*/src/**)
         各包内部逻辑（拦截器、队列、编解码、SSE）
         SSE: router (24 tests), stream (23), strict (10), client (11)
         网络：无，纯逻辑测试
```

---

## 核心 Harness：并发对比

`harness.ts` 的 `runConcurrentComparison()` 是整个 E2E 测试的核心：

```
for i in 1..N:
    vanillaResult, catcherResult = await Promise.all([
        vanillaFn(baseUrl),    // ← 同一时刻、同一网络
        catcherFn(baseUrl),
    ])
```

关键设计：vanilla 和 catcher **在同一轮迭代中并发执行**，共享 proxy 的随机状态。这确保了 vanilla 遇到的那次丢包，catcher 也会遇到——对比结果才有统计意义。

---

## 指标体系

| 指标 | 含义 | 用途 |
|------|------|------|
| successRate | N 次迭代中成功的比例 | **主指标** |
| zeroRetryP50/P95 | 未触发重试的成功请求延迟 | 公平对比 vanilla（都一次成功的时间差） |
| retriedMean | 触发重试后成功的平均延迟 | retry 的延迟代价 |
| zeroRetrySuccesses / retriedSuccesses | 按是否重试分组 | retry 有效性 |
| avgRetries | 平均每次成功请求的重试次数 | retry 策略激进程度 |
| avgConnections | 平均 TCP 连接数 | keepAlive 效率 |
| avgBytes | 平均传输字节数 | msgpack 压缩效果 |

**关于"all-in" vs "仅成功"延迟**：

- "仅成功延迟"（success-only latency）— 只看成功的请求。适合双方成功率都高时对比网络效率
- "all-in 延迟"（包括失败）— 失败请求计入 timeout 值。适合弱网场景
- 基准文档中默认使用 success-only，避免 timeout 值污染分布

---

## 环境变量控制

| 变量 | 默认 | 用途 |
|------|------|------|
| `THROUGHPUT_REQUESTS` | 500 | 直连吞吐测试请求数 |
| `THROUGHPUT_CONCURRENCY` | 50 | 直连吞吐并发数 |
| `WEAK_REQUESTS` | 500 | 弱网测试请求数 |
| `WEAK_CONCURRENCY` | 50 | 弱网测试并发数 |
| `MIXED_REQUESTS` | 300 | 混合负载请求数 |
| `MIXED_CONCURRENCY` | 30 | 混合负载并发数 |
| `EXTREME_ITERATIONS` | 30 | 极端场景迭代次数 |
| `CHAOS_DURATION_MS` | 60000 | 混沌测试持续时间 |

CI 环境可以设小值快速验证，本地开发可以设大值获得稳定数据。
