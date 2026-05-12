# Issue: reporter 统计三个缺陷：全失败时 P50 假改善、S7 拉低平均、P95 相对百分比失真

**发现来源**: 新报告整体改善 P50=-287.8%，P95=-89.1%

**严重程度**: 🟡 中

---

## 三个具体问题

### 1. 全部失败时 P50 改善显示 +100%

S6 🟡弱网 catcher 全部失败(0%)，catcher P50=`-`。`improvements.p50All` 公式：

```typescript
p50All: vanilla.p50All > 0 ? (vanilla.p50All - catcher.p50All) / vanilla.p50All : 0
```

catcher.p50All=0（无成功样本），算出 +100%，但实际 catcher 全部失败应该是退化。

### 2. S7 的 msgFinishOrder 拉低整体平均延迟

S7 的 -2000% 被纳入 19 个场景的 P50 平均，直接拖成 -287.8%。S7 应该用实际延迟而非完成排名（见 [s7-metric-abuse.md](./s7-metric-abuse.md)），或从延迟平均中排除。

### 3. P95 相对百分比在绝对值小的场景下失真

S4 🌍跨地域：vanilla P95=605ms, catcher P95=1.6s。公式 `(605-1600)/605 = -165%`。实际差距只有 ~1s，但百分比看起来极差。小基数放大了相对差异。

## 修复

1. catcher 全部失败时 P50/P95 改善 = `null` 或 N/A，不参与平均
2. S7 修完 metric 后自动解决；或者标记 S7 为"特殊指标场景"
3. P95 改善同时展示绝对差值（ms），不只有相对百分比

## 关联

- [s7-metric-abuse.md](./s7-metric-abuse.md)
