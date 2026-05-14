# @eric8810/catcher-http

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-http)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient HTTP client for **Node.js** — retry, circuit breaker, priority queue, interceptors, SSE streaming. Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

Built on **axios** (optional peer dep) + **cockatiel** (circuit breaker) + **p-retry** + **p-queue**.

## Install

```bash
npm install @eric8810/catcher-http
# axios is an optional peer dependency
npm install axios
```

## Quick Start

```typescript
import { createHttpClient } from '@eric8810/catcher-http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  keepAlive: true,
  retry: { attempts: 3, backoff: 'exponential', minTimeout: 500 },
  concurrency: 10,
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
})

// Basic requests
const user = await client.get('/users/1')
const created = await client.post('/messages', { text: 'hello' })

// Per-request overrides
await client.get('/analytics', { retry: false, timeout: 5000 })

// Dynamic interceptors
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})
```

## SSE Streaming

### One-shot stream (e.g. OpenAI)

```typescript
import { createSSEStream } from '@eric8810/catcher-http'

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
```

### Long-lived push with auto-reconnect

```typescript
import { createSSEClient } from '@eric8810/catcher-http'

const sse = createSSEClient({
  url: 'https://api.example.com/events',
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})

for await (const line of sse) {
  if (line.startsWith('data: ')) console.log(line.slice(6))
}
```

## API

### `createHttpClient(config)`

Resilience layers (inside → out): `axios → retry → circuit breaker → concurrency queue`

```typescript
interface HttpClientConfig {
  baseURL?: string
  timeout?: number
  keepAlive?: boolean
  retry?: RetryOptions | false
  concurrency?: number
  circuitBreaker?: { failureThreshold: number; resetTimeout: number }
  interceptors?: { request?: Function[]; response?: [Function?, Function?] }
  auth?: { username: string; password: string }
  bearerToken?: string | (() => Promise<string>)
  // ... more options
}
```

### Client Methods

| Method | Priority |
|--------|----------|
| `client.get(url, config?)` | 3 (low) |
| `client.put(url, body?, config?)` | 2 |
| `client.patch(url, body?, config?)` | 2 |
| `client.post(url, body?, config?)` | 1 (high) |
| `client.delete(url, config?)` | 3 |
| `client.circuitBreakerState()` | `'closed' \| 'open' \| 'half-open'` |
| `client.queueDepth()` | Current queue size |
| `client.on(event, listener)` | Subscribe to events |
| `client.updateConfig(updates)` | Hot-update retry/timeout at runtime |

### Additional Exports

| Export | Description |
|--------|-------------|
| `createRetryWrapper(fn, retryOpts)` | Wrap any async function with retry |
| `createInterceptorManager()` | Standalone interceptor chain |
| `createSharedAgent(opts)` | TCP keep-alive + DNS cache agent |
| `clearDnsCache()` | Clear the shared DNS cache |
| `createPriorityQueue(opts)` | Priority-based concurrency queue |
| `createSSEStream(opts)` | One-shot SSE async iterable |
| `createSSEClient(opts)` | Long-lived SSE with auto-reconnect |

## License

MIT
