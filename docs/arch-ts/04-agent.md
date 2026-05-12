# 04 — 共享 Agent

> 对应源文件：`packages/catcher-ts/src/agent/shared-agent.ts`（83 行）

## 职责

创建可跨 HTTP 客户端复用的 `https.Agent` 实例，提供：
- TCP keep-alive 连接复用
- DNS 解析结果缓存（基于 `cacheable-lookup`）
- 空闲连接健康检查与自动驱逐

## 核心导出

### `createSharedAgent(options?) → https.Agent`

```typescript
import { createSharedAgent } from 'catcher/agent'

const agent = createSharedAgent({
  keepAlive: true,              // 启用 TCP keep-alive
  keepAliveMsecs: 30_000,       // 空闲保活时长
  maxSockets: 25,               // 每 host 最大并发连接
  maxFreeSockets: 10,           // 每 host 最大空闲连接
  timeout: 60_000,              // socket 超时
  rejectUnauthorized: false,    // TLS 证书验证
  dnsCacheTtl: 300,             // DNS 缓存 TTL (秒)，0 = 禁用
})

// 所有客户端共用同一个 Agent
const client1 = axios.create({ baseURL, httpsAgent: agent })
const client2 = axios.create({ baseURL, httpsAgent: agent })
```

### `clearDnsCache()`

重置全局 DNS 缓存（测试或网络切换时使用）。

```typescript
import { clearDnsCache } from 'catcher/agent'

// 网络切换后强制重新解析
clearDnsCache()
```

### 附加导出

`catcher/agent` 同时 re-export `cacheable-lookup` 的默认导出，供高级场景使用：

```typescript
import { CacheableLookup } from 'catcher/agent'
```

## 实现细节

### 连接池参数

| 参数 | 默认值 | Node.js 默认值 | 说明 |
|------|--------|---------------|------|
| `keepAlive` | `true` | `false` | 启用后 TCP 连接在请求完成后保持打开 |
| `keepAliveMsecs` | `30_000` (30s) | `5_000` (5s) | 空闲 keep-alive 探测间隔 |
| `maxSockets` | `25` | `Infinity` | 限制并发连接防止资源耗尽 |
| `maxFreeSockets` | `10` | `256` | 限制空闲连接数 |
| `freeSocketTimeout` | `keepAliveMsecs + 5_000` | 无 | 空闲连接超时驱逐 |
| `scheduling` | `'fifo'` | `'lifo'` | FIFO 避免连接囤积 |
| `timeout` | `60_000` (60s) | 无 | socket 级别超时 |
| `rejectUnauthorized` | `false` | `true` | 跳过 TLS 证书验证 |

### DNS 缓存

使用模块级全局单例 `CacheableLookup` 实例，懒初始化：

```
shared-agent.ts:_defaultDnsCache (module-level singleton)
  └── getDnsCache(ttl) → CacheableLookup
        └── new CacheableLookup({ maxTtl: ttl })
```

- `dnsCacheTtl > 0`：将 `CacheableLookup.lookup` 注入 Agent options
- `dnsCacheTtl = 0`：不注入，使用系统 DNS 解析
- `clearDnsCache()` 将单例置 `null`，下次调用重新创建

### 健康检查（`free` 事件）

每次 socket 释放回池时，在 `agent.on('free')` 中：

```
socket 释放回池
  → agent 触发 'free' 事件
    → socket 已销毁？→ 跳过
    → 注册 socket.once('error', onError)  // onError → socket.destroy()
    → 注册 socket.once('close', cleanup)  // cleanup → 移除 error 监听器
```

1. 检查 socket 是否已销毁（`socket.destroyed`）
2. 注册 `error` 一次性监听器，一旦出错立即 `socket.destroy()`
3. 注册 `close` 一次性监听器，清理 error 监听器，防止内存泄漏

这解决了 **Issue #1**：retry 时复用已断开的 keep-alive 连接导致 `ECONNRESET`。

## 使用建议

- **复用 Agent**：所有指向同一 host 的 HTTP 客户端应共用同一个 Agent 实例，避免重复 TCP + TLS 握手
- **分离 Agent**：不同 host 使用不同 Agent 实例，或使用 `dnsCacheTtl = 0` 禁用 DNS 缓存
- **测试场景**：每个测试用例前后调用 `clearDnsCache()` 避免 test pollution

## 依赖

| 依赖 | 用途 |
|------|------|
| `cacheable-lookup` (^7.0.0) | DNS 缓存 |
| `node:https` | https.Agent 基类 |
