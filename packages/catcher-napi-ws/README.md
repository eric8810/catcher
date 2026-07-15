# @eric8810/catcher-napi-ws

[![npm version](https://img.shields.io/npm/v/@eric8810/catcher-napi-ws.svg)](https://www.npmjs.com/package/@eric8810/catcher-napi-ws)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Rust-powered WebSocket client** for Node.js via [napi-rs](https://napi.rs). Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

Wraps `catcher-ws`'s `WsTransport` — yawc + RFC 7692 permessage-deflate + auto-reconnect + heartbeat, compiled to a native addon. Includes typed TypeScript wrappers with auto-generated `.d.ts`.

## ⚠️ Breaking Changes (0.3.0+)

> **Migrate from 0.2.x → 0.3.x** — see [napi API docs](https://github.com/eric8810/catcher/blob/master/docs/user-manual/api/napi.md) for full details.
>
> **v0.3.2**: `wss://` connections now work correctly (TLS via rustls enabled by default).

| Change | Before | After |
|--------|--------|-------|
| Entry point | `client.js` / `client.d.ts` | `dist/client.js` / `dist/client.d.ts` |
| Config format | `JSON.stringify(config)` only | Typed object **or** JSON string |
| Class name | `JsWsClient` | `WsClient` |
| Callback events | Raw JSON strings, need `JSON.parse()` | Typed objects, auto-parsed |
| Message event data | `event.data` (raw) | `event.data_base64` (base64 encoded) |
| Default `handshake_timeout_ms` | `10000` | `15000` |
| Default `per_message_deflate` | `true` | `true` |
| Default `initial_delay_ms` | `1000` | `500` |
| camelCase fields | Not supported | `#[serde(alias)]` — both `snake_case` and `camelCase` accepted |

```diff
- const ws = new WsClient(JSON.stringify(config), (eventJson) => {
-   const event = JSON.parse(eventJson)
-   console.log(event.data)
+ import { WsClient } from '@eric8810/catcher-napi-ws'
+ const ws = new WsClient(config, (event) => {
+   if (event.type === 'Message') {
+     console.log(Buffer.from(event.data_base64, 'base64').toString())
+   }
})
```

## Install

```bash
npm install @eric8810/catcher-napi-ws
```

Pre-built binaries available for Linux (x64/arm64 gnu/musl), macOS (x64/arm64), and Windows (x64/arm64). Installed automatically via platform-specific `optionalDependencies`.

**TLS**: `wss://` connections supported out of the box via rustls (bundled, no system TLS dependency).

## Usage

```typescript
import { WsClient } from '@eric8810/catcher-napi-ws'
import type { WsEvent } from '@eric8810/catcher-napi-ws/types'

// Config as typed object (recommended) or JSON string
const ws = new WsClient(
  {
    urls: ['wss://echo.example.com'],
    reconnect: { initial_delay_ms: 500, max_delay_ms: 30000 },
    heartbeat: { interval_ms: 30000, adaptive: true },
    msgpack: true,
  },
  (event: WsEvent) => {
    switch (event.type) {
      case 'Connected':
        console.log(`Connected to ${event.url} (${event.latency_ms}ms)`)
        break
      case 'Message':
        console.log('Received:', event.is_binary ? '(binary)' : Buffer.from(event.data_base64, 'base64').toString())
        break
      case 'Disconnected':
        console.log(`Disconnected: ${event.code} ${event.reason}`)
        break
      case 'Reconnecting':
        console.log(`Reconnecting attempt ${event.attempt} in ${event.delay_ms}ms`)
        break
      case 'HeartbeatRtt':
        console.log(`RTT: ${event.rtt_ms}ms`)
        break
      case 'Error':
        console.error('Error:', event.message)
        break
    }
  },
)

ws.send(JSON.stringify({ event: 'hello' }))
// later:
ws.close()
```

> **Note**: Do not call `send()` synchronously inside event callbacks — this can deadlock due to napi's single-threaded nature. Use `setImmediate` or `process.nextTick` to defer.

## API

### `new WsClient(config: WsClientConfig | string, onEvent?: (event: WsEvent) => void)`

Create a WebSocket client and connect. Config is a typed object or JSON string. Events are delivered as **parsed objects** (not JSON strings). Supports both `snake_case` and `camelCase` field names.

```typescript
interface WsClientConfig {
  urls: string[]                              // required
  protocols?: string[]
  headers?: Record<string, string>
  per_message_deflate?: boolean               // default: true
  deflate_threshold_bytes?: number            // default: 1024
  handshake_timeout_ms?: number               // default: 15000
  max_payload_bytes?: number                  // default: 67108864 (64MB)
  reconnect?: ReconnectConfig
  heartbeat?: HeartbeatConfig
  race_count?: number                         // default: 1
  dns?: DnsConfig
  proxy?: ProxyConfig
  msgpack?: boolean                           // default: false
}

interface DnsConfig {
  mode?: 'catcher' | 'native'             // default: 'catcher'
  cache_size?: number                         // default: 512
  cache_ttl_secs?: number                     // default: 300
  negative_ttl_secs?: number                  // default: 60
  stale_ttl_secs?: number                     // default: 3600
  stale_on_error?: boolean                    // default: true
  nameservers?: string[]
  host_mapping?: Record<string, string>
}

type ProxyConfig =
  | { mode?: 'manual'; url: string; auth?: ProxyAuth; no_proxy?: string[] }
  | { mode: 'direct' }
  | { mode: 'system'; no_proxy?: string[] }
```

When `msgpack` is enabled, JSON text messages are sent as MessagePack binary frames. Incoming MessagePack binary frames are decoded and delivered as JSON text.

`mode: 'direct'` calls reqwest's `ClientBuilder::no_proxy()` and therefore ignores
explicit, environment, and automatic system proxies. `mode: 'system'` detects fixed
system proxies after `networkChanged()`; PAC/WPAD scripts must be resolved by the host
application and passed as a manual proxy because Catcher does not embed a PAC engine.

### Methods

| Method | Signature |
|--------|-----------|
| `send(data)` | `(data: string) => void` |
| `sendBinary(data)` | `(data: Buffer \| ArrayBuffer \| Uint8Array) => void` |
| `close(code?, reason?)` | `(code?: number, reason?: string) => void` |

### Event Types

Events are delivered as typed objects (auto-parsed from JSON):

| Event | Shape |
|-------|-------|
| Connected | `{ type: 'Connected', url: string, latency_ms: number }` |
| Disconnected | `{ type: 'Disconnected', code: number, reason: string }` |
| Message | `{ type: 'Message', data_base64: string, is_binary: boolean }` |
| Error | `{ type: 'Error', message: string }` |
| Reconnecting | `{ type: 'Reconnecting', attempt: number, delay_ms: number }` |
| HeartbeatRtt | `{ type: 'HeartbeatRtt', rtt_ms: number }` |

> `data_base64` is the base64-encoded payload. Decode with `Buffer.from(event.data_base64, 'base64')`.

### MessagePack helpers

```typescript
import { pack, unpack } from '@eric8810/catcher-napi-ws/codec'

const bytes = pack({ event: 'ping', seq: 42 })
const value = unpack(bytes)
```

## Build from Source

Requires Rust toolchain.

```bash
npm run build       # napi build + tsup compile
npm run build:ts    # tsup only (no Rust rebuild)
```

## License

MIT
