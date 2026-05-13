# 10 — Server-Sent Events (SSE) 客户端

> 新增模块 · 设计文档
> 基于 WHATWG Server-Sent Events 规范，面向 AI 流式响应场景

## 需求背景

AI 大模型 API（OpenAI / Anthropic / Google Gemini）的 chat completion 普遍采用 SSE 作为流式输出协议。SSE 在 AI 场景中优于 WebSocket 的关键点：

| 维度 | SSE | WebSocket |
|------|-----|-----------|
| 协议 | 基于 HTTP，无需升级 | 独立协议升级 |
| 方向 | 服务端 → 客户端（单向） | 双向 |
| 断点续传 | `Last-Event-ID` 原生支持 | 需自行实现 |
| AI 场景匹配 | 请求 → 流式响应，天然契合 | 过度设计 |
| 行业标准 | OpenAI / Anthropic / Gemini 全部采用 | 少数场景使用 |

### SSE 协议格式（WHATWG 规范）

```
event: message_start
id: msg_001
data: {"type":"message_start","message":{"id":"msg_01","model":"gpt-4"}}

data: {"choices":[{"delta":{"content":"Hello"}}]}

data: {"choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

字段规范：

| 字段 | 格式 | 用途 |
|------|------|------|
| `data:` | `data: <payload>\n` | 事件数据，可多行拼接 |
| `event:` | `event: <type>\n` | 事件类型，默认 `"message"` |
| `id:` | `id: <string>\n` | 事件 ID，用于断点续传 |
| `retry:` | `retry: <ms>\n` | 服务端建议重连间隔 |
| 空行 | `\n` | 事件分隔符 |
| `: comment` | `: <text>\n` | 注释（心跳保活） |

- MIME 类型：`text/event-stream`
- 字符编码：UTF-8
- 浏览器原生 `EventSource` 限制：仅 GET、无自定义 headers、无 body
- AI 场景需 POST + JSON body + Auth headers → 使用 `fetch` + `ReadableStream` 手动解析

## 包归属

**SSE 不创建独立 npm 包**，而是作为 `catcher-http-ts` 和 `catcher-web` 的内置能力：

```
                @eric8810/catcher-core (zero deps)
                /         |         \
               /          |          \
  @eric8810/catcher-http  @eric8810/catcher-ws  @eric8810/catcher-web
  (axios, Node)           (ws, msgpack)         (fetch, Browser)
       │                                              │
       └── SSE 支持                                   └── SSE 支持
          fetch() + ReadableStream                      fetch() + ReadableStream
```

理由：
1. SSE 本质是 HTTP 的扩展用法，与 HTTP 客户端共享连接层（Agent / DNS）
2. SSE 复用现有韧性层（重试、熔断、拦截器）
3. 避免多一个包的维护负担
4. 类型定义放在 `catcher-core-ts`，与 `HttpClientConfig` 等同级

## 核心导出

### `createSSEClient(config) → ISSEClient`

长连接场景（实时通知、日志流、消息推送）：

```typescript
import { createSSEClient } from '@eric8810/catcher-http'

const sse = createSSEClient({
  url: 'https://api.example.com/events',
  headers: { Authorization: 'Bearer xxx' },
  reconnect: {
    enabled: true,
    maxRetries: 20,
    initialDelay: 1000,
    maxDelay: 30_000,
    backoffMultiplier: 2,
  },
  circuitBreaker: {
    failureThreshold: 5,
    resetTimeout: 30_000,
  },
})

// 监听默认 message 事件
sse.addEventListener('message', (event) => {
  console.log(event.data)
})

// 监听具名事件
sse.addEventListener('update', (event) => {
  console.log('update:', event.data)
})

// 监听连接状态
sse.addEventListener('open', () => console.log('connected'))
sse.addEventListener('error', (event) => console.error('error:', event))

// 关闭连接（不再重连）
sse.close()
```

### `createSSEStream(options) → SSEStream`

一次性消费场景（AI chat completion）：

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

// OpenAI 兼容格式
const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${apiKey}`,
  },
  body: {
    model: 'gpt-4',
    messages: [{ role: 'user', content: 'Hello' }],
    stream: true,
  },
  timeout: 60_000,
})

for await (const event of stream) {
  if (event.data === '[DONE]') break
  const chunk = JSON.parse(event.data)
  process.stdout.write(chunk.choices[0]?.delta?.content ?? '')
}
```

```typescript
// 浏览器端（相同 API）
import { createSSEStream } from '@eric8810/catcher-web'

