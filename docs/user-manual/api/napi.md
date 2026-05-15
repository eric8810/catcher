# napi API Reference

> Node.js native bindings — `@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws`

```bash
npm install @eric8810/catcher-napi-http @eric8810/catcher-napi-ws
```

---

## 概述

napi 包将 Rust 核心编译为原生 `.node` 附加模块。配置通过 JSON 字符串传递，不支持动态拦截器。

| | napi 版 | TS 版 |
|--|:-----:|:----:|
| 性能 | 🚀 Rust 原生 | ⚡ TS |
| 拦截器 | ❌ | ✅ |
| SSE | ❌ | ✅ |
| 编解码 | ✅ (Rust) | ✅ (TS) |

---

## @eric8810/catcher-napi-http

### 导入

```javascript
const { HttpClient } = require('@eric8810/catcher-napi-http')
// 或
import { HttpClient } from '@eric8810/catcher-napi-http'
```

### HttpClient

```typescript
class HttpClient {
  constructor(configJson: string)
  get(url: string, options?: RequestOptions): Promise<JsHttpResponse>
  post(url: string, body?: Buffer, options?: RequestOptions): Promise<JsHttpResponse>
  put(url: string, body?: Buffer, options?: RequestOptions): Promise<JsHttpResponse>
  delete(url: string, options?: RequestOptions): Promise<JsHttpResponse>
  patch(url: string, body?: Buffer, options?: RequestOptions): Promise<JsHttpResponse>
  circuitBreakerState(): string  // 'closed' | 'open' | 'half-open'
}
```

### 构造配置 JSON

```json
{
  "base_url": "https://api.example.com",
  "connect_timeout_ms": 5000,
  "response_timeout_ms": 30000,
  "keep_alive": true,
  "pool": {
    "keep_alive": true,
    "max_idle_per_host": 10,
    "idle_timeout_secs": 60
  },
  "retry": {
    "max_attempts": 3,
    "backoff": "Exponential",
    "min_backoff_ms": 500,
    "max_backoff_ms": 30000
  },
  "circuit_breaker": {
    "failure_threshold": 5,
    "success_threshold": 2,
    "reset_timeout_ms": 30000
  }
}
```

### 配置字段

| JSON 字段 | 类型 | 默认 | 说明 |
|-----------|------|------|------|
| `base_url` | `string` | **必填** | 基础 URL |
| `connect_timeout_ms` | `number` | `5000` | 连接超时 |
| `response_timeout_ms` | `number` | `30000` | 响应超时 |
| `keep_alive` | `boolean` | `true` | TCP keep-alive |
| `pool.keep_alive` | `boolean` | `true` | 连接池 keep-alive |
| `pool.max_idle_per_host` | `number` | `10` | 每 host 最大空闲连接 |
| `pool.idle_timeout_secs` | `number` | `60` | 空闲超时（秒） |
| `retry.max_attempts` | `number` | `3` | 最多尝试次数 |
| `retry.backoff` | `"Fixed" \| "Exponential" \| "DecorrelatedJitter"` | `"Exponential"` | 退避策略 |
| `retry.min_backoff_ms` | `number` | `500` | 最小退避延迟 |
| `retry.max_backoff_ms` | `number` | `30000` | 最大退避延迟 |
| `circuit_breaker.failure_threshold` | `number` | `5` | 连续失败 → OPEN |
| `circuit_breaker.success_threshold` | `number` | `2` | HALF_OPEN 连续成功 → CLOSED |
| `circuit_breaker.reset_timeout_ms` | `number` | `30000` | OPEN → HALF_OPEN 等待 |
| `dns.nameservers` | `string[]` | — | 自定义 DNS |
| `proxy.url` | `string` | — | 代理 URL |
| `tls` | `object` | — | 同 Rust TLS 配置 |

### RequestOptions

| 字段 | 类型 | 说明 |
|------|------|------|
| `headers` | `Record<string, string>` | 请求头 |
| `timeout_ms` | `number` | per-request 超时覆盖 |
| `content_type` | `string` | Content-Type |

### JsHttpResponse

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | `number` | HTTP 状态码 |
| `headers` | `Record<string, string>` | 响应头 |
| `body` | `Buffer` | 响应体（二进制） |
| `elapsed_ms` | `number` | 耗时（ms） |

### 示例

