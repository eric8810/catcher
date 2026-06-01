# napi API Reference

> Node.js native bindings — `@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws`

```bash
npm install @eric8810/catcher-napi-http @eric8810/catcher-napi-ws
```

---

## 概述

napi 包将 Rust 核心编译为原生 `.node` 附加模块。TypeScript wrapper 提供类型安全的配置和事件回调。

| | napi 版 | TS 版 |
|--|:-----:|:----:|
| 性能 | 🚀 Rust 原生 | ⚡ TS |
| 拦截器 | ❌ | ✅ |
| SSE | ✅ | ✅ |
| 编解码 | ✅ (Rust) | ✅ (TS) |
| 配置类型 | ✅ 强类型 | ✅ 强类型 |

### 关键特性

- **类型安全配置**：传对象或 JSON 字符串，IDE 自动补全
- **类型安全事件**：回调直接收到解析后的强类型对象（无需手动 `JSON.parse`）
- **camelCase 兼容**：`base_url` 和 `baseUrl` 均可使用（JSON 配置通过 serde alias）
- **NAPI-RS 自动类型**：`HttpResponse`、`RequestOptions`、`Metrics` 由 NAPI-RS 生成，字段名为 camelCase（如 `elapsedMs`、`timeoutMs`）
- **TLS 内置**：`wss://` 开箱即用（rustls，无系统 TLS 依赖）
- **SSE 支持**：`SseStream`（一次性）和 `SseClient`（自动重连）

---

## @eric8810/catcher-napi-http

### 导入

```typescript
import { HttpClient, SseStream, SseClient } from '@eric8810/catcher-napi-http'
import type { HttpClientConfig, SseClientConfig, SseEvent, StreamEvent } from '@eric8810/catcher-napi-http'
```

### HttpClient

```typescript
class HttpClient {
  constructor(config: HttpClientConfig | string)
  get(url: string, options?: RequestOptions): Promise<HttpResponse>
  post(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse>
  put(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse>
  delete(url: string, options?: RequestOptions): Promise<HttpResponse>
  patch(url: string, body?: Buffer, options?: RequestOptions): Promise<HttpResponse>
  circuitBreakerState(): 'closed' | 'open' | 'half-open'
  metrics(): Metrics
  executeStream(method: string, url: string, body?: Buffer, options?: RequestOptions, onChunk?: (event: StreamEvent) => void): void
  setAdaptiveTimeout(min: number, max: number, mult: number, win: number): void
  disableAdaptiveTimeout(): void
  cancelAll(): void
  cancelRequest(requestId: number): boolean
  nextRequestId(): number
}
```

### 配置

```typescript
const client = new HttpClient({
  base_url: 'https://api.example.com',
  connect_timeout_ms: 5000,
  response_timeout_ms: 30000,
  pool: {
    keep_alive: true,
    max_idle_per_host: 10,
    idle_timeout_secs: 30,
    keep_alive_interval_secs: 20,
  },
  retry: {
    max_attempts: 3,
    backoff: 'Fixed',
    min_backoff_ms: 100,
    max_backoff_ms: 10000,
    jitter: true,
  },
  circuit_breaker: {
    failure_threshold: 5,
    success_threshold: 2,
    reset_timeout_ms: 30000,
    half_open_max_requests: 5,
  },
  dns: {
    cache_size: 512,
    cache_ttl_secs: 300,
    negative_ttl_secs: 60,
    stale_ttl_secs: 3600,
    stale_on_error: true,
    nameservers: [],         // custom DNS servers, e.g. ['8.8.8.8:53']
    host_mapping: {},        // hostname → IP mapping
  },
  msgpack: true,             // auto JSON↔msgpack at transport layer
})
```

> 也支持 `baseUrl`、`connectTimeoutMs` 等 camelCase 字段名。