const stream = createSSEStream({
  url: '/api/chat',
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ prompt: 'Hello', stream: true }),
})

for await (const event of stream) {
  if (event.data === '[DONE]') break
  const chunk = JSON.parse(event.data)
  document.getElementById('output').textContent += chunk.choices[0]?.delta?.content ?? ''
}
```

## 类型定义

### SSEClientConfig

```typescript
interface SSEClientConfig {
  /** SSE 端点 URL */
  url: string
  /** HTTP 方法，默认 'GET'。AI 场景通常用 'POST' */
  method?: 'GET' | 'POST'
  /** 请求 headers（如 Authorization） */
  headers?: Record<string, string>
  /** 请求 body（POST 场景） */
  body?: string | Record<string, unknown>
  /** 连接层选项 */
  keepAlive?: boolean
  dnsCacheTtl?: number
  rejectUnauthorized?: boolean

  /** 自动重连配置 */
  reconnect?: {
    enabled?: boolean          // 默认 true
    maxRetries?: number        // 默认 Infinity
    initialDelay?: number      // 默认 1000
    maxDelay?: number          // 默认 30_000
    backoffMultiplier?: number // 默认 2
  }

  /** 重试配置（连接阶段错误） */
  retry?: RetryOptions

  /** 熔断配置 */
  circuitBreaker?: {
    failureThreshold: number
    resetTimeout: number
  }

  /** 请求超时 ms，默认 30_000 */
  timeout?: number
}
```

### SSEEvent

```typescript
interface SSEEvent {
  /** 事件类型，默认 'message' */
  event: string
  /** 事件数据（原始字符串） */
  data: string
  /** 事件 ID */
  id?: string
  /** 重连间隔（服务端建议） */
  retry?: number
}
```

### ISSEClient

```typescript
interface ISSEClient extends EventTarget {
  readonly readyState: 'CONNECTING' | 'OPEN' | 'CLOSED'
  readonly lastEventId: string

  addEventListener(type: string, listener: (event: SSEEvent) => void): void
  removeEventListener(type: string, listener: (event: SSEEvent) => void): void
  close(): void
}
```

### SSEStream

```typescript
interface SSEStream extends AsyncIterable<SSEEvent> {
  readonly lastEventId: string
  cancel(): void
}
```

## 源文件结构

```
packages/catcher-http-ts/src/
├── sse/
│   ├── parser.ts       # SSE 文本流解析器（~100 行）
│   ├── client.ts       # ISSEClient 实现：长连接 + 自动重连（~200 行）
│   ├── stream.ts       # SSEStream 实现：一次性消费（~120 行）
│   └── index.ts        # 统一导出
├── http/               # 现有
├── agent/              # 现有
├── queue/              # 现有
└── index.ts            # 更新：增加 SSE 导出

packages/catcher-web/src/
├── sse/
│   ├── parser.ts       # 同上解析器
│   ├── client.ts       # 浏览器版（~180 行）
│   ├── stream.ts       # 浏览器版（~100 行）
│   └── index.ts
├── http/               # 现有
├── ws/                 # 现有
└── index.ts            # 更新：增加 SSE 导出
```

## SSE 解析器 (`parser.ts`)

核心类，两个包共享相同逻辑：

```typescript
export class SSEParser {
  private dataBuffer = ''
  private eventTypeBuffer = ''
  private lastEventIdBuffer = ''

  /**
   * 处理一行文本。
   * 空行 → 组装事件返回 SSEEvent
   * 非空行 → 解析 data/event/id/retry 字段，返回 null
   */
  processLine(line: string): SSEEvent | null

  /** 重置缓冲区 */
  reset(): void
}
```

解析逻辑遵循 WHATWG 规范：
- `data:` 行 → 追加到 dataBuffer + 追加 `\n`
- `event:` 行 → 设置 eventTypeBuffer
- `id:` 行 → 设置 lastEventIdBuffer（不含 NULL 字符）
- `retry:` 行 → 解析为整数，更新重连间隔
- 空行 → 组装事件：data 去尾 `\n`，event 默认 `"message"`
- `:` 开头 → 注释行，忽略

## 重连机制

```
createSSEClient()
  │
  ├─ fetch(url, { headers, body, signal })
  │    │
  │    ├─ 200 OK + text/event-stream
  │    │    ├─ 逐行读取 → SSEParser.processLine() → emit events
  │    │    ├─ 记录 lastEventId（用于断点续传）
  │    │    └─ 流结束 → scheduleReconnect()
  │    │
  │    ├─ 网络错误 → scheduleReconnect()
  │    ├─ 204 → close()（服务端要求停止重连）
  │    ├─ 301/307 → 跟随重定向
  │    └─ 其他错误 → error event → scheduleReconnect()
  │
  ▼ scheduleReconnect()
  ├─ delay = initialDelay × multiplier^(attempt-1) + randomJitter(±25%)
  ├─ delay = min(delay, maxDelay)
  ├─ headers['Last-Event-ID'] = lastEventId
  ├─ attempt++ ≤ maxRetries ? → 重新 fetch() : → close()
  └─ 成功连接 → attempt = 0, reset()
