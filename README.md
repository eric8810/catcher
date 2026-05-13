# Catcher 🪤

Resilient network communication toolkit — Rust core, TypeScript wrappers, cross-platform.

> "Catcher" — catches network failures before they reach your business logic.

## Platform Coverage

| Platform | Package | Status |
|----------|---------|--------|
| **Node.js (native)** | `@eric8810/napi-http` / `@eric8810/napi-ws` | ✅ |
| **Node.js (TS)** | `@eric8810/http` / `@eric8810/ws` | ✅ |
| **Electron** | same as Node.js | ✅ |
| **Web** | `@eric8810/web` | ✅ |
| **Rust** | `catcher-http` / `catcher-ws` / `catcher-core` | ✅ |
| **Flutter** | `catcher_core` (dart:ffi) | ⚠️ WIP (HTTP client wired, WS client skeleton) |
| **Android + iOS** | `catcher-uniffi` (UniFFI) | ⚠️ WIP (proc-macro mode, needs mobile build verification) |

## Packages

```
catcher-core (Rust)              @eric8810/core (TS)
     │                                │
 ┌───┴───┐                        ┌───┴───┐
 ▼       ▼                        ▼       ▼
catcher  catcher             @catcher  @catcher  @catcher
-http    -ws                 /http     /ws       /web
 │  │     │  │               (axios)   (ws)     (fetch)
 │  │     │  │
 │  └──napi-rs──┐   ┌──napi-rs──┘
 │              ▼   ▼
 │        @eric8810/napi-http
 │        @eric8810/napi-ws
 │
 ├── UniFFI → Swift + Kotlin
 └── C ABI  → dart:ffi (Flutter)
```

| Package | Path | Description |
|---------|------|-------------|
| `@eric8810/core` | `packages/catcher-core-ts` | Shared TS type definitions |
| `@eric8810/http` | `packages/catcher-http-ts` | HTTP client — retry, CB, queue, interceptors |
| `@eric8810/ws` | `packages/catcher-ws-ts` | WebSocket — reconnect, multi-endpoint, codec |
| `@eric8810/web` | `packages/catcher-web` | Browser HTTP client — fetch-based |
| `@eric8810/napi-http` | `packages/catcher-napi-http` | Rust native via napi-rs |
| `@eric8810/napi-ws` | `packages/catcher-napi-ws` | Rust native via napi-rs |
| `catcher-core` | `packages/catcher-core` | Rust shared types & errors |
| `catcher-http` | `packages/catcher-http` | Rust HTTP — reqwest, retry, CB |
| `catcher-ws` | `packages/catcher-ws` | Rust WS — tokio-tungstenite, codec |
| `catcher-uniffi` | `packages/catcher-uniffi` | UniFFI → Swift + Kotlin |
| `catcher_core` | `packages/catcher_core` | Flutter dart:ffi bindings |

> pnpm monorepo + Cargo workspace. See `pnpm-workspace.yaml` and `packages/Cargo.toml`.

## Features

- **Shared HTTP Agent** — TCP keep-alive, DNS caching, TLS session reuse, idle socket eviction
- **Auto-retry** — exponential backoff with jitter, destroys stale keepAlive sockets on retry
- **Circuit Breaker** — trips on consecutive failures, auto-recovers, prevents retry storms
- **Resilient WebSocket** — perMessageDeflate compression, exponential reconnect, multi-endpoint racing
- **Binary codec** — msgpack / msgpackr (2-4x faster than JSON, ~47% smaller)
- **Priority queue** — POST before prefetch, concurrency-aware scheduling
- **Dynamic interceptors** — use/eject/clear at runtime, per-request retry/timeout/signal overrides

## Quick Start

```bash
# Node.js (native — Rust via napi-rs)
npm install @eric8810/napi-http @eric8810/napi-ws

# Node.js (TS — more API features)
npm install @eric8810/http @eric8810/ws

# Browser
npm install @eric8810/web
```

```typescript
// HTTP — one line to replace axios.create()
import { createHttpClient } from '@eric8810/http'

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
import { createResilientWS, pack, decodeWSMessage } from '@eric8810/ws'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],
  perMessageDeflate: true,
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})

ws.send(pack({ event: 'message', data: msg }))
ws.addEventListener('message', e => console.log(decodeWSMessage(e.data)))
```

## Resilience Layers

```
interceptors → retry → circuit breaker → concurrency queue → HTTP engine
```

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
pnpm test             # run integration tests (vitest)
pnpm typecheck        # type-check all TS packages
pnpm bench             # run benchmarks

# Rust
cd packages && cargo build
```

## License

MIT