### 配置字段

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `base_url` | `string` | `""` | 基础 URL |
| `connect_timeout_ms` | `number` | `10000` | 连接超时 |
| `response_timeout_ms` | `number` | `30000` | 响应超时 |
| `pool.keep_alive` | `boolean` | `true` | TCP keep-alive |
| `pool.max_idle_per_host` | `number` | `10` | 每 host 最大空闲连接 |
| `pool.idle_timeout_secs` | `number` | `30` | 空闲超时（秒） |
| `pool.keep_alive_interval_secs` | `number` | `20` | keepalive 探测间隔（秒） |
| `retry.max_attempts` | `number` | `3` | 最多重试次数 |
| `retry.backoff` | `"Fixed" \| "Exponential" \| "DecorrelatedJitter"` | `"Fixed"` | 退避策略 |
| `retry.min_backoff_ms` | `number` | `100` | 最小退避延迟 |
| `retry.max_backoff_ms` | `number` | `10000` | 最大退避延迟 |
| `retry.jitter` | `boolean` | `true` | 是否添加抖动 |
| `circuit_breaker.failure_threshold` | `number` | `5` | 连续失败 → OPEN |
| `circuit_breaker.success_threshold` | `number` | `2` | HALF_OPEN 连续成功 → CLOSED |
| `circuit_breaker.reset_timeout_ms` | `number` | `30000` | OPEN → HALF_OPEN 等待 |
| `circuit_breaker.half_open_max_requests` | `number` | `5` | HALF_OPEN 最大放行数 |
| `tls` | `TlsConfig` | — | TLS 配置（14 字段） |
| `dns` | `DnsConfig` | — | DNS 配置 |
| `dns.cache_size` | `number` | `512` | DNS 缓存条目数上限 |
| `dns.cache_ttl_secs` | `number` | `300` | DNS 缓存 TTL（秒） |
| `dns.negative_ttl_secs` | `number` | `60` | 否定缓存 TTL（秒） |
| `dns.stale_ttl_secs` | `number` | `3600` | Stale 宽限期（秒）|
| `dns.stale_on_error` | `boolean` | `true` | DNS 失败时用旧缓存兜底 |
| `dns.nameservers` | `string[]` | `[]` | 自定义 DNS 服务器 |
| `dns.host_mapping` | `Record<string, string>` | `{}` | Hostname → IP 映射 |
| `proxy` | `ProxyConfig` | — | 代理配置 |
| `redirect` | `RedirectConfig` | — | 重定向配置 |
| `max_concurrency` | `number` | `50` | 最大并发请求数 |
| `default_headers` | `Record<string, string>` | `{}` | 默认请求头 |
| `auth` | `{ username, password }` | — | Basic 认证 |
| `bearer_token` | `string` | — | Bearer token |
| `msgpack` | `boolean` | `false` | 启用 transport 层 msgpack 编解码 |

### SSE

```typescript
// 一次性流（无重连）
const stream = new SseStream(
  { url: 'https://stream.example.com/events' },
  (event: SseEvent) => {
    if (event.type === 'Line') console.log(event.data)
  },
)
stream.close()

// 自动重连客户端
const sse = new SseClient(
  {
    url: 'https://stream.example.com/events',
    reconnect: { max_retries: 10, initial_delay_ms: 1000, max_delay_ms: 30000 },
    circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
  },
  (event: SseEvent) => {
    if (event.type === 'Line') console.log(event.data)
  },
)
```

### RequestOptions

> NAPI-RS auto-generated type — camelCase fields.

| 字段 | 类型 | 说明 |
|------|------|------|
| `headers` | `Record<string, string>` | 请求头 |
| `timeoutMs` | `number` | per-request 超时覆盖 |
| `contentType` | `string` | Content-Type |

### HttpResponse

> NAPI-RS auto-generated type — camelCase fields.

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | `number` | HTTP 状态码 |
| `headers` | `Record<string, string>` | 响应头 |
| `body` | `Buffer` | 响应体（二进制） |
| `elapsedMs` | `number` | 耗时（ms） |

### Metrics

> NAPI-RS auto-generated type — camelCase fields.

