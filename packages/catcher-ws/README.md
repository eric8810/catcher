# catcher-ws

[![crates.io](https://img.shields.io/crates/v/catcher-ws.svg)](https://crates.io/crates/catcher-ws)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient WebSocket client for the [catcher](https://github.com/eric8810/catcher) toolkit — built on **tokio-tungstenite** with automatic reconnection, heartbeat, and multi-endpoint racing.

## Features

- **Auto-reconnect** — exponential backoff with jitter
- **Adaptive heartbeat** — configurable ping/pong with RTT tracking
- **Multi-endpoint racing** — connect to the fastest of N servers
- **Per-message deflate** — compression support
- **Msgpack codec** — built-in `pack()` / `unpack()` for binary serialization
- **FFI C ABI** — exported symbols for cross-language bindings

## Usage

```toml
[dependencies]
catcher-ws = "0.2"
```

### Basic WebSocket connection

```rust
use catcher_ws::{WsTransport, WsHandle, WsEvent};
use catcher_ws::types::ws::WsClientConfig;

let config = WsClientConfig {
    urls: vec!["wss://echo.example.com".into()],
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
    race_count: 2,  // race first 2 endpoints, use fastest
    ..Default::default()
};
```

## Re-exports

| Type | Description |
|------|-------------|
| `WsTransport`, `WsHandle` | Async WebSocket client & handle |
| `WsEvent`, `WsState` | Event types |
| `WsClientConfig`, `ReconnectConfig`, `HeartbeatConfig` | Configuration |
| `EndpointRacer` | Multi-endpoint racing |
| `ReconnectManager`, `HeartbeatManager` | Internal managers |
| `pack`, `unpack`, `unpack_value` | Msgpack codec |

## License

MIT
