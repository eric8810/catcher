# 00 — TS 包概览

> v0.3.0 — 新增 SSE 支持，面向 AI 流式响应场景

## 依赖关系

```
            @eric8810/catcher-core (zero deps)
            /         |         \
           /          |          \
  @eric8810/catcher-http  @eric8810/catcher-ws  @eric8810/catcher-web
  (axios + SSE, Node)  (ws, msgpack) (fetch + SSE, Browser)
```

## 按场景安装

```bash
# REST API (Node.js)
npm i @eric8810/catcher-http

# IM + 实时通信 (Node.js)
npm i @eric8810/catcher-http @eric8810/catcher-ws

# 浏览器
npm i @eric8810/catcher-web

# Node.js native (Rust via napi-rs)
npm i @eric8810/catcher-napi-http @eric8810/catcher-napi-ws
```

## 使用方式

```typescript
// HTTP
import { createHttpClient } from '@eric8810/catcher-http'
const client = createHttpClient({ baseURL: 'https://api.example.com' })

// WebSocket
import { createResilientWS } from '@eric8810/catcher-ws'
const ws = createResilientWS({
  url: 'wss://ws.example.com',
  codec: 'msgpack',  // 开关：'json' (默认) | 'msgpack'
})
ws.send({ event: 'message', data: 'hello' })  // 内部自动 pack

// SSE — AI 流式响应（一次性消费）
import { createSSEStream } from '@eric8810/catcher-http'
const stream = createSSEStream({
  url: 'https://api.openai.com/v1/chat/completions',
  method: 'POST',
  headers: { Authorization: `Bearer ${apiKey}` },
  body: { model: 'gpt-4', messages: [{ role: 'user', content: 'Hello' }], stream: true },
})
for await (const event of stream) {
  if (event.data === '[DONE]') break  // 业务逻辑：自行判断终止
  const chunk = JSON.parse(event.data)  // 业务逻辑：自行解析
  process.stdout.write(chunk.choices[0]?.delta?.content ?? '')
}

// SSE — 长连接推送（自动重连 + 断点续传）
import { createSSEClient } from '@eric8810/catcher-http'
const sse = createSSEClient({
  url: 'https://api.example.com/events',
  headers: { Authorization: 'Bearer xxx' },
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})
sse.addEventListener('message', (e) => console.log(e.data))

// 类型
import type { HttpClientConfig, ResilientWSOptions, SSEClientConfig, SSEStreamOptions } from '@eric8810/catcher-core'
```
