# catcher_core API Reference

> Flutter dart:ffi 绑定 — CatcherHttpClient, CatcherWsClient, codec, quality

```yaml
# pubspec.yaml
dependencies:
  catcher_core: ^0.2.2
```

---

## 导入

```dart
import 'package:catcher_core/catcher_core.dart';
```

---

## CatcherHttpClient

```dart
class CatcherHttpClient {
  CatcherHttpClient(HttpClientConfig config);
  Future<HttpResponse> get(String path, {Map<String, String>? queryParams, CancelToken? cancelToken});
  Future<HttpResponse> post(String path, {dynamic body, String? contentType, CancelToken? cancelToken});
  Future<HttpResponse> put(String path, {dynamic body, String? contentType, CancelToken? cancelToken});
  Future<HttpResponse> delete(String path, {CancelToken? cancelToken});
  Future<HttpResponse> patch(String path, {dynamic body, String? contentType, CancelToken? cancelToken});
  CircuitBreakerState get circuitBreakerState;
  int get queueDepth;
  void dispose();
}
```

### HttpClientConfig

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `baseUrl` | `String` | **必填** | 基础 URL |
| `connectTimeoutMs` | `int` | `10000` | 连接超时（ms） |
| `responseTimeoutMs` | `int` | `30000` | 响应超时（ms） |
| `pool` | `PoolConfig` | 默认配置 | 连接池配置 |
| `dns` | `DnsConfig?` | — | DNS 缓存、旧缓存兜底、自定义解析；显式 `mode: 'native'` 时使用原生解析 |
| `retry` | `RetryConfig?` | — | 重试配置 |
| `circuitBreaker` | `CircuitBreakerConfig?` | — | 熔断器配置 |
| `msgpack` | `bool` | `false` | 启用 HTTP body 自动 JSON ↔ msgpack |

### DnsConfig

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `mode` | `String` | `'catcher'` | `'native'` 使用原生解析 |
| `cacheSize` | `int` | `512` | DNS 缓存条目上限 |
| `cacheTtlSecs` | `int` | `300` | 正常缓存有效时间（秒） |
| `negativeTtlSecs` | `int` | `60` | 失败结果缓存时间（秒） |
| `staleTtlSecs` | `int` | `3600` | 旧缓存可兜底的时间（秒） |
| `staleOnError` | `bool` | `true` | DNS 失败时是否使用旧缓存 |
| `nameservers` | `List<String>` | `[]` | 自定义 DNS 服务器 |
| `hostMapping` | `Map<String, String>` | `{}` | 域名到 IP 的固定映射 |

### RetryConfig

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `maxAttempts` | `int` | `3` | 最多尝试次数 |
| `backoff` | `String` | `'fixed'` | `'fixed'` 或 `'exponential'` |

### CircuitBreakerConfig

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `failureThreshold` | `int` | `5` | 连续失败 N 次后 OPEN |
| `resetTimeoutMs` | `int` | `30000` | OPEN → HALF_OPEN 等待（ms） |

### HttpResponse

```dart
class HttpResponse {
  int status;
  Map<String, String> headers;
  Uint8List body;
  String? bodyAsString;
  int elapsedMs;
}
```

### CancelToken

```dart
class CancelToken {
  void cancel();
  bool get isCancelled;
}
```

### 示例

```dart
final client = CatcherHttpClient(HttpClientConfig(
  baseUrl: 'https://api.example.com',
  connectTimeoutMs: 5000,
  responseTimeoutMs: 30000,
  retry: RetryConfig(maxAttempts: 3, backoff: 'fixed'),
  circuitBreaker: CircuitBreakerConfig(
    failureThreshold: 5,
    resetTimeoutMs: 30000,
  ),
));

// GET
final resp = await client.get('/users/1');
print('Status: ${resp.status}, Body: ${resp.bodyAsString}');

// POST
await client.post('/messages', body: {'text': 'hello'});

// 带查询参数
await client.get('/search', queryParams: {'q': 'test', 'page': '1'});

// 取消
final token = CancelToken();
Future.delayed(Duration(seconds: 5), () => token.cancel());
await client.get('/slow', cancelToken: token);

// 释放
client.dispose();
```

---

## CatcherWsClient

```dart
class CatcherWsClient {
  CatcherWsClient(WsClientConfig config);
  Stream<WsEvent> get events;
  void sendText(String data);
  void sendBinary(Uint8List data);
  void close([int code = 1000, String reason = 'normal']);
  void dispose();
}
```

### WsClientConfig

