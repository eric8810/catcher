# @eric8810/catcher-web API Reference

> 浏览器 HTTP + SSE + WebSocket 客户端 — fetch-based

```bash
npm install @eric8810/catcher-web
```

---

## 导出清单

```typescript
import {
  // HTTP 客户端
  createWebClient,

  // WebSocket 客户端
  createWebSocketClient,

  // SSE
  createSSEStream,
  createSSEClient,
} from '@eric8810/catcher-web'

import type {
  IHttpClient,
  RequestConfig,
  HttpClientConfig,
  HttpResponse,
  SSEStreamOptions,
  SSEClientOptions,
  SSEStream,
  SSEClient,
  SSETimeoutError,
  CatcherHttpError,
  CatcherErrorType,
  ClientEvent,
  WebSocketClientOptions,
  WebSocketClient,
  WsStatus,
} from '@eric8810/catcher-web'

export { isCatcherError } from '@eric8810/catcher-web'
```

---

## createWebClient

```typescript
function createWebClient(config: HttpClientConfig): IHttpClient
```

创建 fetch-based HTTP 客户端，接口与 `@eric8810/catcher-http` 完全一致。

**与 Node.js 版的差异**：

| | catcher-http (Node.js) | catcher-web (Browser) |
|--|----------------------|----------------------|
| 底层 HTTP | axios → node:http | fetch() |
| keepAlive | ✅ Agent 连接池 | ❌ 浏览器管理 |
| DNS 缓存 | ✅ cacheable-lookup | ❌ 浏览器 DNS |
| WebSocket | `ws` 库 | 原生 `WebSocket` |
| CORS | 无关 | 需服务端配合 |
| 代理 | ✅ HTTP/SOCKS5 | ❌ 浏览器不支持 |

### HttpClientConfig（Web 特有字段）

| 参数 | 类型 | 说明 |
|------|------|------|
| `credentials` | `'include' \| 'same-origin' \| 'omit'` | fetch credentials 策略 |
| `fetchMode` | `'cors' \| 'no-cors' \| 'same-origin' \| 'navigate'` | fetch mode |

Node.js 特有的 `keepAlive` / `dnsCacheTtl` / `proxy` / `dns` / `tls` 在 Web 端无效（静默忽略）。

### 示例

```typescript
import { createWebClient } from '@eric8810/catcher-web'

const client = createWebClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3 },
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
  concurrency: 10,
  credentials: 'include',
})

const data = await client.get('/users/1')

await client.post('/messages', { text: 'hello' })

// 和 Node.js 版一致的拦截器
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})
```

IHttpClient, RequestConfig, HttpResponse, InterceptorManager 等接口与 catcher-http 相同，参见 [`api/ts-http.md`](./ts-http.md)。

---

## createWebSocketClient

```typescript
function createWebSocketClient(options: WebSocketClientOptions): WebSocketClient
```

创建浏览器原生 WebSocket 客户端。

### WebSocketClientOptions

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `url` | `string \| string[]` | **必填** | WebSocket URL(s) |
| `protocol` | `string \| string[]` | — | 子协议 |
| `reconnect` | `{ initialDelay, maxDelay, maxAttempts }` | — | 自动重连 |
| `headers` | `Record<string, string>` | `{}` | 自定义头（浏览器可能不支持） |

### WebSocketClient

```typescript
interface WebSocketClient {
  send(data: string | ArrayBuffer | Blob): void
  close(code?: number, reason?: string): void
  readonly readyState: number
  readonly url: string
  readonly status: WsStatus  // 'CONNECTING' | 'CONNECTED' | 'CLOSED'
  addEventListener(type: 'open' | 'close' | 'message' | 'error' | 'statuschange', listener: EventListener): void
}
```

### 示例

```typescript
import { createWebSocketClient } from '@eric8810/catcher-web'

const ws = createWebSocketClient({
  url: 'wss://api.example.com/ws',
  reconnect: { initialDelay: 1000, maxDelay: 30000 },
})

ws.addEventListener('message', e => {
  console.log(e.data)
})

ws.send(JSON.stringify({ event: 'ping' }))
```

---

## createSSEStream

```typescript
function createSSEStream(options: SSEStreamOptions): SSEStream
```

一次性 SSE 流（AI 流式对话）。API 与 Node.js 版一致。

```typescript
import { createSSEStream } from '@eric8810/catcher-web'

const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${key}` },
  body: { model: 'gpt-4', messages, stream: true },
})

for await (const line of stream) {
  // 浏览器端同样的 async iteration
}
```

## createSSEClient

```typescript
function createSSEClient(options: SSEClientOptions): SSEClient
```

长连接 SSE 带自动重连。API 与 Node.js 版一致。

**浏览器注意事项**：
- CORS：服务端需返回 `Access-Control-Allow-Origin`
- `Last-Event-ID` 头通过 fetch headers 发送

---

## 接口复用

以下接口与 `@eric8810/catcher-http` 完全一致，此处不重复展开，参见 [`api/ts-http.md`](./ts-http.md)：

- `IHttpClient` — HTTP 客户端接口
- `RequestConfig` — 请求配置
- `HttpClientConfig` — 客户端配置（部分字段仅 Node.js 有效）
- `HttpResponse` — 响应对象
- `CatcherHttpError`, `CatcherErrorType` — 错误类型
- `ClientEvent` — 事件类型
- `SSEStream`, `SSEClient`, `SSEStreamOptions`, `SSEClientOptions` — SSE 接口