```

重连延迟表（默认参数 initialDelay=1000, multiplier=2, maxDelay=30000）：

| attempt | 延迟 |
|---------|------|
| 1 | ~1000ms ±250ms |
| 2 | ~2000ms ±500ms |
| 3 | ~4000ms ±1000ms |
| 4 | ~8000ms ±2000ms |
| 5 | ~16000ms ±4000ms |
| 6+ | ~30000ms ±7500ms (cap) |

与 `catcher-ws` 的重连策略完全一致。

## 状态机

```
          ┌──────────────┐
          │  CONNECTING  │ ← 创建时
          └──────┬───────┘
                 │ fetch() 响应成功 + Content-Type: text/event-stream
          ┌──────▼───────┐
          │    OPEN       │ ← 发出 'open' 事件
          └──────┬───────┘
                 │ 连接断开 / 流结束 / 网络错误
          ┌──────▼───────┐
          │ RECONNECTING │ ← 指数退避等待 → 重新 fetch()
          └──────┬───────┘
                 │ maxRetries 耗尽 或 close()
          ┌──────▼───────┐
          │   CLOSED     │ ← 发出 'close' 事件，不再重连
          └──────────────┘
```

## 韧性层次（SSE 场景）

```
                          ┌──────────────────┐
                          │   调用方代码       │
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  SSE Parser       │  ← 逐行解析 data/event/id/retry
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Reconnect Layer  │  ← 指数退避 + Last-Event-ID 续传
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Circuit Breaker  │  ← cockatiel 熔断保护
                          └────────┬─────────┘
                                   │
                          ┌────────▼─────────┐
                          │  Retry Wrapper    │  ← 连接阶段错误重试
                          └────────┬─────────┘
                                   │
       ┌───────────────────────────┼───────────────────────────┐
       │  Node.js                  │                           │  Browser
       │  fetch() + SharedAgent    │                           │  fetch() (native)
       │  (连接池 + DNS 缓存)       │                           │
       └───────────────────────────┘                           └─────────┘
```

## 与现有模块的关系

| 复用模块 | 复用方式 |
|---------|---------|
| `SharedAgent` | SSE 连接复用连接池和 DNS 缓存（Node.js） |
| `createRetryWrapper` | 连接阶段错误重试（网络不可达、DNS 失败等） |
| `CircuitBreakerPolicy` | SSE 端点级别的熔断保护 |
| `SSEParser` | 独立实现，无外部依赖 |
| `types` | 类型定义在 `catcher-core-ts` |

## 依赖

| 依赖 | 用途 | 新增？ |
|------|------|--------|
| Node 18+ `fetch` | HTTP 请求 + ReadableStream | 否（内置） |
| Browser `fetch` | HTTP 请求 + ReadableStream | 否（内置） |
| `cockatiel` | 熔断器 | 否（现有） |
| `p-retry` | 连接重试 | 否（现有） |

**零新增依赖。**

## 与 catcher-ws 的对比

| 维度 | catcher-ws | catcher-sse |
|------|-----------|-------------|
| 协议 | WebSocket（双向） | HTTP SSE（单向） |
| 底层 | `ws` 库 | `fetch` + `ReadableStream` |
| 数据格式 | 二进制（msgpack）/ 文本 | 纯文本（`text/event-stream`） |
| 重连 | `createReconnectStrategy` | 相同策略 |
| 多端点 | `raceEndpoints` 竞速 | 暂不支持（可后续添加） |
| AI 场景 | 不适合（双向过度） | 核心场景 |
| 浏览器支持 | `catcher-web/src/ws/` | `catcher-web/src/sse/` |

## 不涉及的范围

- **不创建新 npm 包** — SSE 是 HTTP 的扩展能力
- **不修改 Rust 侧** — SSE 纯 TS 层，不涉及 reqwest / FFI
- **不修改 napi 包** — SSE 不走 Rust native 路径
- **不支持 IE** — 需要 `fetch` + `ReadableStream`
- **不实现 SSE 服务端** — Catcher 是客户端库
