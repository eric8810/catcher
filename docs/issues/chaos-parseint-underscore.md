# Issue: chaos 测试 `parseInt('600_000')` 因下划线分隔符只解析到 600ms

**发现来源**: 测试运行时发现 chaos 实际只跑了 600ms，而非预期的 10 分钟

**严重程度**: 🟡 中（不影响库，但测试无效）

---

## 根因

`test/chaos/chaos.test.ts:28`：

```typescript
const CHAOS_DURATION_MS = parseInt(process.env.CHAOS_DURATION_MS ?? '600_000', 10)
```

`parseInt` 遇到非数字字符 `_` 时停止解析，所以 `parseInt('600_000', 10)` 返回 `600`，不是 `600000`。

## 影响

混沌测试默认只跑 600ms（不到 1 个 send 周期），完全没有压力测试效果。之前所有 chaos 测试结果都是无效的。

## 修复

`'600_000'` → `'600000'`

## 关联

- 混沌测试中 circuit breaker 不会有机会被触发（600ms 内最多 1 次 send）
