# Issue: 延迟对比未按重试次数分桶，catcher 的 1-retry 成功被拿去和 vanilla 的 0-retry 成功比延迟

**发现来源**: 修复后报告仍有 P95 负项，追查发现全是 retry 代价被当退化

**严重程度**: 🟡 中 — 统计方法缺陷，不影响库本身

---

## 根因

同一个 5 次迭代中：
- vanilla: 4 次 0-retry 成功 (2s), 1 次失败
- catcher: 3 次 0-retry 成功 (2s), 2 次 1-retry 成功 (3.5s)

当前算法：算 P50 时 catcher 的 5 个时间 [2, 2, 2, 3.5, 3.5]，vanilla 的 4 个 [2, 2, 2, 2]。catcher P50=2s (OK)，P95=3.5s → 比 vanilla P95=2s → 算作"恶化"。

但实际上：
- **0-retry 对 0-retry**: 双方 P50 都是 2s，持平
- **1-retry 成功**: vanilla 没有这个能力（它的 1 次失败=10s timeout），catcher 用 +1.5s 代价把失败变成了成功

把不同重试次数的请求混在一起算百分位，等于要求 catcher "既要 retry 救回失败，又要和没 retry 一样快"——不公平。

## 修复方案

### 数据层

1. `IterationResult` 新增 `retries?: number` 字段
2. `RetryOptions` 新增 `onRetry` 回调，每次 retry 触发时通知
3. E2E catcher 函数通过 `onRetry` 记录重试次数，填入 `IterationResult.retries`

### 展示层

每个场景的延迟拆成三行：

```
0-retry 成功  →  双方公平对比基础设施开销（keepAlive, 队列）
N-retry 成功  →  单独列 retry 代价，不和 0-retry 混算
失败          →  含在成功率里，不计延迟
```

### 汇总层

- 成功率改善：保持不变（主指标）
- 0-retry P50 改善：只在双方都有 ≥1 次 0-retry 的场景参与
- retry 代价：独立统计 "catcher 在 X% 成功迭代中触发 retry，平均增加 Yms"

不再计算含失败 P95（没有意义），retry 代价单独看。
