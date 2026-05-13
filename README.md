# Catcher 🪤

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-http)
[![pub version](https://img.shields.io/pub/v/catcher_core.svg)](https://pub.dev/packages/catcher_core)
[![crates.io](https://img.shields.io/crates/v/catcher-http.svg)](https://crates.io/crates/catcher-http)
[![CI](https://github.com/eric8810/catcher/actions/workflows/ci.yml/badge.svg)](https://github.com/eric8810/catcher/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient network communication toolkit — Rust core, TypeScript wrappers, Flutter bindings, cross-platform.

> "Catcher" — catches network failures before they reach your business logic.

## Platform Coverage

| Platform | Package | Status |
|----------|---------|--------|
| **Node.js (native)** | `@eric8810/catcher-napi-http` / `@eric8810/catcher-napi-ws` | ✅ Published |
| **Node.js (TS)** | `@eric8810/catcher-http` / `@eric8810/catcher-ws` | ✅ Published |
| **Electron** | same as Node.js | ✅ |
| **Web** | `@eric8810/catcher-web` | ✅ Published |
| **Rust** | `catcher-http` / `catcher-ws` / `catcher-core` | ✅ Published |
| **Flutter** | `catcher_core` (dart:ffi) | ✅ Published |
| **Android + iOS** | `catcher-uniffi` (UniFFI) | ⚠️ WIP |

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
 │  └──napi-rs──┐   ┌──napi-rs──┘
 │              ▼   ▼
 │        @eric8810/catcher-napi-http
 │        @eric8810/catcher-napi-ws
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
```

## Packages

### npm (@eric8810 scope)

| Package | Version | Description |
|---------|---------|-------------|
| [`@eric8810/catcher-core`](https://www.npmjs.com/package/@eric8810/catcher-core) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-core.svg)](https://www.npmjs.com/package/@eric8810/catcher-core) | Shared TS type definitions |
| [`@eric8810/catcher-http`](https://www.npmjs.com/package/@eric8810/catcher-http) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-http) | HTTP client — retry, CB, queue, interceptors |
| [`@eric8810/catcher-ws`](https://www.npmjs.com/package/@eric8810/catcher-ws) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-ws) | WebSocket — reconnect, multi-endpoint, codec |
| [`@eric8810/catcher-web`](https://www.npmjs.com/package/@eric8810/catcher-web) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-web.svg)](https://www.npmjs.com/package/@eric8810/catcher-web) | Browser HTTP client — fetch-based |
| [`@eric8810/catcher-napi-http`](https://www.npmjs.com/package/@eric8810/catcher-napi-http) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-napi-http.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-http) | Rust native via napi-rs |
| [`@eric8810/catcher-napi-ws`](https://www.npmjs.com/package/@eric8810/catcher-napi-ws) | [![npm](https://img.shields.io/npm/v/@eric8810/catcher-napi-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-ws) | Rust native via napi-rs |

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
| `catcher-uniffi` | — | UniFFI → Swift + Kotlin (WIP) |

## Quick Start

### Node.js (native — Rust via napi-rs)

```bash
npm install @eric8810/catcher-napi-http @eric8810/catcher-napi-ws
```

### Node.js (TS — full API)

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
  catcher_core: ^0.1.0
```

### Usage

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

```dart
// Flutter — HTTP via Rust FFI
import 'package:catcher_core/catcher_core.dart';

final client = CatcherHttpClient();
final resp = await client.get('https://httpbin.org/get');
print(resp.body);
await client.close();
```

## Features

- **Shared HTTP Agent** — TCP keep-alive, DNS caching, TLS session reuse, idle socket eviction
- **Auto-retry** — exponential backoff with jitter, destroys stale keepAlive sockets on retry
- **Circuit Breaker** — trips on consecutive failures, auto-recovers, prevents retry storms
- **Resilient WebSocket** — perMessageDeflate compression, exponential reconnect, multi-endpoint racing
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
| TS E2E (scenarios + rust-vs-vanilla) | 38/38 | ✅ |
| TS Integration (http + ws + chaos) | 12/12 | ✅ |
| Dart Unit Tests | 20/20 | ✅ |
| Dart Integration (real FFI + httpbin.org) | 8/8 | ✅ |
| Rust catcher-ffi FFI tests | 8/8 | ✅ |

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
