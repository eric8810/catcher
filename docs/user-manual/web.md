# Web 浏览器使用指南

> 状态：✅ 已实现 — `@eric8810/catcher-web` 包，fetch-based，纯 TS  
> 代码位置：`packages/catcher-web/`

---

## 为什么需要独立包

`@eric8810/catcher-http` 基于 axios → `node:http`，不兼容浏览器。`@eric8810/catcher-web` 将韧性层（retry / CB / queue / interceptors）完整保留，底层换为 `fetch()`。

```
@eric8810/catcher-http (Node.js)        @eric8810/catcher-web (Browser)
─────────────────────          ────────────────────
axios → node:http              fetch() → browser HTTP
p-retry + cockatiel            p-retry + cockatiel (相同)
@eric8810/catcher-core (types)          @eric8810/catcher-core (types) (相同)
```

---

## API（目标）

```typescript
import { createWebClient } from '@eric8810/catcher-web'

const client = createWebClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
  concurrency: 10,
})

// 与 @eric8810/catcher-http 完全一致的 API
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

| | @eric8810/catcher-http (Node.js) | @eric8810/catcher-web (Browser) |
|--|------------------------|----------------------|
| HTTP 底层 | axios → node:http | fetch() |
| keepAlive | ✅ Node.js Agent 连接池 | ❌ 浏览器自动管理 |
| DNS 缓存 | ✅ cacheable-lookup | ❌ 浏览器 DNS |
| WebSocket | `ws` 库 | 原生 `WebSocket` |
| CORS | 无关 | ⚠️ 需要服务端配合 |
| 响应流 | stream | ReadableStream |
| 超时 | axios timeout | AbortController + setTimeout |

---

## 当前状态

| 功能 | 状态 |
|------|------|
| HTTP GET/POST/PUT/DELETE/PATCH | ✅ 已实现（fetch 底层） |
| retry + 退避 | ✅ 已实现（p-retry） |
| circuitBreaker | ✅ 已实现（cockatiel） |
| 优先级队列 | ✅ 已实现（p-queue） |
| 动态拦截器 | ⏳ stub（待完集成） |
| keepAlive / DNS 缓存 | ❌ 浏览器不支持 |
| WebSocket client | ⏳ 待建（原生 WebSocket 封装） |
