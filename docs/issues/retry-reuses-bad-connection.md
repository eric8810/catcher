# Issue: Retry 不建新连接，导致对坏连接反复重试

**发现来源**: E2E 场景 S3 🟡弱网 — catcher P50=8s vs vanilla P50=2s，双方成功率均为 100%

**严重程度**: 🔴 高

---

## 根因

当 keepAlive 连接池中的 TCP 连接因丢包/断连变得不可用，`p-retry` 仍然用这个坏连接重试：

```
vanilla (每次新连接):
  请求 → 新 TCP → 成功 → 2s

catcher (keepAlive + retry):
  请求 → 复用坏连接 → socket hang up
  → retry#1(1s后) → 复用同一个坏连接 → socket hang up
  → retry#2(2s后) → 复用同一个坏连接 → socket hang up
  → retry#3(4s后) → 恰好连接恢复 → 成功 → 8s
```

catcher 比 vanilla 慢 4 倍，且重试都在浪费的同一个损坏连接上。

## 当前代码位置

- `src/http/retry.ts:15-42` — `createRetryWrapper` 使用 `p-retry`，不控制底层连接
- `src/http/client.ts:26` — 创建 `createSharedAgent` 后，所有请求共享同一个 Agent 实例
- `src/agent/shared-agent.ts:29-43` — Agent 配置了 `keepAliveMsecs: 30_000`，连接在关闭前被缓存

## 影响

- **轻度弱网**: retry 在不该重试时触发，延迟放大 3-4 倍
- **中度弱网**: 所有请求都走同一个损坏连接池，catcher 成功率可能比 vanilla 还低
- **极弱网**: S2 🔴极弱网 20%→100% 证明了 retry 有价值，但延迟代价高

## 建议修复

1. retry 时为每次重试请求建新的 `https.Agent` 或新 Socket
2. 或使用 `agent.destroy()` 在 retry 前踢掉坏连接
3. 或利用 Node.js 的 `socket.on('error')` 自动标记 socket 不可用（Node 已部分支持）

## 关联问题

- [keepalive-broken-connection.md](./keepalive-broken-connection.md)
