# catcher-ws

[![crates.io](https://img.shields.io/crates/v/catcher-ws.svg)](https://crates.io/crates/catcher-ws)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient WebSocket client for the [catcher](https://github.com/eric8810/catcher) toolkit — built on **tokio-tungstenite** with automatic reconnection, heartbeat, and multi-endpoint racing.

> **⚠️ Breaking Change (0.3.0)**:
> - `WsClientConfig` field renames: `deflate_threshold` → `deflate_threshold_bytes`, `max_message_size` → `max_payload_bytes`
> - `HeartbeatConfig`: `ping_timeout_ms` → `pong_timeout_ms`, added `max_missed_pongs` field
> - `per_message_deflate` default changed from `true` to `false`
> - `handshake_timeout_ms` default changed from `10000` to `15000`
> - `initial_delay_ms` default changed from `1000` to `500`
> - All config structs accept `camelCase` via `#[serde(alias)]`

## Features

- **Auto-reconnect** — exponential backoff with jitter
- **Adaptive heartbeat** — configurable ping/pong with RTT tracking
- **Multi-endpoint racing** — connect to the fastest of N servers
- **DNS cache config** — shared `DnsConfig` with cache TTL, stale fallback, nameservers, host mapping
- **Per-message deflate** — compression support
- **Msgpack codec** — built-in `pack()` / `unpack()` for binary serialization
- **FFI C ABI** — exported symbols for cross-language bindings

## Usage

```toml
[dependencies]
catcher-ws = "0.3.10"
```

### Basic WebSocket connection

```rust
use catcher_ws::{DnsConfig, HeartbeatConfig, ReconnectConfig, WsEvent, WsTransport};
use catcher_ws::types::ws::WsClientConfig;

let config = WsClientConfig {
    urls: vec!["wss://echo.example.com".into()],
    dns: Some(DnsConfig {
        cache_ttl_secs: 300,
        stale_on_error: true,
        ..Default::default()
    }),
    reconnect: Some(ReconnectConfig {
        initial_delay_ms: 500,
        max_delay_ms: 30_000,
        backoff_multiplier: 2.0,
        max_attempts: 20,
    }),
    heartbeat: Some(HeartbeatConfig {
        interval_ms: 30_000,
        adaptive: true,
        pong_timeout_ms: 10_000,
        max_missed_pongs: 3,
    }),
    ..Default::default()
};

let (handle, mut rx) = WsTransport::connect("wss://echo.example.com", &config).await?;

// Send
handle.send_text("hello")?;

// Receive events
while let Some(event) = rx.recv().await {
    match event {
        WsEvent::Connected { url, latency_ms } => println!("Connected to {} ({}ms)", url, latency_ms),
        WsEvent::Message { data, is_binary } => println!("Received: {} bytes", data.len()),
        WsEvent::Disconnected { code, reason } => println!("Disconnected: {} {}", code, reason),
        _ => {}
    }
}
```

### Binary codec (msgpack)

```rust
use catcher_ws::codec::{pack, unpack};
use serde_json::json;

let value = json!({"event": "ping", "seq": 42});
let packed: Vec<u8> = pack(&value)?;  // msgpack binary
let unpacked = unpack(&packed)?;      // back to serde_json::Value
```

### Multi-endpoint racing

```rust
let config = WsClientConfig {
    urls: vec![
        "wss://cn.example.com".into(),
        "wss://sg.example.com".into(),
        "wss://us.example.com".into(),
    ],
    race_count: 2,  // race first 2 endpoints, use fastest (default: 1)
    ..Default::default()
};
```

## Re-exports

| Type | Description |
|------|-------------|
| `WsTransport`, `WsHandle` | Async WebSocket client & handle |
| `WsEvent`, `WsState` | Event types |
| `WsClientConfig`, `DnsConfig`, `ReconnectConfig`, `HeartbeatConfig` | Configuration |
| `EndpointRacer` | Multi-endpoint racing |
| `ReconnectManager`, `HeartbeatManager` | Internal managers |
| `pack`, `unpack`, `unpack_value` | Msgpack codec |

## License

MIT
