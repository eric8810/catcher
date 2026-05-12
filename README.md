# Catcher 🪤

Resilient network communication toolkit for Node.js / Electron apps.

> "Catcher" — catches network failures before they reach your business logic.

## Packages

```
@catcher/core (zero deps, pure types)
    /            \
   /              \
@catcher/http   @catcher/ws
(axios-based)   (ws + msgpackr)
```

| Package | npm | Description |
|---------|-----|-------------|
| `@catcher/core` | `packages/catcher-core-ts` | Shared type definitions, zero runtime deps |
| `@catcher/http` | `packages/catcher-http-ts` | HTTP client with retry, circuit breaker, priority queue |
| `@catcher/ws` | `packages/catcher-ws-ts` | WebSocket with reconnect, multi-endpoint racing, msgpack codec |
| `@catcher/napi-http` | `packages/catcher-napi-http` | Native HTTP addon via napi-rs (Rust) |
| `@catcher/napi-ws` | `packages/catcher-napi-ws` | Native WebSocket addon via napi-rs (Rust) |

This is a pnpm monorepo. See [pnpm-workspace.yaml](./pnpm-workspace.yaml).

## Features

- **Shared HTTP Agent** — TCP keep-alive, DNS caching (cacheable-lookup), TLS session reuse, idle socket eviction. One agent for all clients.
- **Auto-retry** — exponential backoff with ±25% jitter, configurable per client. On retry, destroys stale keepAlive sockets to force fresh connections.
- **Circuit Breaker** — trips on consecutive failures, auto-recovers after reset timeout. Prevents retry storms.
- **Resilient WebSocket** — perMessageDeflate compression, exponential backoff reconnection, multi-endpoint racing (connect to N regions, use the fastest).
- **Binary codec** — msgpackr (2-4x faster than JSON, ~47% smaller). Auto-detects binary vs text frames. Built into `@catcher/ws`.
- **Priority queue** — lower number = higher priority. POST (1) > PUT/PATCH (2) > GET/DELETE (3). Message sending before prefetch.

## Quick Start

```bash
# REST API
npm install @catcher/http

# IM / real-time communication
npm install @catcher/http @catcher/ws
```

```typescript
// HTTP — one line to replace axios.create()
import { createHttpClient } from '@catcher/http'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  keepAlive: true,                 // connection pooling
  retry: { attempts: 3 },         // auto-retry on failure
  concurrency: 10,                // max parallel requests
  circuitBreaker: {               // trip on 5+ failures
    failureThreshold: 5,
    resetTimeout: 30_000,
  },
})

const data = await client.get('/users/1')
const result = await client.post('/messages', { text: 'hello' })
```

```typescript
// WebSocket — compression + reconnect + multi-endpoint racing
import { createResilientWS } from '@catcher/ws'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],  // multi-region racing
  perMessageDeflate: true,          // ~80% bandwidth reduction
  handshakeTimeout: 10_000,         // fail fast
  reconnect: {
    initialDelay: 1000,
    maxDelay: 30_000,
    maxAttempts: 20,
  },
})

ws.addEventListener('message', (e) => console.log(e.data))
```

```typescript
// Codec — msgpackr binary (faster & smaller than JSON)
import { pack, unpack, decodeWSMessage } from '@catcher/ws'

ws.send(pack({ event: 'message', data: msg }))

ws.addEventListener('message', (e) => {
  const data = decodeWSMessage(e.data)  // auto-detects binary vs JSON
})
```

```typescript
// Standalone exports from @catcher/http
import {
  createHttpClient,
  createRetryWrapper,
  createSharedAgent,
  clearDnsCache,
  createPriorityQueue,
  enqueueWithPriority,
} from '@catcher/http'

// Agent — shared connection pool with DNS caching
const agent = createSharedAgent({ keepAlive: true, dnsCacheTtl: 300 })

// Types from @catcher/core (zero runtime cost)
import type {
  HttpClientConfig,
  ResilientWSOptions,
  SharedAgentOptions,
  RetryOptions,
  PriorityQueueOptions,
} from '@catcher/core'
```

## Resilience Layers (HTTP)

```
axios  →  retry  →  circuit breaker  →  concurrency queue
```

1. **axios** — underlying HTTP engine (pluggable)
2. **retry** (p-retry) — exponential backoff, evicts stale keepAlive sockets before retry
3. **circuit breaker** (cockatiel) — trips on `failureThreshold` consecutive failures, half-opens after `resetTimeout`
4. **concurrency queue** (p-queue) — caps parallel requests, priority-aware scheduling

## Documentation

| Directory | Content |
|-----------|---------|
| [`docs/arch-ts/`](./docs/arch-ts/) | TypeScript package architecture — overview, module tree, per-module design |
| [`docs/arch-rs/`](./docs/arch-rs/) | Rust native addon architecture — cargo workspace, FFI, transport, resilience |
| [`docs/plan/`](./docs/plan/) | Implementation plan — phased milestones (scaffold → types → transport → resilience → FFI) |
| [`docs/research/`](./docs/research/) | Technical research — API gap analysis, WS/TUS split, TS/Dart package split |
| [`docs/issues/`](./docs/issues/) | Bug tracker & fix documentation — keepAlive, retry, circuit breaker edge cases |

## Development

```bash
pnpm install          # install all dependencies
pnpm build            # build all packages
pnpm test             # run all tests
pnpm typecheck        # type-check all packages
pnpm --filter @catcher/http typecheck   # type-check a single package
```

## License

MIT
