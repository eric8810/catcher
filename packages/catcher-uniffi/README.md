# catcher-uniffi

[![crates.io](https://img.shields.io/crates/v/catcher-uniffi.svg)](https://crates.io/crates/catcher-uniffi)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

UniFFI bindings for the [catcher](https://github.com/eric8810/catcher) toolkit — auto-generates **Swift** (iOS) and **Kotlin** (Android) bindings from the Rust API.

Uses UniFFI 0.28 proc-macro mode (no UDL file needed).

> **⚠️ Breaking Change (0.3.0)**: JSON config now accepts `camelCase` field names via `#[serde(alias)]`. `BackoffKind` default changed to `Fixed`. WS config field renames: `deflate_threshold` → `deflate_threshold_bytes`, `max_message_size` → `max_payload_bytes`, `ping_timeout_ms` → `pong_timeout_ms`.

## Exposed API

### `HttpClient`

| Method | Description |
|--------|-------------|
| `new(config_json)` | Create from JSON config string |
| `get(url)` | GET request |
| `post(url, body, content_type?)` | POST request |
| `put(url, body, content_type?)` | PUT request |
| `delete(url)` | DELETE request |
| `patch(url, body, content_type?)` | PATCH request |

Returns `HttpResponseDto` with `status: UInt16`, `body: [UInt8]`, `elapsed_ms: UInt64`.

### `WsClient`

| Method | Description |
|--------|-------------|
| `new(config_json, observer)` | Connect and register event observer |
| `send_text(text)` | Send a text message |
| `send_binary(data)` | Send a binary message |
| `close(code, reason)` | Close the connection |

### `WsEventDto` (enum)

| Variant | Fields |
|---------|--------|
| `Connected` | `url: String`, `latency_ms: UInt64` |
| `Disconnected` | `code: UInt16`, `reason: String` |
| `Reconnecting` | `attempt: UInt32`, `delay_ms: UInt64` |
| `Message` | `data: [UInt8]`, `is_binary: Boolean` |
| `Error` | `message: String` |
| `HeartbeatRtt` | `rtt_ms: UInt64` |

### `WsEventObserver` (callback interface)

Implement in Swift/Kotlin to receive WebSocket events:

```swift
class MyObserver: WsEventObserver {
    func onEvent(event: WsEventDto) {
        switch event {
        case .Message(let data, let isBinary):
            print("Received \(data.count) bytes")
        case .Connected(let url, let latencyMs):
            print("Connected to \(url) in \(latencyMs)ms")
        default: break
        }
    }
}
```

## Build & Generate

```bash
# Build the shared library
cargo build --release

# Generate Swift bindings
uniffi-bindgen generate --library ../target/release/libcatcher_uniffi.so --language swift --out-dir generated/swift

# Generate Kotlin bindings
uniffi-bindgen generate --library ../target/release/libcatcher_uniffi.so --language kotlin --out-dir generated/kotlin
```

## Config JSON Schema

### HTTP

```json
{
  "base_url": "https://api.example.com",
  "connect_timeout_ms": 10000,
  "response_timeout_ms": 30000,
  "pool": { "keep_alive": true, "max_idle_per_host": 10 },
  "retry": { "max_attempts": 3, "backoff": "Fixed" },
  "circuit_breaker": { "failure_threshold": 5, "reset_timeout_ms": 30000 }
}
```

### WebSocket

```json
{
  "urls": ["wss://echo.example.com"],
  "reconnect": { "initial_delay_ms": 500, "max_delay_ms": 30000 },
  "heartbeat": { "interval_ms": 30000, "adaptive": true },
  "per_message_deflate": true
}
```

## License

MIT
