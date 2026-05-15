# SSE 指南

> Server-Sent Events 完整开发指南 — AI 流式对话 & 服务端推送

---

## 概述

Catcher 提供两种 SSE 模式，分别对应两种典型场景：

| | `createSSEStream` | `createSSEClient` |
|--|:--:|:--:|
| **定位** | 一次性流式请求 | 长连接 + 自动重连 |
| **典型场景** | AI 流式对话（OpenAI / Anthropic / Gemini） | 服务端推送、实时通知、监控仪表盘 |
| **生命周期** | 消费完即结束，不重连 | 断开后自动重连，携带 `Last-Event-ID` |
| **退出方式** | `for await` 自然结束 / `break` / `AbortSignal` | `client.close()` / `AbortSignal` |
| **重连** | ❌ 无 | ✅ 指数退避 + jitter |
| **熔断器** | ❌ 无 | ✅ 可选 `circuitBreaker` |
| **返回类型** | `SSEStream`（`AsyncIterable<string>`） | `SSEClient`（`AsyncIterable<string>`） |
| **可迭代次数** | 仅一次（第二次迭代抛错） | 多次（`close()` 后结束） |
| **可用包** | `@eric8810/catcher-http` / `@eric8810/catcher-web` | `@eric8810/catcher-http` / `@eric8810/catcher-web` |

**选择依据**：如果连接是一次性的请求-响应流（如 AI 对话），用 `createSSEStream`；如果连接需要一直保持存活（如推送通道），用 `createSSEClient`。

---

## 一、createSSEStream — 一次性流式请求

适用于 AI 流式对话场景。连接建立后通过 `for await` 逐行消费内容，流结束即完成，无需手动清理。

### 基本用法

```typescript
import { createSSEStream } from '@eric8810/catcher-http'
// 浏览器端使用 '@eric8810/catcher-web'

const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${apiKey}` },
  body: {
    model: 'gpt-4',
    messages: [{ role: 'user', content: 'Hello' }],
    stream: true,
  },
})

for await (const line of stream) {
  if (!line.startsWith('data:')) continue
  const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)
  if (payload === '[DONE]') break
  const chunk = JSON.parse(payload)
  process.stdout.write(chunk.choices[0]?.delta?.content ?? '')
}
// 循环结束 = 连接关闭，无需手动清理
```

### 参数说明

```typescript
interface SSEStreamOptions {
  url: string                              // SSE 端点 URL
  method?: 'GET' | 'POST'                  // HTTP 方法，默认 'GET'
  headers?: Record<string, string>         // 请求 headers（如 Authorization）
  body?: string | Record<string, unknown>  // 请求 body；对象自动 JSON.stringify
  timeout?: number                         // 超时 ms，默认 30_000
  signal?: AbortSignal                     // 中断信号
}
```

**注意事项**：

- `body` 传入对象时会自动 `JSON.stringify` 并设置 `Content-Type: application/json`；传入字符串则原样发送
- 如果 `headers` 中已有 `Content-Type`，不会被覆盖
- 只能迭代一次，第二次调用 `[Symbol.asyncIterator]()` 会抛错

### 中断请求

```typescript
const controller = new AbortController()

const stream = createSSEStream({
  url: 'https://api.example.com/stream',
  signal: controller.signal,
})

// 5 秒后中断
setTimeout(() => controller.abort(), 5000)

for await (const line of stream) {
  console.log(line) // abort 后 for await 自然结束
}
```

---

## 二、createSSEClient — 长连接 + 自动重连

适用于服务端推送场景。连接断开后自动重连，携带 `Last-Event-ID` 实现断点续传。

### 基本用法

```typescript
import { createSSEClient } from '@eric8810/catcher-http'
// 浏览器端使用 '@eric8810/catcher-web'

const client = createSSEClient({
  url: 'https://api.example.com/events',
  headers: { Authorization: 'Bearer xxx' },
  reconnect: {
    initialDelay: 1000,
    maxDelay: 30_000,
    backoffMultiplier: 2,
  },
})

for await (const line of client) {
  if (line.startsWith('data: ')) {
    const payload = line.slice(6)
    console.log('收到推送:', payload)
  }
}

