# @eric8810/catcher-napi-ws

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-napi-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-ws)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Rust-powered WebSocket client** for Node.js via [napi-rs](https://napi.rs). Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

Wraps `catcher-ws`'s `WsTransport` — tokio-tungstenite + auto-reconnect + heartbeat, compiled to a native addon.

## Install

```bash
npm install @eric8810/catcher-napi-ws
```

Pre-built binaries available for Linux (x64/arm64), macOS (x64/arm64), and Windows (x64).

## Usage

```javascript
const { JsWsClient } = require('@eric8810/catcher-napi-ws')

const ws = new JsWsClient(
  JSON.stringify({
    urls: ['wss://echo.example.com'],
    reconnect: { initial_delay_ms: 500, max_delay_ms: 30000 },
    heartbeat: { interval_ms: 30000, adaptive: true },
  }),
  (eventJson) => {
    const event = JSON.parse(eventJson)
    switch (event.type) {
      case 'Connected':
        console.log(`Connected to ${event.url} (${event.latency_ms}ms)`)
        break
      case 'Message':
        console.log('Received:', event.data)
        break
      case 'Disconnected':
        console.log(`Disconnected: ${event.code} ${event.reason}`)
        break
      case 'Error':
        console.error('WS Error:', event.message)
        break
    }
  }
)

ws.send('hello')
// ...
ws.close()
```

## API

### `new JsWsClient(configJson: string, onEvent?: (eventJson: string) => void)`

Create a WebSocket client and connect. Events are delivered as JSON strings to the callback.

#### Config JSON

```typescript
interface WsClientConfig {
  urls: string[]
  reconnect?: { initial_delay_ms?: number; max_delay_ms?: number; backoff_multiplier?: number; max_attempts?: number }
  heartbeat?: { interval_ms?: number; adaptive?: boolean; pong_timeout_ms?: number }
  per_message_deflate?: boolean
}
```

### Methods

| Method | Signature |
|--------|-----------|
| `send(data)` | `(data: string) => void` |
| `close()` | `() => void` |

### Event Types (JSON)

| Event | Shape |
|-------|-------|
| Connected | `{ "type": "Connected", "url": "...", "latency_ms": 5 }` |
| Disconnected | `{ "type": "Disconnected", "code": 1000, "reason": "..." }` |
| Message | `{ "type": "Message", "data": "...", "is_binary": false }` |
| Error | `{ "type": "Error", "message": "..." }` |

## Build from Source

Requires Rust toolchain.

```bash
npm run build
```

## License

MIT
