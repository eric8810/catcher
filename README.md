# Catcher 🪤

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-http)
[![pub version](https://img.shields.io/pub/v/catcher_core.svg)](https://pub.dev/packages/catcher_core)
[![crates.io](https://img.shields.io/crates/v/catcher-http.svg)](https://crates.io/crates/catcher-http)
[![CI](https://github.com/eric8810/catcher/actions/workflows/ci.yml/badge.svg)](https://github.com/eric8810/catcher/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient network communication toolkit — Rust core, TypeScript wrappers, Flutter bindings, cross-platform.

> "Catcher" — catches network failures before they reach your business logic.

## ⚠️ Breaking Changes (0.3.0+)

### napi packages (Node.js native)

| Change | Before | After |
|--------|--------|-------|
| Entry point | `client.js` | `dist/client.js` |
| Config | JSON string only | Typed object or JSON string |
| Class names | `JsHttpClient`, `JsWsClient` | `HttpClient`, `WsClient` |
| Callback events | JSON strings | Typed objects (auto-parsed) |
| WS Message data | `event.data` | `event.data_base64` (base64) |
| NAPI-RS types | — | camelCase fields (`elapsedMs`, `timeoutMs`, etc.) |
| TLS (wss://) | ❌ napi-ws broken | ✅ rustls built-in (v0.3.2) |

### Rust crates

| Change | Before | After |
|--------|--------|-------|
| `BackoffKind::default()` | `Exponential` | `Fixed` |
| `RetryConfig::default().backoff` | `Exponential` | `Fixed` |
| WS `deflate_threshold` | — | Renamed → `deflate_threshold_bytes` |
| WS `max_message_size` | — | Renamed → `max_payload_bytes` |
| WS `ping_timeout_ms` | — | Renamed → `pong_timeout_ms` |
| All config structs | `snake_case` only | `snake_case` + `camelCase` via `#[serde(alias)]` |

See package-specific READMEs for detailed migration instructions.

## Platform Coverage

| Platform | Package | Status |
|----------|---------|--------|
| **Node.js (native)** | `@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws` | ✅ ⭐ 推荐 |
| **Node.js (TS)** | `@eric8810/catcher-http` / `@eric8810/catcher-ws` | ✅ Published |
| **Electron** | same as Node.js | ✅ |
| **Web** | `@eric8810/catcher-web` | ✅ Published |
| **Rust** | `catcher-http` / `catcher-ws` / `catcher-core` | ✅ Published |
| **Flutter** | `catcher_core` (dart:ffi) | ✅ Published |
| **Android + iOS** | `catcher-uniffi` (UniFFI) | ✅ Published |

## Architecture

```
catcher-core (Rust)              @eric8810/catcher-core (TS)
     │                                │
 ┌───┴───┐                        ┌───┴───┐
 ▼       ▼                        ▼       ▼
catcher  catcher              @eric8810  @eric8810
-http    -ws                  /catcher-  /catcher-
 │  │     │  │                http       ws
 │  │     │  │               (axios)    (ws)
 │  │     │  │               (+SSE)
 │  └──napi-rs──┐   ┌──napi-rs──┘
 │              ▼   ▼
 │        @eric8810/catcher-napi-http  ⭐ Node.js 推荐
 │        @eric8810/catcher-napi-ws    ⭐ Node.js 推荐
 │        (typed TS wrappers + .node)
 │
 │   ┌─────────────────────────────────┐
 ├───┤ catcher-ffi (cdylib umbrella)   │
 │   │  bridges catcher-http + ws      │
 │   │  exports 16 C ABI symbols       │
 │   └──────┬──────────┬───────────────┘
 │          ▼          ▼
 │    dart:ffi      UniFFI
 │    (Flutter)   (Kotlin/Swift)
 │          │          │
 │   catcher_core  catcher-uniffi
 └─────────────────────────────────┘

                          ┌─────────────────────┐
                          │ @eric8810/catcher-web│
                          │  (Browser, fetch)    │
                          │  HTTP + SSE          │
                          └─────────────────────┘
```

## Packages

### npm (@eric8810 scope)

| Package | Version | Description |
|---------|---------|-------------|
| [`@eric8810/catcher-core`](https://www.npmjs.com/package/@eric8810/catcher-core) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-core.svg)](https://www.npmjs.com/package/@eric8810/catcher-core) | Shared TS type definitions |
| [`@eric8810/catcher-http`](https://www.npmjs.com/package/@eric8810/catcher-http) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-http) | HTTP + SSE client — retry, CB, queue, interceptors |
| [`@eric8810/catcher-ws`](https://www.npmjs.com/package/@eric8810/catcher-ws) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-ws) | WebSocket — reconnect, multi-endpoint, codec |
| [`@eric8810/catcher-web`](https://www.npmjs.com/package/@eric8810/catcher-web) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-web.svg)](https://www.npmjs.com/package/@eric8810/catcher-web) | Browser HTTP + SSE client — fetch-based |
| [`@eric8810/catcher-napi-http`](https://www.npmjs.com/package/@eric8810/catcher-napi-http) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-napi-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-http) | ⭐ Rust native HTTP + SSE (typed wrappers) |
| [`@eric8810/catcher-napi-ws`](https://www.npmjs.com/package/@eric8810/catcher-napi-ws) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-napi-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-ws) | ⭐ Rust native WS (typed wrappers) |

### pub.dev

| Package | Version | Description |
|---------|---------|-------------|
| [`catcher_core`](https://pub.dev/packages/catcher_core) | [![pub](https://img.shields.io/pub/v/catcher_core.svg)](https://pub.dev/packages/catcher_core) | Flutter dart:ffi bindings |

### Rust (crates.io)

| Crate | Version | Description |
|-------|---------|-------------|
| [`catcher-core`](https://crates.io/crates/catcher-core) | [![crates.io](https://img.shields.io/crates/v/catcher-core.svg)](https://crates.io/crates/catcher-core) | Shared types & errors |
| [`catcher-http`](https://crates.io/crates/catcher-http) | [![crates.io](https://img.shields.io/crates/v/catcher-http.svg)](https://crates.io/crates/catcher-http) | HTTP — reqwest, retry, CB |
| [`catcher-ws`](https://crates.io/crates/catcher-ws) | [![crates.io](https://img.shields.io/crates/v/catcher-ws.svg)](https://crates.io/crates/catcher-ws) | WS — tokio-tungstenite, codec |
| [`catcher-ffi`](https://crates.io/crates/catcher-ffi) | [![crates.io](https://img.shields.io/crates/v/catcher-ffi.svg)](https://crates.io/crates/catcher-ffi) | cdylib umbrella — 16 C ABI symbols |
| [`catcher-uniffi`](https://crates.io/crates/catcher-uniffi) | [![crates.io](https://img.shields.io/crates/v/catcher-uniffi.svg)](https://crates.io/crates/catcher-uniffi) | UniFFI → Swift + Kotlin |

## Quick Start

### Node.js (native — Rust via napi-rs) ⭐ 推荐

```bash
npm install @eric8810/catcher-napi-http @eric8810/catcher-napi-ws
```

### Node.js (TS — pure TypeScript)

```bash
npm install @eric8810/catcher-http @eric8810/catcher-ws
```

### Browser

```bash
npm install @eric8810/catcher-web
```

### Flutter

```yaml
dependencies:
  catcher_core: ^0.3.3
```

### Usage

#### napi (Rust native) ⭐ 推荐

```typescript
// HTTP — Rust native performance + typed config
import { HttpClient } from '@eric8810/catcher-napi-http'

const client = new HttpClient({
  base_url: 'https://api.example.com',       // camelCase 也可以: baseUrl
  connect_timeout_ms: 10000,
  retry: { max_attempts: 3, backoff: 'Fixed' },
  circuit_breaker: { failure_threshold: 5, reset_timeout_ms: 30000 },
})

const resp = await client.get('/users/1')
console.log(resp.status, resp.body.toString())

// SSE — auto-reconnect
import { SseClient } from '@eric8810/catcher-napi-http'

const sse = new SseClient(
  { url: 'https://stream.example.com/events',
    reconnect: { max_retries: 10, initial_delay_ms: 1000 } },
  (event) => { if (event.type === 'Line') console.log(event.data) },
)

// WebSocket — typed events
import { WsClient } from '@eric8810/catcher-napi-ws'

const ws = new WsClient(
  { urls: ['wss://cn.example.com', 'wss://sg.example.com'],
    reconnect: { initial_delay_ms: 500, max_delay_ms: 30000 } },
  (event) => {
    if (event.type === 'Message')
      console.log(Buffer.from(event.data_base64, 'base64').toString())
  },
)
ws.send('hello')
```

#### TypeScript (pure TS)

```typescript
// HTTP — one line to replace axios.create()
import { createHttpClient } from '@eric8810/catcher-http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  keepAlive: true,
  retry: { attempts: 3 },
  concurrency: 10,
  circuitBreaker: { failureThreshold: 5, resetTimeout: 30_000 },
})

const data = await client.get('/users/1')
const result = await client.post('/messages', { text: 'hello' })

// Per-request overrides
await client.get('/analytics', { retry: false, timeout: 5000 })

// Dynamic interceptors
client.interceptors.request.use(config => {
  config.headers['Authorization'] = `Bearer ${token}`
  return config
})
```

```typescript
// WebSocket — compression + reconnect + multi-endpoint
import { createResilientWS, pack, decodeWSMessage } from '@eric8810/catcher-ws'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],
  perMessageDeflate: true,
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})

ws.send(pack({ event: 'message', data: msg }))
ws.addEventListener('message', e => console.log(decodeWSMessage(e.data)))
```

```typescript
// SSE — AI streaming (OpenAI compatible)
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
  if (payload === '[DONE]') break  // business logic: handle termination yourself
  process.stdout.write(JSON.parse(payload).choices[0]?.delta?.content ?? '')
}
// loop ends = connection closed, no manual cleanup

// SSE — Rust (reqwest + tokio_stream)
// use catcher_http::sse::{SseStream, SseClientConfig};
// let config = SseClientConfig { url: "...".into(), method: SseMethod::POST, ..Default::default() };
// let mut stream = SseStream::connect(config).await?;
// while let Some(line) = stream.next().await {
//     let line = line?;
//     if let Some(payload) = line.strip_prefix("data: ") { println!("{}", payload); }
// }

// SSE — long-lived push with auto-reconnect
import { createSSEClient } from '@eric8810/catcher-http'

const client = createSSEClient({
  url: 'https://api.example.com/events',
  headers: { Authorization: 'Bearer xxx' },
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})
for await (const line of client) {
  if (line.startsWith('data: ')) console.log(line.slice(6))
}
```

```dart
// Flutter — HTTP via Rust FFI
import 'package:catcher_core/catcher_core.dart';

void main() async {
  final client = CatcherHttpClient(HttpClientConfig(
    baseUrl: 'https://httpbin.org',
    retry: RetryConfig(maxAttempts: 3),
  ));

  final resp = await client.get('/get');
  print('Status: ${resp.status}, Body: ${resp.bodyAsString}');

  // POST
  final created = await client.post('/post', body: {'key': 'value'});

  client.dispose();

  // WebSocket via Rust FFI
  final ws = CatcherWsClient(WsClientConfig(
    urls: ['wss://echo.example.com'],
    reconnect: WsReconnectConfig(initialDelayMs: 1000),
  ));

  ws.events.listen((event) {
    if (event is WsMessageEvent) print('Received: ${event.text}');
  });

  ws.sendText('hello');
  await Future.delayed(Duration(seconds: 5));
  ws.dispose();
}
```

## Features

- **Shared HTTP Agent** — TCP keep-alive, DNS caching, TLS session reuse, idle socket eviction
- **Auto-retry** — exponential backoff with jitter, destroys stale keepAlive sockets on retry
- **Circuit Breaker** — trips on consecutive failures, auto-recovers, prevents retry storms
- **Resilient WebSocket** — perMessageDeflate compression, exponential reconnect, multi-endpoint racing
- **Server-Sent Events (SSE)** — raw line stream, auto-reconnect, `Last-Event-ID` resume, `AbortSignal`, cross-platform (Rust + TS + Browser)
- **Binary codec** — msgpack / msgpackr (2-4x faster than JSON, ~47% smaller)
- **Priority queue** — POST before prefetch, concurrency-aware scheduling
- **Dynamic interceptors** — use/eject/clear at runtime, per-request retry/timeout/signal overrides

## Resilience Layers

```
interceptors → retry → circuit breaker → concurrency queue → HTTP engine
```

## Test Results

| Suite | Count | Status |
|-------|-------|--------|
| TS Unit + Integration (http, ws, sse, web) | 323/325 | ✅ |
| TS E2E (scenarios + rust-vs-vanilla) | 38/38 | ✅ |
| Rust Unit — catcher-core | 23/23 | ✅ |
| Rust Unit — catcher-http | 4/4 | ✅ |
| Rust FFI Integration (http + sse + codec) | 17/17 | ✅ |
| Dart Unit Tests | 20/20 | ✅ |
| Dart Integration (real FFI + httpbin.org) | 8/8 | ✅ |

## Documentation

| Directory | Content |
|-----------|---------|
| [`docs/arch-ts/`](./docs/arch-ts/) | TypeScript architecture — overview, module tree, per-module design |
| [`docs/arch-rs/`](./docs/arch-rs/) | Rust architecture — cargo workspace, transport, FFI, resilience |
| [`docs/user-manual/`](./docs/user-manual/) | Platform usage guides — Node.js, Browser, Flutter |
| [`docs/test/`](./docs/test/) | Test architecture — proxy, profiles, scenarios |
| [`docs/research/`](./docs/research/) | Research — API gaps, platform support, strategy |
| [`docs/plan/`](./docs/plan/) | Implementation plan — phased milestones |
| [`docs/issues/`](./docs/issues/) | Bug tracker — code review findings |
| [`docs/showcase.html`](./docs/showcase.html) | Performance showcase page |

## Development

```bash
pnpm install          # install all dependencies
pnpm build            # build all TS packages
pnpm test             # run integration tests (vitest)
pnpm typecheck        # type-check all TS packages
pnpm bench            # run benchmarks

# Rust
cd crates && cargo build
cd crates && cargo test
```

## License

MIT
