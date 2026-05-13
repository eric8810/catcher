# catcher_core

Resilient HTTP/WebSocket client for Dart/Flutter, powered by a Rust core via FFI.

## Features

- HTTP client with retries, timeouts, and connection pooling
- WebSocket client with auto-reconnection and compression
- Priority request queue with concurrency control
- Native performance via Rust FFI

## Quick Start

```dart
import 'package:catcher_core/catcher_core.dart';

final client = CatcherHttpClient();
final resp = await client.get('https://httpbin.org/get');
print(resp.body);
await client.close();
```

## Installation

```yaml
dependencies:
  catcher_core: ^0.1.0
```

## License

MIT
