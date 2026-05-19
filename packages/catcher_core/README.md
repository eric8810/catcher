# catcher_core

[![pub version](https://img.shields.io/pub/v/catcher_core.svg)](https://pub.dev/packages/catcher_core)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Resilient HTTP/WebSocket client for **Flutter** — powered by a **Rust core** via dart:ffi. Part of the [catcher](https://github.com/eric8810/catcher) toolkit.

## Features

- **HTTP client** — GET / POST / PUT / DELETE / PATCH via Rust reqwest
- **WebSocket client** — auto-reconnect, heartbeat, multi-endpoint racing
- **Retry & circuit breaker** — configurable resilience policies
- **Connection pooling** — keep-alive with configurable idle timeout
- **Binary codec** — msgpack pack/unpack
- **Network quality** — evaluate connection health

## Installation

```yaml
dependencies:
  catcher_core: ^0.3.8
```

> **Note:** This package loads a native Rust library (`libcatcher_ffi.so` / `catcher_ffi.dylib` / `catcher_ffi.dll`). The library must be bundled with your app. See the [Flutter manual build guide](../../docs/user-manual/flutter.md#手动构建-android--ios-native-二进制) for Android/iOS build instructions.

## Quick Start

### HTTP Client

```dart
import 'package:catcher_core/catcher_core.dart';

void main() async {
  final client = CatcherHttpClient(HttpClientConfig(
    baseUrl: 'https://api.example.com',
    connectTimeoutMs: 5000,
    responseTimeoutMs: 30000,
    retry: RetryConfig(maxAttempts: 3, backoff: 'Fixed'),
    pool: PoolConfig(keepAlive: true, maxIdlePerHost: 10),
  ));

  // GET
  final resp = await client.get('/channels');
  print('Status: ${resp.status}, Body: ${resp.bodyAsString}');

  // POST
  final created = await client.post('/messages',
    body: {'text': 'hello'},
    contentType: 'application/json',
  );

  client.dispose();
}
```

### WebSocket Client

```dart
import 'package:catcher_core/catcher_core.dart';

void main() async {
  final ws = CatcherWsClient(WsClientConfig(
    urls: ['wss://echo.example.com'],
    reconnect: WsReconnectConfig(initialDelayMs: 1000, maxDelayMs: 30000),
    heartbeat: WsHeartbeatConfig(intervalMs: 30000, adaptive: true),
  ));

  // Listen to events
  ws.events.listen((event) {
    if (event is WsConnectedEvent) {
      print('Connected to ${event.url} (${event.latencyMs}ms)');
    } else if (event is WsMessageEvent) {
      print('Received: ${event.text}');
    } else if (event is WsDisconnectedEvent) {
      print('Disconnected: ${event.code} ${event.reason}');
    } else if (event is WsReconnectingEvent) {
      print('Reconnecting attempt ${event.attempt}');
    } else if (event is WsHeartbeatRttEvent) {
      print('Heartbeat RTT: ${event.rttMs}ms');
    } else if (event is WsErrorEvent) {
      print('Error: ${event.message}');
    }
  });

  ws.sendText('hello');
  await Future.delayed(Duration(seconds: 5));
  ws.dispose();
}
```

### Binary Codec

```dart
import 'package:catcher_core/catcher_core.dart';

final packed = pack({'event': 'ping', 'seq': 42}); // Uint8List (msgpack)
final unpacked = unpack(packed);                   // Map<String, dynamic>
```

## API Reference

### HTTP

| Type | Description |
|------|-------------|
| `CatcherHttpClient` | HTTP client wrapper (get, post, put, delete, patch) |
| `HttpClientConfig` | Base URL, timeouts, pool, retry, circuit breaker |
| `HttpResponse` | status, headers, body bytes, elapsedMs, bodyAsString |
| `RetryConfig` | maxAttempts, backoff, jitter |
| `CircuitBreakerConfig` | failureThreshold, resetTimeoutMs |
| `PoolConfig` | keepAlive, maxIdlePerHost, idleTimeoutSecs |
| `CatcherHttpError` | Exception from Rust HTTP client |

### WebSocket

| Type | Description |
|------|-------------|
| `CatcherWsClient` | WebSocket client with event stream |
| `WsClientConfig` | URLs, reconnect, heartbeat, compression |
| `WsReconnectConfig` | Reconnect timing (initial, max delay, backoff) |
| `WsHeartbeatConfig` | Heartbeat interval, adaptive, pong timeout |
| `WsEvent` | Base event class |
| `WsConnectedEvent` | url, latencyMs |
| `WsDisconnectedEvent` | code, reason |
| `WsReconnectingEvent` | attempt, delayMs |
| `WsMessageEvent` | data (bytes), isBinary, text getter |
| `WsErrorEvent` | message |
| `WsHeartbeatRttEvent` | rttMs |
| `CatcherWsError` | Exception from Rust WS client |

## License

MIT
