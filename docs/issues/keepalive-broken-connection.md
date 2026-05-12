# Issue: keepAlive 连接无健康检查，坏连接被反复重用

**发现来源**: E2E 场景 S5 🟡弱网 — catcher=60% vs vanilla=80%；S8 🟡弱网 — catcher=40% vs vanilla=60%

**严重程度**: 🔴 高

---

## 根因

共享 Agent 的 keepAlive 连接池在弱网下是**双刃剑**：

- ✅ 好的一面：减少 TCP+TLS 握手次数（S1 连接数从 3→1）
- ❌ 坏的一面：一旦池中连接损坏，所有后续请求都走坏连接，全部失败

```
良好网络:
  keepAlive 连接池 → 复用正常连接 → 快

弱网 (5% 丢包, 2% 断连):
  第一次请求 → 连接建立 → socket hang up(2%触发)
  → 连接标记为 CLOSE_WAIT 但仍在池中
  第二次请求 → 复用坏连接 → socket hang up
  第三次请求 → 复用坏连接 → socket hang up
  ...
  全部失败
```

而 vanilla 每次新建连接，某个连接坏了不影响下一个。

## 当前代码位置

- `src/agent/shared-agent.ts:29-43` — Agent 配置了 `keepAlive: true`, `keepAliveMsecs: 30_000`，但没有连接健康检查
- `src/http/client.ts:26` — `createSharedAgent({ keepAlive, dnsCacheTtl, ... })`

## 数据证据

| 场景 | 网络 | Vanilla 成功率 | Catcher 成功率 | 差异 |
|------|------|:---:|:---:|:---:|
| S5 大体积 | 🟡弱网 | 80% | 60% | **-20pp** |
| S8 DNS缓存 | 🟡弱网 | 60% | 40% | **-20pp** |

S5 和 S8 都是多请求场景，catcher 的 keepAlive 在连接损坏后**系统性降低成功率**。

## 对比：极弱网为什么反而好？

S1 🔴极弱网: vanilla=60% → catcher=100%，catcher 胜出。因为极弱网下**每次连接都大概率失败**，catcher 的 retry 机制比 keepAlive 问题更重要。

## 建议修复

1. **连接健康检查**: 复用前对空闲连接做快速 ping/timeout 检查
2. **最大复用次数限制**: 单连接最多复用 N 次后主动关闭重建
3. **socket error 自动驱逐**: 在 `socket.on('error')` 时立即从池中移除（Node 22+ 部分支持）
4. **retry 时建新连接**: retry 逻辑应强制使用 fresh socket

## 关联问题

- [retry-reuses-bad-connection.md](./retry-reuses-bad-connection.md)
