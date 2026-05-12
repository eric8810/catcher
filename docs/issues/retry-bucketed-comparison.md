# Issue: 延迟对比不应跨重试次数混合计算

**发现来源**: P95 报告中大量负项实际是 retry 成本（拿延迟换成功率），但统计把 0-retry 的请求和 1-retry、2-retry 的请求混在一起算 P50/P95

**严重程度**: 🟡 中（不影响库，但报告误导）

---

## 根因

当前每个迭代返回一个 `time` + `success`，所有迭代混在一起算百分位：

```
S1 弱网 5 迭代:
vanilla:  [2s, 2s, 2s, 2s, 10s†]   → P50=2s
catcher:  [2s, 2s, 2s, 3.5s(retry1), 4s(retry2)] → P50=2s ← 但均值/P95被retry拉高
```

catcher 的 retry 请求比 vanilla 的 0-retry 请求多花 1~3s（退避延迟），但**多花的代价换来了成功率提升**。混在一起算百分位，catcher 必然输。

## 正确做法：按重试次数分桶

```
S1 🟡弱网 (5 iterations)
┌────────────────┬─────────┬─────────┐
│                │ vanilla │ catcher │
├────────────────┼─────────┼─────────┤
│ 成功率          │   80%   │  100%   │
│ 0-retry 成功    │  4 次   │  3 次   │
│ 0-retry P50    │  2.0s   │  2.0s   │  ← 公平基线
│ 1-retry 成功    │   —     │  2 次   │
│ 1-retry 延迟    │   —     │  3.5s   │  ← retry 代价单独展示
│ 失败            │  1 次   │  0 次   │
└────────────────┴─────────┴─────────┘
```

0-retry vs 0-retry 对比才是公平的。retry 代价单独列出，不污染延迟对比。

## 改动范围

4 个文件：

1. `src/types.ts` — `HttpClientConfig.retry` 加 `onRetry` 回调，让调用方感知重试次数
2. `src/http/retry.ts` — 在 `onFailedAttempt` 里调用 `onRetry`，递增计数器
3. `test/harness.ts` — `IterationResult` 加 `retries` 字段；`ScenarioMetrics` 加 `byRetries` 分桶数据
4. `test/reporters/comparison-reporter.ts` — 分桶展示，0-retry 对比延迟，retry 代价单独列

## 关联

- 报告中 S2 3G P95=-100%(+15s) 等负项本质是 retry 成本被当成了退化
- [s7-metric-abuse.md](./s7-metric-abuse.md) — 同类问题：metric 定义导致不可比
