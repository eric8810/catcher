# Node.js & Electron 使用指南

---

## 双轨策略

| | `@catcher/napi-*` (推荐) | `@catcher/http` (备选) |
|--|:--:|:--:|
| 实现 | Rust via napi-rs | 纯 TypeScript |
| 网络层 | reqwest (Rust) | axios → node:http |
| 韧性层 | catcher-rs (Rust) | p-retry + cockatiel (TS) |
| 拦截器 | ❌ 待暴露 | ✅ 完整 |
| 状态 | ✅ 已编译 | ✅ 功能最全 |

---

## 一、Rust Native 版本

```bash
npm install @catcher/napi-http @catcher/napi-ws
```

### HTTP

```javascript
const { HttpClient } = require('@catcher/napi-http')

const client = new HttpClient(JSON.stringify({
  base_url: 'https://api.example.com',
  connect_timeout_ms: 5000,
  response_timeout_ms: 30000,
  keep_alive: true,
  retry: { max_attempts: 3, backoff: 'exponential' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
}))

const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

await client.post('/messages', Buffer.from('hello'), 'text/plain')
```

### WebSocket

```javascript
const { WsClient } = require('@catcher/napi-ws')

const ws = new WsClient(JSON.stringify({
  urls: ['wss://cn.example.com', 'wss://sg.example.com'],
  per_message_deflate: true,
  reconnect: { initial_delay_ms: 1000, max_delay_ms: 30000, max_attempts: 20 },
}), (eventJson) => {
  const event = JSON.parse(eventJson)
  console.log(event.type, event)
})

ws.send('hello')
ws.close()
```

---

## 二、TypeScript 版本

```bash
npm install @catcher/http @catcher/ws
```

### HTTP

```typescript
import { createHttpClient } from '@catcher/http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  keepAlive: true,
  retry: { attempts: 3, backoff: 'exponential' },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
  concurrency: 10,
})

const data = await client.get('/users/1')
const result = await client.post('/messages', { text: 'hello' })

// 动态拦截器（napi 版暂不支持）
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})

// Per-request 覆盖
await client.get('/analytics', { retry: false, timeout: 5000 })
await client.post('/upload', formData, { onUploadProgress: e => console.log(e) })
```

### WebSocket

```typescript
import { createResilientWS, pack, decodeWSMessage } from '@catcher/ws'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],
  perMessageDeflate: true,
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})

ws.addEventListener('message', (e) => {
  const data = decodeWSMessage(e.data)
})

ws.send(pack({ event: 'message', data: { text: 'hi' } }))
```

### 独立组件

```typescript
import { createSharedAgent, clearDnsCache } from '@catcher/http'
import { createPriorityQueue, enqueueWithPriority } from '@catcher/http'

// 共享连接池
const agent = createSharedAgent({ keepAlive: true, dnsCacheTtl: 300 })
clearDnsCache()

// 优先级队列
const queue = createPriorityQueue({ concurrency: 10 })
await enqueueWithPriority(queue, 1, async () => fetchData())
```

---

## 三、Electron

Main process 直接引用，两种包均可用：

```typescript
// main.ts — native 版
import { HttpClient } from '@catcher/napi-http'
// 或 TS 版
import { createHttpClient } from '@catcher/http'

ipcMain.handle('api:get', async (_e, url) => {
  return await client.get(url)
})
```

```typescript
// preload.ts
contextBridge.exposeInMainWorld('api', {
  get: (url: string) => ipcRenderer.invoke('api:get', url),
})
```