```javascript
const { HttpClient } = require('@eric8810/catcher-napi-http')

const client = new HttpClient(JSON.stringify({
  base_url: 'https://api.example.com',
  connect_timeout_ms: 5000,
  response_timeout_ms: 30000,
  keep_alive: true,
  retry: { max_attempts: 3, backoff: 'Exponential' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
}))

// GET
const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

// POST with body
await client.post('/messages', Buffer.from('hello'), { content_type: 'text/plain' })

// POST with headers
await client.post('/messages', Buffer.from(JSON.stringify({ text: 'hi' })), {
  headers: { Authorization: 'Bearer xxx' },
  content_type: 'application/json',
})

// Circuit breaker state
console.log(client.circuitBreakerState())  // 'closed'
```

---

## @eric8810/catcher-napi-ws

### 导入

```javascript
const { WsClient } = require('@eric8810/catcher-napi-ws')
```

### WsClient

```typescript
class WsClient {
  constructor(configJson: string, onEvent?: (eventJson: string) => void)
  send(data: string): void
  close(): void
}
```

### 构造配置 JSON

```json
{
  "urls": ["wss://cn.example.com", "wss://sg.example.com"],
  "per_message_deflate": true,
  "handshake_timeout_ms": 10000,
  "reconnect": {
    "initial_delay_ms": 1000,
    "max_delay_ms": 30000,
    "max_attempts": 20
  },
  "heartbeat": {
    "interval_ms": 30000,
    "adaptive": true
  }
}
```

### 配置字段

| JSON 字段 | 类型 | 默认 | 说明 |
|-----------|------|------|------|
| `urls` | `string[]` | **必填** | WebSocket URL(s) |
| `per_message_deflate` | `boolean` | `true` | per-message deflate |
| `handshake_timeout_ms` | `number` | `10000` | 握手超时 |
| `max_message_size` | `number` | `1048576` | 最大消息（字节） |
| `reconnect.initial_delay_ms` | `number` | `1000` | 初始重连延迟 |
| `reconnect.max_delay_ms` | `number` | `30000` | 最大重连延迟 |
| `reconnect.backoff_multiplier` | `number` | `2.0` | 指数因子 |
| `reconnect.max_attempts` | `number` | `20` | 最多重连次数 |
| `heartbeat.interval_ms` | `number` | `30000` | 心跳间隔 |
| `heartbeat.adaptive` | `boolean` | `true` | 自适应间隔 |
| `heartbeat.ping_timeout_ms` | `number` | `10000` | Ping 超时 |
| `headers` | `object` | `{}` | 自定义头 |
| `reject_unauthorized` | `boolean` | `true` | TLS 校验 |

### 回调事件 JSON

```typescript
// eventJson 是 JSON 字符串，解析后：
type WsEventJson =
  | { type: 'Connected'; url: string; latency_ms: number }
  | { type: 'Disconnected'; code: number; reason: string }
  | { type: 'Message'; data: string; is_binary: boolean }
  | { type: 'Error'; message: string }
  | { type: 'Reconnecting'; attempt: number; delay_ms: number }
  | { type: 'HeartbeatRtt'; rtt_ms: number }
```

### 示例

```javascript
const { WsClient } = require('@eric8810/catcher-napi-ws')

const ws = new WsClient(JSON.stringify({
  urls: ['wss://cn.example.com', 'wss://sg.example.com'],
  per_message_deflate: true,
  reconnect: { initial_delay_ms: 1000, max_delay_ms: 30000, max_attempts: 20 },
}), (eventJson) => {
  const event = JSON.parse(eventJson)
  switch (event.type) {
    case 'Connected':
      console.log(`Connected to ${event.url} (${event.latency_ms}ms)`)
      break
    case 'Message':
      console.log('Received:', event.is_binary ? '(binary)' : event.data)
      break
    case 'Disconnected':
      console.log(`Disconnected: ${event.code} ${event.reason}`)
      break
    case 'Reconnecting':
      console.log(`Reconnecting attempt ${event.attempt} in ${event.delay_ms}ms`)
      break
    case 'HeartbeatRtt':
      console.log(`Heartbeat RTT: ${event.rtt_ms}ms`)
      break
    case 'Error':
      console.error('Error:', event.message)
      break
  }
})

ws.send('hello')
ws.close()
```

---

## 平台支持

| 平台 | Target |
|------|--------|
| Linux x64 gnu | `linux-x64-gnu` |
| Linux x64 musl | `linux-x64-musl` |
| macOS arm64 | `darwin-arm64` |
| macOS x64 | `darwin-x64` |
| Windows x64 | `win32-x64-msvc` |
