# @eric8810/catcher-napi-http

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-napi-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-http)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Rust-powered HTTP + SSE client** for Node.js via [napi-rs](https://napi.rs). Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

Wraps `catcher-http`'s `HttpTransport` — reqwest + retry + circuit breaker, compiled to a native addon. Includes typed TypeScript wrappers with auto-generated `.d.ts`.

## ⚠️ Breaking Changes (0.3.0)

> **Migrate from 0.2.x → 0.3.x** — see [napi API docs](https://github.com/eric8810/catcher/blob/master/docs/user-manual/api/napi.md) for full details.

| Change | Before | After |
|--------|--------|-------|
| Entry point | `client.js` / `client.d.ts` | `dist/client.js` / `dist/client.d.ts` |
| Config format | `JSON.stringify(config)` only | Typed object **or** JSON string |
| Class names | `JsHttpClient`, `JsSseStream`, `JsSseClient` | `HttpClient`, `SseStream`, `SseClient` |
| Callback events | Raw JSON strings, need `JSON.parse()` | Typed objects, auto-parsed |
| Default backoff | `Exponential` | `Fixed` |
| Default `connect_timeout_ms` | `5000` | `10000` |
| Default `min_backoff_ms` | `500` | `100` |
| Default `max_backoff_ms` | `30000` | `10000` |
| camelCase fields | Not supported | `#[serde(alias)]` — both `snake_case` and `camelCase` accepted |

```diff
- const client = require('@eric8810/catcher-napi-http').HttpClient
- const c = new HttpClient(JSON.stringify({ base_url: '...' }))
+ import { HttpClient } from '@eric8810/catcher-napi-http'
+ const c = new HttpClient({ base_url: '...' })
```

## Install

```bash
npm install @eric8810/catcher-napi-http
```

Pre-built binaries available for Linux (x64 gnu/musl), macOS (x64/arm64), and Windows (x64).

## Usage

```typescript
import { HttpClient } from '@eric8810/catcher-napi-http'
import type { HttpClientConfig, SseEvent } from '@eric8810/catcher-napi-http'

// Config as typed object (recommended) or JSON string
const client = new HttpClient({
  base_url: 'https://api.example.com',
  connect_timeout_ms: 10000,
  retry: { max_attempts: 3, backoff: 'Fixed' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
})

// GET
const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

// POST
await client.post('/messages', Buffer.from('hello'), {
  content_type: 'text/plain',
})

// Circuit breaker state
console.log(client.circuitBreakerState()) // 'closed' | 'open' | 'half-open'

// SSE (one-shot stream)
import { SseStream } from '@eric8810/catcher-napi-http'

const stream = new SseStream(
  { url: 'https://stream.example.com/events' },
  (event: SseEvent) => {
    if (event.type === 'Line') console.log(event.data)
  },
)
// later: stream.close()

// SSE (auto-reconnect client)
import { SseClient } from '@eric8810/catcher-napi-http'

const sse = new SseClient(
  {
    url: 'https://stream.example.com/events',
    reconnect: { max_retries: 10, initial_delay_ms: 1000 },
  },
  (event: SseEvent) => {
    if (event.type === 'Line') console.log(event.data)
  },
)
```

## API

### `new HttpClient(config: HttpClientConfig | string)`

Create a client from a typed config object or JSON string. All fields are optional with sensible defaults. Supports both `snake_case` and `camelCase` field names.

```typescript
interface HttpClientConfig {
  base_url?: string
  connect_timeout_ms?: number      // default: 10000
  response_timeout_ms?: number     // default: 30000
  pool?: PoolConfig
  tls?: TlsConfig
  dns?: DnsConfig
  retry?: RetryConfig
  circuit_breaker?: CircuitBreakerConfig
  max_concurrency?: number         // default: 50
  default_headers?: Record<string, string>
  hostname_override?: string
  proxy?: ProxyConfig
  redirect?: RedirectConfig
  auth?: { username: string; password: string }
  bearer_token?: string
}
```

### Methods

| Method | Signature |
|--------|-----------|
| `get(url, options?)` | `async (url: string, options?: RequestOptions) => HttpResponse` |
| `post(url, body?, options?)` | `async (url: string, body?: Buffer, options?: RequestOptions) => HttpResponse` |
| `put(url, body?, options?)` | `async (url: string, body?: Buffer, options?: RequestOptions) => HttpResponse` |
| `delete(url, options?)` | `async (url: string, options?: RequestOptions) => HttpResponse` |
| `patch(url, body?, options?)` | `async (url: string, body?: Buffer, options?: RequestOptions) => HttpResponse` |
| `circuitBreakerState()` | `() => 'closed' \| 'open' \| 'half-open'` |
| `metrics()` | `() => Metrics` |
| `executeStream(method, url, body?, options?, onChunk?)` | `(method: string, url: string, body?: Buffer, options?: RequestOptions, onChunk?: (event: StreamEvent) => void) => void` |
| `setAdaptiveTimeout(min, max, mult, win)` | `(min: number, max: number, mult: number, win: number) => void` |
| `cancelAll()` | `() => void` |

### `HttpResponse`

```typescript
interface HttpResponse {
  status: number
  headers: Record<string, string>
  body: Buffer
  elapsed_ms: number
}
```

### SSE

| Class | Description |
|-------|-------------|
| `SseStream` | One-shot SSE stream (no auto-reconnect) |
| `SseClient` | Long-lived SSE client with auto-reconnect |

```typescript
new SseStream(config: SseClientConfig | string, onEvent: (event: SseEvent) => void)
new SseClient(config: SseClientConfig | string, onEvent: (event: SseEvent) => void)

type SseEvent =
  | { type: 'Line'; data: string }
  | { type: 'Error'; message: string }
  | { type: 'End' }
```

### `StreamEvent`

```typescript
type StreamEvent =
  | { type: 'Headers'; status: number; headers: Record<string, string> }
  | { type: 'Chunk'; data: string }  // base64 encoded
  | { type: 'Done' }
  | { type: 'Error'; message: string }
```

## Build from Source

Requires Rust toolchain.

```bash
npm run build       # napi build + tsup compile
npm run build:ts    # tsup only (no Rust rebuild)
```

## License

MIT
