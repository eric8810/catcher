# @eric8810/catcher-core

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-core.svg)](https://www.npmjs.com/package/@eric8810/catcher-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Shared TypeScript type definitions** for the [catcher](https://github.com/eric8810/catcher) toolkit. Zero runtime dependencies.

This package is consumed by `@eric8810/catcher-http`, `@eric8810/catcher-ws`, and `@eric8810/catcher-web`. You typically don't install it directly.

> **💡 Node.js users**: For Rust-native performance, use [`@eric8810/catcher-napi-http`](https://www.npmjs.com/package/@eric8810/catcher-napi-http) and [`@eric8810/catcher-napi-ws`](https://www.npmjs.com/package/@eric8810/catcher-napi-ws) instead. They provide typed TS wrappers with the same config schema and much better throughput.

## Install

```bash
npm install @eric8810/catcher-core
```

## Exported Types

### HTTP

| Type | Description |
|------|-------------|
| `IHttpClient` | HTTP client interface (`get`, `post`, `put`, `delete`, `patch`) |
| `HttpClientConfig` | Client configuration |
| `RequestConfig` | Per-request options |
| `HttpResponse` | Response shape |
| `ProgressEvent` | Upload/download progress |
| `InterceptorManager` | Request/response interceptor API |

### Error

| Type | Description |
|------|-------------|
| `CatcherHttpError` | Enhanced error with type, request, response, attempt info |
| `CatcherErrorType` | `'connection' \| 'timeout' \| 'http' \| 'cancelled' \| 'unknown'` |
| `isCatcherError()` | Type guard for CatcherHttpError |

### Network

| Type | Description |
|------|-------------|
| `ProxyConfig` | HTTP/HTTPS proxy settings |
| `DnsConfig` | DNS resolution options |
| `TlsConfig` | TLS/SSL settings |
| `RedirectInfo` | Redirect policy |
| `TransportAdapter` | Custom transport adapter |

### SSE

| Type | Description |
|------|-------------|
| `SSEStream` | AsyncIterable SSE line stream |
| `SSEClient` | Long-lived SSE connection with auto-reconnect |
| `SSEStreamOptions` | Stream configuration |
| `SSEClientOptions` | Client configuration |
| `SSETimeoutError` | Timeout error type |

### Events

| Type | Description |
|------|-------------|
| `ClientEvent` | Event payload types (`requestComplete`, `retry`, etc.) |

## Usage

```typescript
import type { IHttpClient, HttpClientConfig, CatcherHttpError } from '@eric8810/catcher-core'
import { isCatcherError } from '@eric8810/catcher-core'

try {
  const data = await client.get('/api/data')
} catch (err) {
  if (isCatcherError(err)) {
    console.log(err.type, err.attempt, err.elapsedMs)
  }
}
```

## License

MIT
