# Catcher 🪤

Resilient network communication toolkit for Electron / Node.js apps.

> "Catcher" — catches network failures before they reach your business logic.

## Features

- **Shared HTTP Agent** — keep-alive, DNS caching, TLS session sharing. One agent for all clients.
- **Auto-retry** — exponential backoff with jitter, configurable per client.
- **Circuit Breaker** — trip on failure threshold, auto-recover after reset timeout.
- **Resilient WebSocket** — perMessageDeflate compression, exponential backoff reconnection, multi-endpoint racing.
- **Binary codec** — msgpackr (2-4x faster than JSON, 47% smaller). Drop-in replacement.
- **Priority queue** — message sending before avatar loading.

## Quick Start

```bash
npm install catcher
```

```typescript
// HTTP — one line to replace axios.create()
import { createHttpClient } from 'catcher'

const client = createHttpClient({
  baseURL: 'https://api.example.com',
  keepAlive: true,          // connection pooling
  retry: { attempts: 3 },   // auto-retry on failure
  concurrency: 10,          // max parallel requests
})

const data = await client.get('/users/1')
const result = await client.post('/messages', { text: 'hello' })
```

```typescript
// WebSocket — one line for compression + reconnect + multi-endpoint
import { createResilientWS } from 'catcher'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],  // multi-region racing
  perMessageDeflate: true,   // 80% bandwidth reduction
  handshakeTimeout: 10_000,  // fail fast
})

ws.addEventListener('message', (e) => console.log(e.data))
```

```typescript
// Codec — msgpackr binary (faster & smaller than JSON)
import { pack, unpack } from 'catcher'

ws.send(pack({ event: 'message', data: msg }))
const data = unpack(buffer)
```

## Modules

| Module | Import | Key export |
|--------|--------|-----------|
| Agent | `catcher/agent` | `createSharedAgent()` |
| HTTP | `catcher/http` | `createHttpClient()` |
| WebSocket | `catcher/ws` | `createResilientWS()` |
| Codec | `catcher/codec` | `pack()`, `unpack()` |
| Queue | `catcher/queue` | `createPriorityQueue()` |

## Documentation

- [通信传输层分析](docs/weak-network-communication-analysis.md)
- [开源框架选型评估](docs/open-source-communication-frameworks.md)
- [优化前后数字对比](docs/simulation-before-after.md)
- [真实移动场景可用性](docs/real-world-scenarios.md)
- [CPU 与电池消耗分析](docs/cpu-battery-impact.md)
- [network-kit 设计文档](docs/network-kit-design.md)

## License

MIT