| 参数 | 类型 | 默认 | 说明 |
|------|------|------|------|
| `urls` | `List<String>` | **必填** | WebSocket URL(s) |
| `perMessageDeflate` | `bool` | `true` | 标准 RFC 7692 permessage-deflate，和 Node.js `ws` 路径对齐 |
| `applicationCompression` | `WsApplicationCompressionConfig?` | — | 应用层 gzip/zstd fallback；`perMessageDeflate` 开启时不会叠加使用 |
| `applicationCompression.enabled` | `bool` | `true` | 是否启用应用层压缩 |
| `applicationCompression.algorithm` | `WsApplicationCompressionAlgorithm` | `gzip` | `gzip` 或 `zstd` |
| `applicationCompression.thresholdBytes` | `int` | `1024` | 大于等于此大小才压缩 |
| `reconnect` | `ReconnectConfig?` | — | 重连 |
| `reconnect.initialDelayMs` | `int` | `1000` | 初始延迟（ms） |
| `reconnect.maxDelayMs` | `int` | `30000` | 最大延迟（ms） |
| `reconnect.maxAttempts` | `int` | `20` | 最多重连次数 |
| `heartbeat` | `HeartbeatConfig?` | — | 心跳 |
| `heartbeat.intervalMs` | `int` | `30000` | 心跳间隔（ms） |
| `heartbeat.adaptive` | `bool` | `true` | 自适应间隔 |
| `dns` | `DnsConfig?` | — | DNS 缓存、旧缓存兜底、自定义解析；显式 `mode: 'native'` 时使用原生解析 |
| `msgpack` | `bool` | `false` | 启用 WS 文本消息 JSON ↔ msgpack |

### WsEvent 类型

```dart
class WsConnectedEvent { String url; int latencyMs; }
class WsMessageEvent { String? text; Uint8List? binary; bool isBinary; }
class WsDisconnectedEvent { int code; String reason; }
class WsReconnectingEvent { int attempt; int delayMs; }
class WsHeartbeatRttEvent { int rttMs; }
class WsErrorEvent { String message; }
```

### 示例

```dart
final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://cn.example.com', 'wss://sg.example.com'],
  reconnect: ReconnectConfig(
    initialDelayMs: 1000,
    maxDelayMs: 30000,
    maxAttempts: 20,
  ),
  heartbeat: HeartbeatConfig(intervalMs: 30000, adaptive: true),
));

ws.events.listen((event) {
  if (event is WsConnectedEvent) {
    print('Connected to ${event.url} (${event.latencyMs}ms)');
  } else if (event is WsMessageEvent) {
    print('Received: ${event.text}');
  } else if (event is WsDisconnectedEvent) {
    print('Disconnected: ${event.code} ${event.reason}');
  } else if (event is WsReconnectingEvent) {
    print('Reconnecting attempt ${event.attempt} in ${event.delayMs}ms');
  } else if (event is WsHeartbeatRttEvent) {
    print('Heartbeat RTT: ${event.rttMs}ms');
  } else if (event is WsErrorEvent) {
    print('Error: ${event.message}');
  }
});

ws.sendText('hello');
ws.sendBinary(Uint8List.fromList([1, 2, 3]));
ws.close();
ws.dispose();
```

---

## 编解码

```dart
// pack — Dart value → msgpack binary (Uint8List)
Uint8List pack(dynamic value);

// unpack — msgpack binary → Dart value
dynamic unpack(Uint8List data);
```

```dart
final packed = pack({'event': 'message', 'data': {'text': 'hello'}});
ws.sendBinary(packed);

final data = unpack(packed);  // Map<String, dynamic>
```

---

## NetworkQuality

```dart
class NetworkQualityEvaluator {
  void recordRtt(int rttMs);
  void recordFailure();
  NetworkQuality evaluate();
  void reset();
}

enum NetworkQuality { excellent, good, fair, poor }
```

---

## 内存管理

Dart 侧通过 `Finalizer` 自动释放 Rust 侧资源。调用 `dispose()` 可手动提前释放：

```dart
client.dispose();  // 释放 HTTP 客户端 + Rust handle
ws.dispose();      // 释放 WebSocket 客户端 + Rust handle
```

---

## 与 Node.js 的 API 对应

| napi (推荐) | Flutter |
|---------|---------|
| `new HttpClient(config)` | `CatcherHttpClient(config)` |
| `client.get(url)` | `client.get(path)` |
| `client.post(url, body)` | `client.post(path, body: data)` |
| `new SseStream(config, cb)` | ❌ (使用 `SseClient`) |
| `new SseClient(config, cb)` | ❌ |
| `new WsClient(config, cb)` | `CatcherWsClient(config)` |
| `event.type === 'Message'` + `event.data_base64` | `event is WsMessageEvent` + `event.binary` |
| `ws.send(data)` | `ws.sendText(data)` / `ws.sendBinary(data)` |
| `client.circuitBreakerState()` | `client.circuitBreakerState` |
| `client.metrics()` | `client.queueDepth` |

> napi 包现在提供类型安全的 TS wrapper（`HttpClient`、`WsClient`、`SseStream`、`SseClient`），
> 配置支持对象或 JSON 字符串，事件回调直接返回强类型对象（无需 `JSON.parse`）。
> 详见 [napi API 文档](./napi.md)。
