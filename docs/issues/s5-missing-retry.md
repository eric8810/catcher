# Issue: S5 大体积消息场景未开 retry，弱网下 keepAlive 坏连接导致 catcher 成功率低于 vanilla

**发现来源**: S5 🟡弱网 vanilla=80% vs catcher=60%（修复代理后）

**严重程度**: 🟡 中

---

## 根因

S5 测试的 catcher 函数 (`test/e2e/scenarios.test.ts:270-290`) 未配置 `retry`：

```typescript
const client = createHttpClient({
  baseURL: baseUrl, keepAlive: true,
  timeout: { response: 15_000 },
  // 没有 retry!
})
```

大体积消息列表请求在弱网下（1000ms 延迟 + 5% 丢包 + 25KB/s 带宽限制），keepAlive 连接池中的坏连接会导致请求失败。没有 retry 保护，失败直接暴露。

而 S5 🔴极弱网 catcher=100% vs vanilla=80% 之所以胜出，是因为极弱网的随机性恰好没触发 keepAlive 坏连接。

## 建议

S5 catcher 加上 `retry: { attempts: 2 }`，与其他场景保持一致。

## 关联

- [keepalive-broken-connection.md](./keepalive-broken-connection.md) — keepAlive 无 retry 时是双刃剑