| 字段 | 类型 | 说明 |
|------|------|------|
| `httpRequests` | `number` | HTTP 请求数 |
| `httpSuccessRate` | `number` | HTTP 成功率 |
| `httpAvgLatencyUs` | `number` | HTTP 平均延迟（μs） |
| `httpRetries` | `number` | HTTP 重试次数 |
| `wsConnectSuccessRate` | `number` | WS 连接成功率 |
| `wsDisconnects` | `number` | WS 断连次数 |
| `wsMessagesSent` | `number` | WS 发送消息数 |
| `wsMessagesReceived` | `number` | WS 接收消息数 |
| `cbOpenCount` | `number` | 熔断器打开次数 |
| `queueTimeouts` | `number` | 队列超时次数 |

### StreamEvent

```typescript
type StreamEvent =
  | { type: 'Headers'; status: number; headers: Record<string, string> }
  | { type: 'Chunk'; data: string }   // base64 编码
  | { type: 'Done' }
  | { type: 'Error'; message: string }
```

### SseEvent

```typescript
type SseEvent =
  | { type: 'Line'; data: string }
  | { type: 'Error'; message: string }
  | { type: 'End' }
```

### 示例

```typescript
import { HttpClient, SseStream } from '@eric8810/catcher-napi-http'

const client = new HttpClient({
  base_url: 'https://api.example.com',
  retry: { max_attempts: 3, backoff: 'Fixed' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
})

// GET
const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

// POST with body
await client.post('/messages', Buffer.from('hello'), { contentType: 'text/plain' })

// POST with headers
await client.post('/messages', Buffer.from(JSON.stringify({ text: 'hi' })), {
  headers: { Authorization: 'Bearer xxx' },
  contentType: 'application/json',
})

// Circuit breaker state
console.log(client.circuitBreakerState())  // 'closed'
```

---

## @eric8810/catcher-napi-ws

### 导入

```typescript
import { WsClient } from '@eric8810/catcher-napi-ws'
import type { WsClientConfig, WsEvent } from '@eric8810/catcher-napi-ws'
```

### WsClient

```typescript
class WsClient {
  constructor(config: WsClientConfig | string, onEvent?: (event: WsEvent) => void)
  send(data: string): void
  sendBinary(data: Buffer | ArrayBuffer | Uint8Array): void
  close(code?: number, reason?: string): void
}
```

> **注意**：不要在回调内同步调用 `send()`，否则可能死锁。使用 `setImmediate` 或 `process.nextTick` 延迟。

### 配置

```typescript
const ws = new WsClient(
  {
    urls: ['wss://cn.example.com', 'wss://sg.example.com'],
    per_message_deflate: true,
    handshake_timeout_ms: 15000,
    reconnect: {
      initial_delay_ms: 500,
      max_delay_ms: 30000,
      backoff_multiplier: 2.0,
      max_attempts: 20,
    },
    heartbeat: {
      interval_ms: 30000,
      adaptive: true,
      pong_timeout_ms: 10000,
      max_missed_pongs: 3,
    },
  },
  (event: WsEvent) => {
    // event 已经是解析后的对象，无需 JSON.parse
  },
)
```

### 配置字段

| 字段 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `urls` | `string[]` | **必填** | WebSocket URL(s) |
| `per_message_deflate` | `boolean` | `true` | 标准 RFC 7692 permessage-deflate |
| `deflate_threshold_bytes` | `number` | `1024` | 压缩阈值 |
| `handshake_timeout_ms` | `number` | `15000` | 握手超时 |
| `max_payload_bytes` | `number` | `67108864` | 最大 payload（64MB） |
| `reconnect.initial_delay_ms` | `number` | `500` | 初始重连延迟 |
| `reconnect.max_delay_ms` | `number` | `30000` | 最大重连延迟 |
| `reconnect.backoff_multiplier` | `number` | `2.0` | 指数因子 |
| `reconnect.max_attempts` | `number` | `20` | 最多重连次数 |
| `heartbeat.interval_ms` | `number` | `30000` | 心跳间隔 |
| `heartbeat.adaptive` | `boolean` | `true` | 自适应间隔 |
| `heartbeat.pong_timeout_ms` | `number` | `10000` | Pong 超时 |
| `heartbeat.max_missed_pongs` | `number` | `3` | 丢失 pong 判定断线 |
| `race_count` | `number` | `1` | 同时竞速端点数 |
| `headers` | `object` | `{}` | 自定义头 |
| `msgpack` | `boolean` | `false` | 启用 msgpack 编解码 |

