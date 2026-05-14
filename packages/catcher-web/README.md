# @eric8810/catcher-web

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-web.svg)](https://www.npmjs.com/package/@eric8810/catcher-web)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient HTTP + WebSocket + SSE client for **browsers** — built on **fetch** with retry, circuit breaker, and priority queue. Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

API is intentionally identical to `@eric8810/catcher-http` (Node.js) so you can share the same config across environments.

## Install

```bash
npm install @eric8810/catcher-web
```

## Quick Start

### HTTP

```typescript
import { createWebClient } from '@eric8810/catcher-web'

const client = createWebClient({
  baseURL: 'https://api.example.com',
  retry: { attempts: 3, backoff: 'exponential', minTimeout: 500 },
  concurrency: 6,
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
})

const data = await client.get('/users/1')
const result = await client.post('/messages', { text: 'hello' })

// Interceptors
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})

// Events
client.on('requestComplete', (e) => {
  console.log(`${e.method} ${e.url} → ${e.status} (${e.durationMs}ms)`)
})

// Runtime config update
client.updateConfig({ retry: { attempts: 5 } })
```

### WebSocket

```typescript
import { createWebSocketClient } from '@eric8810/catcher-web'

const ws = createWebSocketClient({
  url: 'wss://echo.example.com',
  reconnect: { initialDelay: 1000, maxDelay: 30_000, maxAttempts: 20 },
})

ws.addEventListener('open', () => ws.send('hello'))
ws.addEventListener('message', (e) => console.log(e.data))
ws.addEventListener('close', (e) => console.log('Closed', e.code))

// Close
ws.close(1000, 'done')
```

### SSE

```typescript
import { createSSEStream, createSSEClient } from '@eric8810/catcher-web'

// One-shot stream
const stream = createSSEStream({
  url: '/api/chat',
  method: 'POST',
  body: { prompt: 'Hello', stream: true },
})
for await (const line of stream) {
  if (line.startsWith('data: ')) console.log(line.slice(6))
}

// Long-lived with auto-reconnect
const client = createSSEClient({
  url: '/api/events',
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})
for await (const line of client) {
  console.log(line)
}
```

## API

### `createWebClient(config) → IHttpClient`

Resilience layers: `fetch → retry → circuit breaker → concurrency queue`

Supports all `HttpClientConfig` options including:
- `auth` / `bearerToken` — automatic auth headers
- `xsrfCookieName` / `xsrfHeaderName` — XSRF protection
- `credentials` — CORS credentials policy
- `redirect` — redirect following control
- `responseType` — `'json'` | `'text'` | `'bytes'` | `'stream'` (ReadableStream)

### `createWebSocketClient(options) → WebSocketClient`

| Property | Description |
|----------|-------------|
| `url` | Single URL or array for fallback |
| `reconnect` | Exponential backoff with jitter |
| `binaryType` | `'blob'` (default) or `'arraybuffer'` |

### `createSSEStream(opts)` / `createSSEClient(opts)`

Browser-compatible SSE via `fetch` + `ReadableStream` parsing.

## Browser Features

- **No axios dependency** — pure `fetch()`
- **FormData support** — automatic body serialization
- **CORS/Credentials** — configurable `mode` and `credentials`
- **ReadableStream** — native streaming for large responses
- **XSRF** — automatic cookie → header injection

## License

MIT
