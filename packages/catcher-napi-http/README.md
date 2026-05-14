# @eric8810/catcher-napi-http

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-napi-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-http)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Rust-powered HTTP client** for Node.js via [napi-rs](https://napi.rs). Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

Wraps `catcher-http`'s `HttpTransport` — reqwest + retry + circuit breaker, compiled to a native addon.

## Install

```bash
npm install @eric8810/catcher-napi-http
```

Pre-built binaries available for Linux (x64/arm64), macOS (x64/arm64), and Windows (x64).

## Usage

```javascript
const { JsHttpClient } = require('@eric8810/catcher-napi-http')

const client = new JsHttpClient(JSON.stringify({
  base_url: 'https://api.example.com',
  connect_timeout_ms: 5000,
  response_timeout_ms: 30000,
  retry: { max_attempts: 3, backoff: 'Exponential' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
}))

// GET
const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

// POST
const created = await client.post('/messages', Buffer.from(JSON.stringify({ text: 'hello' })), {
  content_type: 'application/json',
})

// Circuit breaker state
console.log(client.circuit_breaker_state()) // "closed" | "open" | "half-open"
```

## API

### `new JsHttpClient(configJson: string)`

Create a client from a JSON config string. All fields are optional with sensible defaults.

```typescript
interface HttpClientConfig {
  base_url?: string
  connect_timeout_ms?: number
  response_timeout_ms?: number
  pool?: { keep_alive?: boolean; max_idle_per_host?: number }
  retry?: { max_attempts?: number; backoff?: 'Fixed' | 'Exponential' | 'DecorrelatedJitter' }
  circuit_breaker?: { failure_threshold?: number; reset_timeout_ms?: number }
}
```

### Methods

| Method | Signature |
|--------|-----------|
| `get(url, options?)` | `async (url: string, options?: RequestOptions) => JsHttpResponse` |
| `post(url, body?, options?)` | `async (url: string, body?: Buffer, options?: RequestOptions) => JsHttpResponse` |
| `put(url, body?, options?)` | `async (url: string, body?: Buffer, options?: RequestOptions) => JsHttpResponse` |
| `delete(url, options?)` | `async (url: string, options?: RequestOptions) => JsHttpResponse` |
| `patch(url, body?, options?)` | `async (url: string, body?: Buffer, options?: RequestOptions) => JsHttpResponse` |
| `circuit_breaker_state()` | `() => string` |

### `JsHttpResponse`

```typescript
interface JsHttpResponse {
  status: number
  headers: Record<string, string>
  body: Buffer
  elapsed_ms: number
}
```

### `RequestOptions`

```typescript
interface RequestOptions {
  headers?: Record<string, string>
  timeout_ms?: number
  content_type?: string
}
```

## Build from Source

Requires Rust toolchain.

```bash
npm run build
```

## License

MIT
