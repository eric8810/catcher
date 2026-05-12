# Issue: 代理延迟在连接建立时固化，keepAlive 连接复用导致整个 E2E 测试数据不可靠

**发现来源**: 代码审查 — S1 🟡弱网/🔴极弱网 P50=54ms，在 1000ms/2000ms 单向延迟下不可能出现

**严重程度**: 🔴 严重 — 整个 E2E 对比测试的数据被污染

---

## 根因（两个层面）

### 层面 1：代理延迟在连接建立时固化

`test/network/proxy.ts:141-142` 在 `targetSocket.connect()` 回调中捕获延迟：

```typescript
targetSocket.connect(targetPort, '127.0.0.1', () => {
  const latency = conditions.latency ?? 0  // ← 此时固化！
  const bw = conditions.bandwidth ?? 0     // ← 此时固化！
  createThrottledPipe(clientSocket, targetSocket, latency, bw)
  createThrottledPipe(targetSocket, clientSocket, latency, bw)
})
```

后续 `setConditions()` 改变延迟后，已建立的连接不受影响。

### 层面 2：axios keepAlive 连接跨测试复用

axios 默认全局 `http.Agent({ keepAlive: true })` 缓存同一 `host:port` 的连接。测试流程：

```
S1 🟢良好 → setConditions({latency: 25})     → 建立连接 A(25ms)
S1 🟡弱网 → setConditions({latency: 1000})   → 复用连接 A(25ms) !!!
S1 🔴极弱网 → setConditions({latency: 2000}) → 复用连接 A(25ms) !!!
S2 🟡弱网 → setConditions({latency: 1000})   → 连接 A 被 reset 销毁，新建 B(1000ms) ✓
```

## 证据

| 场景 | 代理设置 | 预期 RTT | 实测 P50 | 匹配？ |
|------|:---:|:---:|:---:|:---:|
| S1 🟢良好 | 25ms | ~50ms | 55ms | ✅ |
| S1 🟡弱网 | 1000ms | ~2000ms | 54ms | ❌ 与良好一致 |
| S1 🔴极弱网 | 2000ms | ~4000ms | 54ms | ❌ 与良好一致 |
| S2 🔴极弱网 | 2000ms | ~4000ms | 4.0s | ✅ |

S1 弱网/极弱网的 54ms 与良好网络完全一样，说明走的是同一个连接。S2 的 4.0s 恰好是 2×2000ms = 4000ms。

代理单独用 Node.js 验证：`latency: 1000` → RTT=2010ms ✅，逻辑本身正确。

## 影响

所有 E2E 弱网/极弱网测试数据被上一测试的 keepAlive 连接污染：

- **延迟数据**：S1/S3/S4/S5/S7 弱网场景可能走了良好网络的旧连接，P50 严重偏低
- **成功率数据**：无延迟 = 无 packetLoss 效果（packetLoss 在 chunk handler 里，但 chunk 可能根本没走代理管道？不，chunk 还是走管道，但延迟没了则 dropout 时机也变了）
- **retry 触发**：延迟不够则超时少、ECONNRESET 少，retry 不会被触发，catcher 的优势被低估
- **keepAlive 问题**：连接没经历真实弱网压力，S8 的 33% 失败率可能也是别的原因

## 修复

1. **代理层面**：`createThrottledPipe` 不捕获 `latency`/`bw` 参数，改为每次 chunk 实时读 `conditions`
2. **测试层面**：`setConditions` 后调用 `disruptAll()` 断开所有旧连接，确保新请求走新连接

## 关联

- 所有 5 个已有 issue 的 E2E 证据可能都需要重新评估
- [keepalive-broken-connection.md](./keepalive-broken-connection.md)
- [retry-reuses-bad-connection.md](./retry-reuses-bad-connection.md)