// 正常情况下永不主动断开。如需断开：
client.close()
```

### 参数说明

`SSEClientOptions` 继承 `SSEStreamOptions`，额外增加：

```typescript
interface SSEClientOptions extends SSEStreamOptions {
  reconnect?: {
    enabled?: boolean          // 是否启用自动重连，默认 true
    maxRetries?: number        // 最大重试次数，默认 Infinity
    initialDelay?: number      // 初始延迟 ms，默认 1000
    maxDelay?: number          // 最大延迟 ms，默认 30_000
    backoffMultiplier?: number // 退避倍数，默认 2
  }
  circuitBreaker?: {
    failureThreshold: number   // 连续失败次数阈值
    resetTimeout: number       // 熔断恢复时间 ms
  }
}
```

### 重连机制

```
createSSEClient()
  │
  ├─ fetch 请求（携带 headers + body）
  │    │
  │    ├─ 200 OK
  │    │    ├─ 读取流 → routeLine() → yield 内容行
  │    │    ├─ id: 行 → 静默记录 lastEventId
  │    │    ├─ retry: 行 → 静默调整重连间隔
  │    │    └─ 流结束 → scheduleReconnect()
  │    │
  │    ├─ 网络错误 → scheduleReconnect()
  │    ├─ 204 → 停止重连（SSE 规范：服务端要求停止）
  │    └─ 其他 HTTP 错误 → scheduleReconnect()
  │
  ▼ scheduleReconnect()
  ├─ delay = initialDelay × multiplier^(attempt-1) ± 25% jitter
  ├─ headers['Last-Event-ID'] = lastEventId（自动携带）
  ├─ attempt ≤ maxRetries ? → 重新请求 : → 停止
  └─ 成功连接 → attempt 重置为 0
```

**重连延迟计算示例**（默认参数：`initialDelay=1000`, `maxDelay=30000`, `multiplier=2`）：

| 重试次数 | 基础延迟 | 实际范围（含 jitter） |
|---------|---------|---------------------|
| 第 1 次 | 1000ms | 750 ~ 1250ms |
| 第 2 次 | 2000ms | 1500 ~ 2500ms |
| 第 3 次 | 4000ms | 3000 ~ 5000ms |
| 第 4 次 | 8000ms | 6000 ~ 10000ms |
| 第 5 次 | 16000ms | 12000 ~ 20000ms |
| 第 6 次+ | 30000ms（封顶） | 22500 ~ 37500ms |

如果服务端发送了 `retry: 5000`，后续重连延迟以 5000ms 为基准（同样带 jitter）。

### 熔断器

配置 `circuitBreaker` 后，连续连接失败达到阈值时触发熔断，停止重连尝试，等待恢复时间后进入半开状态：

```typescript
const client = createSSEClient({
  url: 'https://api.example.com/events',
  circuitBreaker: {
    failureThreshold: 5,    // 连续失败 5 次触发熔断
    resetTimeout: 60_000,   // 60 秒后尝试恢复
  },
})
```

### 状态跟踪

```typescript
const client = createSSEClient({ url: '...' })

console.log(client.readyState) // 'CONNECTING' | 'OPEN' | 'CLOSED'
console.log(client.lastEventId) // 从 id: 行提取的事件 ID

// 手动关闭
client.close() // readyState 变为 'CLOSED'，for await 结束
```

### 204 停止重连

SSE 规范规定：如果服务端返回 HTTP 204，表示服务端要求客户端停止重连。Catcher 遵循此规范，收到 204 后自动停止，`for await` 自然结束。

---

## 三、SSE Router — 行路由机制

Catcher 的 SSE 模块不解析业务数据，只做行路由：将 SSE 文本流按 `\n` 切行，然后根据前缀分类处理。

### 路由规则

```
服务端发送的每一行
    │
    ▼
┌───────────────────────────────────────────┐
│ Line Router                               │
│                                           │
│  空行             → 静默吃掉（事件分隔符）  │
│  : comment        → 静默吃掉（心跳/注释）  │
│  id: msg_001      → 记录 lastEventId      │
│  retry: 5000      → 调整重连间隔          │
│  data: Hello      → 原样 yield 给用户     │
│  event: msg       → 原样 yield 给用户     │
│  其他             → 原样 yield 给用户     │
└───────────────────────────────────────────┘
    │
    ▼
  string（完整一行，不做任何解析或前缀剥离）
