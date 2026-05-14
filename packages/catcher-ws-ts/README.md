# @eric8810/catcher-ws

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-ws)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient WebSocket client for **Node.js** — auto-reconnect, multi-endpoint racing, msgpack codec. Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

Built on the **ws** library (optional peer dep) with reconnect + endpoint racing + binary codec.

## Install

```bash
npm install @eric8810/catcher-ws
# ws and msgpackr are optional peer dependencies
npm install ws msgpackr
```

## Quick Start

```typescript
import { createResilientWS, pack, decodeWSMessage } from '@eric8810/catcher-ws'

const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com'],
  perMessageDeflate: true,
  reconnect: { initialDelay: 1000, maxDelay: 30_000 },
})

// Send msgpack-encoded message (2-4x faster than JSON, ~47% smaller)
ws.send(pack({ event: 'message', data: 'hello' }))

// Receive and decode
ws.addEventListener('message', (e) => {
  const decoded = decodeWSMessage(e.data)
  console.log(decoded)
})
```

## API

### `createResilientWS(options)`

Creates a WebSocket client with auto-reconnect and multi-endpoint support.

```typescript
interface ResilientWSOptions {
  url: string | string[]
  perMessageDeflate?: boolean
  reconnect?: {
    initialDelay?: number
    maxDelay?: number
    backoffMultiplier?: number
    maxAttempts?: number
  }
}
```

Returns a standard `WebSocket`-compatible object with `send()`, `close()`, and `addEventListener()`.

### Codec Functions

| Function | Description |
|----------|-------------|
| `pack(data)` | Encode JSON-compatible value to msgpack `Buffer` |
| `unpack(buffer)` | Decode msgpack `Buffer` to JSON-compatible value |
| `isBinary(data)` | Check if WebSocket message data is binary |
| `decodeWSMessage(data)` | Auto-detect and decode WS message (text or binary) |

### Utility Functions

| Function | Description |
|----------|-------------|
| `createReconnectStrategy(opts)` | Create a standalone reconnect strategy |
| `raceEndpoints(urls, options)` | Race multiple WebSocket endpoints |

## Multi-endpoint Racing

```typescript
const ws = createResilientWS({
  url: ['wss://cn.example.com', 'wss://sg.example.com', 'wss://us.example.com'],
  reconnect: { initialDelay: 500, maxDelay: 30_000 },
})

// Connects to all endpoints simultaneously, keeps the fastest
```

## License

MIT
