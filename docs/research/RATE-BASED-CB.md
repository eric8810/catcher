# Rate-based Circuit Breaker — 实验验证报告

> 基于 Experiment 8 结果
> 发现日期：2026-05

---

## 问题

Catcher 当前的 CB 使用 **count-based** 模型：连续 N 次失败 → OPEN。

Experiment 2 证明：在 Starlink 15s 周期性 RTT 突增场景下，这个模型 **100% 误触发**。

## 实验对比

| CB 模型 | 参数 | Starlink 误触发率 |
|---------|------|:------:|
| Count-based | threshold=3 | **100%** |
| Count-based | threshold=5 | **100%** |
| Count-based | threshold=10 | **100%** |
| Count-based | threshold=20 | **100%** |
| **Rate-based** | rate_threshold=30% | **0%** |
| **Rate-based** | rate_threshold=50% | **0%** |
| **Rate-based** | rate_threshold=70% | **0%** |
| **Rate-based** | rate_threshold=90% | **0%** |

## 原理

Count-based: 只要 spike 期间连续 ≥5 个请求超时 → OPEN。Starlink 的 2s spike 在 ≥1 req/s 下必定触发。

Rate-based: 滑动时间窗口内的**失败率**超过阈值 → OPEN。Starlink 的 2s spike 只占 15s 周期的 13.3%。即使 spike 期间请求全失败，窗口内失败率仍 ≈ 13.3%，远低于任何合理阈值（30-50%）。

## 建议

Catcher 的 CB 应增加模式选择：

```rust
pub enum CbMode {
    /// 连续失败计数（当前实现，适合独立故障）
    Count { failure_threshold: u32 },
    /// 滑动窗口失败率（提案，适合周期性抖动场景）
    Rate { 
        failure_rate_threshold: f64,  // 如 0.5 = 50%
        window_seconds: f64,          // 如 30.0
    },
}
```

默认建议：`Rate { failure_rate_threshold: 0.5, window_seconds: 30.0 }`