```

### 库静默处理的行（不传递给用户）

| 服务端发送 | 库的行为 |
|-----------|---------|
| `: keepalive` | 静默吃掉，重置 idle timer |
| （空行） | 静默吃掉（SSE 事件分隔符） |
| `id: msg_003` | 静默记录 `lastEventId`，重连时自动携带 |
| `retry: 5000` | 静默调整重连间隔 |

这些是 SSE 协议自身的控制信令，类似 HTTP 的 `Content-Length`——库处理掉，用户不需要关心。

### 原样输出给用户的行

| 服务端发送 | 用户拿到的 |
|-----------|-----------|
| `event: message_start` | `"event: message_start"` |
| `data: {"type":"start",...}` | `"data: {\"type\":\"start\",...}"` |
| `data: Hello` | `"data: Hello"` |
| `data:  world` | `"data:  world"` |
| `data: [DONE]` | `"data: [DONE]"` |

用户拿到的是**完整的原始行**——保留 `data:` / `event:` 前缀，不做任何结构化或解析。这种设计让用户可以按自己的需求处理：

```typescript
for await (const line of stream) {
  // 过滤 data 行
  if (!line.startsWith('data:')) continue

  // 手动剥离前缀
  const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)

  // 业务处理
  if (payload === '[DONE]') break
  const data = JSON.parse(payload)
}
```

### Chunk 缓冲

网络层按 chunk 交付数据，不一定按行边界对齐。Catcher 内部维护行缓冲区，保证每次 yield 的是完整的一行：

```
chunk1: "data: Hel"    → 缓冲，不够一行
chunk2: "lo\ndata: "   → yield "data: Hello"，缓冲 "data: "
chunk3: "world\n"      → yield "data: world"
```

同时处理 `\r\n` 和 `\n` 两种行尾（兼容 Windows 换行），以及 UTF-8 多字节字符跨 chunk 的边界问题。

---

## 四、错误处理

### 错误类型

| 错误 | 触发条件 | 场景 |
|------|---------|------|
| `SSETimeoutError` | 指定时间内没有新数据到达 | `createSSEStream` |
| `Error: HTTP {status}` | 服务端返回非 2xx 状态码 | 两种模式 |
| `Error: Aborted` | `AbortSignal` 触发 | 两种模式 |
| `Error: SSEStream can only be iterated once` | 第二次迭代 `SSEStream` | `createSSEStream` |
| `Error: SSE: response body is null` | 响应体不可读（环境不支持 ReadableStream） | 两种模式 |

### SSETimeoutError

当 `timeout` 时间内没有收到任何新数据时抛出：

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

try {
  const stream = createSSEStream({
    url: 'https://api.example.com/stream',
    timeout: 30_000, // 30 秒内没有新数据则超时
  })

  for await (const line of stream) {
    console.log(line)
  }
} catch (err: any) {
  if (err.type === 'SSE_TIMEOUT') {
    console.error('SSE 超时：30 秒内没有收到数据')
  } else {
    throw err
  }
}
```

**注意**：`timeout` 同时控制连接超时和 idle 超时（两次数据之间的最长等待时间）。

### HTTP 错误

```typescript
try {
  const stream = createSSEStream({ url: 'https://api.example.com/stream' })
  for await (const line of stream) { /* ... */ }
} catch (err: any) {
  if (err.message?.startsWith('SSE connection failed: HTTP')) {
    // 服务端返回 4xx / 5xx
    console.error('连接失败:', err.message)
  }
}
```

### createSSEClient 中的错误

`createSSEClient` 的自动重连会吞掉大部分网络错误，以下情况会导致迭代结束：

| 场景 | 行为 |
|------|------|
| 网络波动 / 临时断开 | 自动重连，用户无感知 |
| 达到 `maxRetries` | `for await` 结束 |
| 熔断器打开 | 停止重连，等待恢复 |
| 服务端返回 204 | 停止重连（SSE 规范） |
| `client.close()` | 立即停止 |

---

## 五、实战示例

### OpenAI 兼容的 AI 流式对话

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

async function chat(prompt: string) {
  const stream = createSSEStream({
    url: 'https://api.openai.com/v1/chat/completions',
    method: 'POST',
    headers: { Authorization: `Bearer ${process.env.OPENAI_API_KEY}` },
    body: {
      model: 'gpt-4',
      messages: [{ role: 'user', content: prompt }],
      stream: true,
    },
    timeout: 60_000, // AI 响应可能较慢
  })

  let fullText = ''
  for await (const line of stream) {
    if (!line.startsWith('data:')) continue
    const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)
    if (payload === '[DONE]') break
    const chunk = JSON.parse(payload)
    const content = chunk.choices[0]?.delta?.content ?? ''
    fullText += content
    process.stdout.write(content)
  }
  return fullText
}
```

### Anthropic Claude 流式对话

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

const stream = createSSEStream({
  url: 'https://api.anthropic.com/v1/messages',
  method: 'POST',
  headers: {
    'x-api-key': apiKey,
    'anthropic-version': '2023-06-01',
    'Content-Type': 'application/json',
  },
  body: {
    model: 'claude-sonnet-4-20250514',
    max_tokens: 4096,
    stream: true,
    messages: [{ role: 'user', content: 'Hello' }],
  },
})

for await (const line of stream) {
  if (!line.startsWith('data:')) continue
  const payload = line.slice(6) // "data: ".length === 6
  const event = JSON.parse(payload)
  if (event.type === 'content_block_delta') {
    process.stdout.write(event.delta?.text ?? '')
  }
}
```