### 回调事件

回调直接收到强类型对象（无需 `JSON.parse`）：

```typescript
type WsEvent =
  | { type: 'Connected'; url: string; latency_ms: number }
  | { type: 'Disconnected'; code: number; reason: string }
  | { type: 'Message'; data_base64: string; is_binary: boolean }
  | { type: 'Error'; message: string }
  | { type: 'Reconnecting'; attempt: number; delay_ms: number }
  | { type: 'HeartbeatRtt'; rtt_ms: number }
```

> `data_base64` 为 base64 编码的二进制数据。解码：`Buffer.from(event.data_base64, 'base64')`。

### 示例

```typescript
import { WsClient } from '@eric8810/catcher-napi-ws'
import type { WsEvent } from '@eric8810/catcher-napi-ws'

const ws = new WsClient(
  {
    urls: ['wss://cn.example.com', 'wss://sg.example.com'],
    reconnect: { initial_delay_ms: 500, max_delay_ms: 30000, max_attempts: 20 },
  },
  (event: WsEvent) => {
    switch (event.type) {
      case 'Connected':
        console.log(`Connected to ${event.url} (${event.latency_ms}ms)`)
        break
      case 'Message':
        const text = Buffer.from(event.data_base64, 'base64').toString()
        console.log('Received:', event.is_binary ? '(binary)' : text)
        break
      case 'Disconnected':
        console.log(`Disconnected: ${event.code} ${event.reason}`)
        break
      case 'Reconnecting':
        console.log(`Reconnecting attempt ${event.attempt} in ${event.delay_ms}ms`)
        break
      case 'HeartbeatRtt':
        console.log(`RTT: ${event.rtt_ms}ms`)
        break
      case 'Error':
        console.error('Error:', event.message)
        break
    }
  },
)

ws.send('hello')
ws.close()
```

---

## @eric8810/catcher-napi-ws/codec

### 导入

```typescript
import { pack, unpack } from '@eric8810/catcher-napi-ws/codec'
```

### API

| 函数 | 签名 | 说明 |
|------|------|------|
| `pack` | `(value: unknown) => Buffer` | JS 值 → msgpack 字节（Rust rmp-serde） |
| `unpack` | `(data: Buffer) => any` | msgpack 字节 → JS 值（Rust rmp-serde） |

> **注意**：独立调用 pack/unpack 有 NAPI 边界开销（~6x slower than JS msgpackr）。
> 推荐使用 `msgpack: true` 配置，让 transport 层在 Rust 内部完成编解码，无跨边界开销。

---

## 迁移指南（从旧版 hand-written wrapper）

### HTTP

```diff
- const { HttpClient } = require('@eric8810/catcher-napi-http')
- const client = new HttpClient(JSON.stringify({ base_url: '...' }))
+ import { HttpClient } from '@eric8810/catcher-napi-http'
+ const client = new HttpClient({ base_url: '...' })
```

### WebSocket

```diff
- const { WsClient } = require('@eric8810/catcher-napi-ws')
- const ws = new WsClient(JSON.stringify(config), (eventJson) => {
-   const event = JSON.parse(eventJson)
-   console.log(event.data)
+ import { WsClient } from '@eric8810/catcher-napi-ws'
+ const ws = new WsClient(config, (event) => {
+   // event 已自动解析为强类型对象
+   if (event.type === 'Message') {
+     console.log(Buffer.from(event.data_base64, 'base64').toString())
+   }
})
```

**主要变化**：
- `configJson: string` → `config: XxxConfig | string`
- 回调参数从 JSON 字符串变为解析后的强类型对象
- `event.data` → `event.data_base64`（base64 编码）
- 类名 `JsHttpClient`/`JsWsClient` → `HttpClient`/`WsClient`

---

## 平台支持

| 平台 | Target |
|------|--------|
| Linux x64 gnu | `linux-x64-gnu` |
| Linux x64 musl | `linux-x64-musl` |
| macOS arm64 | `darwin-arm64` |
| macOS x64 | `darwin-x64` |
| Windows x64 | `win32-x64-msvc` |
