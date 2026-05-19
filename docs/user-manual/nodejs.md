# Node.js & Electron 使用指南

---

## 双轨策略

| | `@eric8810/napi-*` (推荐) | `@eric8810/catcher-http` (备选) |
|--|:--:|:--:|
| 实现 | Rust via napi-rs | 纯 TypeScript |
| 网络层 | reqwest (Rust) | axios → node:http |
| 韧性层 | catcher-rs (Rust) | p-retry + cockatiel (TS) |
| 拦截器 | ❌ 待暴露 | ✅ 完整 |
| TLS | ✅ rustls 内置 | ✅ Node.js 内置 |
| 状态 | ✅ 已编译 | ✅ 功能最全 |

---

## 一、Rust Native 版本

```bash
npm install @eric8810/catcher-napi-http @eric8810/catcher-napi-ws
```

### HTTP

```typescript
import { HttpClient } from '@eric8810/catcher-napi-http'

const client = new HttpClient({
  base_url: 'https://api.example.com',
  connect_timeout_ms: 5000,
  response_timeout_ms: 30000,
  retry: { max_attempts: 3, backoff: 'Fixed' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
  dns: { cache_ttl_secs: 300, stale_on_error: true },
  msgpack: true,
})

const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())
console.log('Elapsed:', resp.elapsedMs, 'ms')

await client.post('/messages', Buffer.from('hello'), { contentType: 'text/plain' })
```

### WebSocket

```typescript
import { WsClient } from '@eric8810/catcher-napi-ws'
import type { WsEvent } from '@eric8810/catcher-napi-ws'

const ws = new WsClient(
  {
    urls: ['wss://cn.example.com', 'wss://sg.example.com'],
    reconnect: { initial_delay_ms: 1000, max_delay_ms: 30000, max_attempts: 20 },
  },
  (event: WsEvent) => {
    // event 已经是解析后的强类型对象，无需 JSON.parse
    if (event.type === 'Message') {
      console.log(Buffer.from(event.data_base64, 'base64').toString())
    }
  },
)

ws.send('hello')
ws.close()
```

> **TLS**: `wss://` connections work out of the box (rustls bundled, no system TLS dependency).

---

## 二、TypeScript 版本

```bash
npm install @eric8810/catcher-http @eric8810/catcher-ws
```

### HTTP

```typescript
import { createHttpClient } from '@eric8810/catcher-http'

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
import { createResilientWS, pack, decodeWSMessage } from '@eric8810/catcher-ws'

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
import { createSharedAgent, clearDnsCache } from '@eric8810/catcher-http'
import { createPriorityQueue, enqueueWithPriority } from '@eric8810/catcher-http'

// 共享连接池
const agent = createSharedAgent({ keepAlive: true, dnsCacheTtl: 300 })
clearDnsCache()

// 优先级队列
const queue = createPriorityQueue({ concurrency: 10 })
await enqueueWithPriority(queue, 1, async () => fetchData())
```

### SSE — AI 流式响应

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

// 一次性流式请求（OpenAI / Anthropic / Gemini 兼容）
const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${apiKey}` },
  body: { model: 'gpt-4', messages: [{ role: 'user', content: 'Hello' }], stream: true },
})

for await (const line of stream) {
  if (!line.startsWith('data:')) continue
  const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)
  if (payload === '[DONE]') break
  process.stdout.write(JSON.parse(payload).choices[0]?.delta?.content ?? '')
}
// 循环结束 = 连接关闭，无需手动清理
```

### SSE — 长连接推送（自动重连）

```typescript
import { createSSEClient } from '@eric8810/catcher-http'

const client = createSSEClient({
  url: 'https://api.example.com/events',
  headers: { Authorization: 'Bearer xxx' },
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
})

for await (const line of client) {
  if (line.startsWith('data: ')) console.log(line.slice(6))
}

// 永不主动断开。如需断开：
client.close()
```

---

## 三、Electron

Main process 直接引用，两种包均可用：

```typescript
// main.ts — native 版
import { HttpClient } from '@eric8810/catcher-napi-http'
// 或 TS 版
import { createHttpClient } from '@eric8810/catcher-http'

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

---

## 四、DNS 缓存

NAPI 版内置 StaleAwareDnsResolver，默认启用。无需额外配置即可获得 DNS 缓存。

| 配置 | 说明 | 默认值 |
|------|------|--------|
| `dns.cache_ttl_secs` | 缓存有效期 | 300 |
| `dns.stale_ttl_secs` | 过期后仍可用的宽限期 | 3600 |
| `dns.stale_on_error` | DNS 失败时用旧缓存兜底 | true |

Benchmark：cold start 203ms → cached 0.3ms（676x 加速）。

## 五、Msgpack 内置编解码

设置 `msgpack: true` 后，transport 层自动将 JSON body 编码为 msgpack 发送，收到 msgpack 响应后自动解码为 JSON 返回给 JS。编解码在 Rust 内部完成，无 NAPI 边界开销。

```typescript
const client = new HttpClient({
  base_url: 'https://api.example.com',
  msgpack: true,
})
// JS 侧始终收发 JSON，wire 上走 msgpack（小 10-35%）
```

WS 同理：
```typescript
const ws = new WsClient({
  urls: ['wss://rt.example.com'],
  msgpack: true,
})
// send text → Rust 自动编码为 msgpack binary frame
// 收到 binary → Rust 自动解码为 JSON text event
```