### 实时通知推送

```typescript
import { createSSEClient } from '@eric8810/catcher-http'

const client = createSSEClient({
  url: 'https://api.example.com/notifications',
  headers: { Authorization: `Bearer ${token}` },
  reconnect: {
    initialDelay: 1000,
    maxDelay: 30_000,
    backoffMultiplier: 2,
  },
  circuitBreaker: {
    failureThreshold: 5,
    resetTimeout: 60_000,
  },
})

for await (const line of client) {
  if (line.startsWith('event: ')) {
    const eventType = line.slice(7)
    console.log('事件类型:', eventType)
  }
  if (line.startsWith('data: ')) {
    const notification = JSON.parse(line.slice(6))
    showNotification(notification)
  }
}

// 应用退出时
client.close()
```

### 浏览器端 AI 对话

```typescript
import { createSSEStream } from '@eric8810/catcher-web'

const output = document.getElementById('output')!

async function streamChat(prompt: string) {
  const stream = createSSEStream({
    url: '/api/chat', // 同源，无 CORS 问题
    method: 'POST',
    body: { prompt, stream: true },
  })

  for await (const line of stream) {
    if (!line.startsWith('data:')) continue
    const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)
    if (payload === '[DONE]') break
    const chunk = JSON.parse(payload)
    output.textContent += chunk.choices[0]?.delta?.content ?? ''
  }
}
```

---

## 六、最佳实践

### 1. 选择正确的模式

```
你的场景是什么？
    │
    ├─ AI 流式响应（请求 → 流式回复 → 结束）
    │   → createSSEStream
    │
    └─ 需要保持连接（推送通知、实时更新）
        → createSSEClient
```

### 2. 超时设置

- **AI 场景**：模型首次响应可能有较长延迟（思考时间），建议设置 `timeout: 60_000` 或更高
- **推送场景**：`createSSEClient` 的超时仅影响单次连接的空闲检测，重连不受此限制

### 3. 剥离 data 前缀的通用模式

```typescript
for await (const line of stream) {
  if (!line.startsWith('data:')) continue
  // 兼容 "data:xxx" 和 "data: xxx" 两种格式
  const payload = line.startsWith('data: ') ? line.slice(6) : line.slice(5)
  // ...
}
```

### 4. 错误重试（createSSEStream）

`createSSEStream` 不自动重连。如果需要重试，可以结合 try-catch 和外部重试逻辑：

```typescript
async function resilientStream(options: SSEStreamOptions, maxRetries = 3) {
  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const stream = createSSEStream(options)
      for await (const line of stream) {
        yield line
      }
      return // 正常完成
    } catch (err: any) {
      if (err.type === 'SSE_TIMEOUT' && attempt < maxRetries) {
        await new Promise(r => setTimeout(r, 1000 * (attempt + 1)))
        continue
      }
      throw err
    }
  }
}
```

### 5. 资源清理

- `createSSEStream`：`for await` 结束 = 连接关闭，无需手动清理
- `createSSEClient`：务必在不需要时调用 `client.close()`，否则后台连接会持续存在

```typescript
// 应用关闭时清理
process.on('SIGTERM', () => {
  client.close()
  process.exit(0)
})
```

### 6. 事件路由（多事件类型）

服务端可能发送不同类型的 `event:` 行，用户可以在 `data:` 行前观察 `event:` 行来做路由：

```typescript
let currentEvent = ''
for await (const line of client) {
  if (line.startsWith('event: ')) {
    currentEvent = line.slice(7)
  } else if (line.startsWith('data: ')) {
    const payload = line.slice(6)
    switch (currentEvent) {
      case 'message': handleMessage(payload); break
      case 'notification': handleNotification(payload); break
      default: console.log('未知事件:', currentEvent, payload)
    }
    currentEvent = ''
  }
}
```

### 7. 浏览器 CORS 注意事项

浏览器端 SSE 依赖 `fetch` + `ReadableStream`，需要服务端配合 CORS：

- 服务端返回 `Access-Control-Allow-Origin`
- 如需自定义 headers（如 `Authorization`），服务端需支持 preflight（`OPTIONS` 请求）
- 最低浏览器版本：Chrome 43+, Firefox 65+, Safari 10.1+
