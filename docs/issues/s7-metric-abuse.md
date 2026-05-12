# Issue: S7 用 `msgFinishOrder`（完成排名）当延迟指标，报告中出现 -2000% 虚假退化

**发现来源**: S7 🟢良好网络/🟡弱网报告显示 P50: vanilla=1ms vs catcher=21ms (-2000%)

**严重程度**: 🟡 中（不影响库本身，但污染报告）

---

## 根因

`test/e2e/scenarios.test.ts:477`（vanillaS7）和 catcherS7 的 `time` 字段存的是 `msgFinishOrder`：

```typescript
return { success, time: msgFinishOrder, connections: 0 }
```

`msgFinishOrder` = 消息请求在 21 个并发请求中的完成排名（1~21，越小越好）。但 harness 把它当毫秒算 P50/P95。

- vanilla 无并发限制，消息无延迟 → 总是第 1 个完成 → 显示 "1ms"
- catcher 有 concurrency=10 队列，消息等槽位 → 第 10~21 个完成 → 显示 "21ms"

这不是队列慢了 20ms，是**两个不可比的量被当成同一单位展示**。

## 建议

S7 的 `time` 改为消息请求的实际延迟（ms）：

```typescript
const msgStart = Date.now()
const msgResp = await client.post('/messages', { text: 'prio test' })
const msgLatency = Date.now() - msgStart
return { success, time: msgLatency, connections: 0 }
```

同时保留 `msgFinishOrder` 作为优先级验证的辅助指标（可以在报告中单独标注）。

## 关联

- reporter 的 S7 异常数字 (-2000%) 拖垮整体平均延迟改善
