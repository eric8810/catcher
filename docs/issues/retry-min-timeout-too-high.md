# Issue: retry minTimeout 偏高导致不必要时也付出沉重退避代价

**发现来源**: S3 🔴极弱网双方 80% 成功率持平，但 catcher P50=13s vs vanilla P50=4s

**严重程度**: 🟡 中

---

## 根因

当前 `src/http/retry.ts:33` 退避序列从 1s 起步：

```typescript
minTimeout: minTimeout ?? 1_000,
factor: backoff === 'exponential' ? 2 : 1,
```

退避: 1s → 2s → 4s。在双方都能成功的场景下，catcher 遇到一次瞬时 ECONNRESET 就白花至少 1s 等待，成功率没有提升。

S3 极弱网典型路径：
```
vanilla:  请求 → 4s → 成功
catcher:  请求 → ECONNRESET → 等1s → retry → 4s → 成功  (总7s+)
          请求 → ECONNRESET → 等1s → ECONNRESET → 等2s → ECONNRESET → 失败(15s)
```

## 建议

`minTimeout: 500`，退避从 500ms/1s/2s 起步，在瞬时网络抖动场景下减半不必要的等待。

## 关联

- [retry-over-triggers.md](./retry-over-triggers.md) — retry 触发过多的根本原因之一
