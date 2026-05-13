# Web 浏览器使用指南

> 状态：⚠️ 缺失 — 唯一需要新建 TS 包的平台。Rust/WASM 不能做网络，必须纯 TS + fetch。

---

## 为什么需要独立包

`@catcher/http` 基于 axios → `node:http`，不兼容浏览器。`@catcher/web` 将韧性层（retry / CB / queue / interceptors）完整保留，底层换为 `fetch()`。

```
@catcher/http (Node.js)        @catcher/web (Browser)
─────────────────────          ────────────────────
axios → node:http              fetch() → browser HTTP
p-retry + cockatiel            p-retry + cockatiel (相同)
@catcher/core (types)          @catcher/core (types) (相同)
```

---

## API（目标）

```typescript
import { createWebClient } from '@catcher/web'

const client = createWebClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
  concurrency: 10,
})

// 与 @catcher/http 完全一致的 API
const data = await client.get('/users/1')
await client.post('/messages', { text: 'hello' })

// AbortController 原生支持
const controller = new AbortController()
await client.get('/search?q=test', { signal: controller.signal })

// 动态拦截器
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${getToken()}`
  return config
})
```

---

## 浏览器特有差异

| | @catcher/http (Node.js) | @catcher/web (Browser) |
|--|------------------------|----------------------|
| HTTP 底层 | axios → node:http | fetch() |
| keepAlive | ✅ Node.js Agent 连接池 | ❌ 浏览器自动管理 |
| DNS 缓存 | ✅ cacheable-lookup | ❌ 浏览器 DNS |
| WebSocket | `ws` 库 | 原生 `WebSocket` |
| CORS | 无关 | ⚠️ 需要服务端配合 |
| 响应流 | stream | ReadableStream |
| 超时 | axios timeout | AbortController + setTimeout |

---

## 当前限制

| 功能 | 状态 |
|------|------|
| HTTP GET/POST/PUT/DELETE/PATCH | ⚠️ 新建包 |
| retry + 退避 | ⚠️ 复用 p-retry |
| circuitBreaker | ⚠️ 复用 cockatiel |
| 优先级队列 | ⚠️ 复用 p-queue |
| 动态拦截器 | ⚠️ 复用拦截器管理器 |
| keepAlive / DNS 缓存 | ❌ 浏览器不支持 |
| WebSocket client | ⚠️ 原生 WebSocket + 重连封装 |

---

## 优先级

P0 — 在所有平台中，Web 用户量最大、实现成本最低（纯 TS，复用韧性逻辑，仅换网络底层）。建议在 Rust crate 之前先做。
