# Flutter 使用指南

> 状态：✅ 已实现 — `catcher_core` pub 包 + dart:ffi 绑定层  
> 代码位置：`packages/catcher_core/`  
> 架构文档：[`arch-rs/13-dart-ffi.md`](../arch-rs/13-dart-ffi.md)

---

## 一、架构概览

```
Flutter App (Dart)
      │ dart:ffi — 直接调用 C ABI
      ▼
libcatcher_core.so / .dylib (Rust cdylib)
      │ 复用 src/ffi/ — 与 napi-rs 同一套 C ABI
      ▼
catcher-rs (Rust 核心)
```

**为什么是 dart:ffi 而不是 flutter_rust_bridge？**

| | dart:ffi ✅ | flutter_rust_bridge ❌ |
|--|-----------|---------------------|
| 构建时间 | ~9min | 18min+ (codegen + 双编) |
| 复用 C ABI | ✅ 直接调 `src/ffi/` | ❌ 需重写 Rust API |
| 依赖 | Dart 内置 | codegen + LLVM + cmake |
| 兼容性 | 无已知问题 | Nix/macOS 已知问题 |

---

## 二、依赖

```yaml
# pubspec.yaml
dependencies:
  catcher_core: ^0.1.0
```

`catcher_core` 是一个 pub.dev 包，内部包含：
- Dart 封装层（`lib/src/http_client.dart` 等）
- Rust 动态库（预编译 `.so` / `.dylib`，或通过 `native_assets` 自动构建）

---

## 三、HTTP 客户端

```dart
import 'package:catcher_core/catcher_core.dart';

void main() async {
  final client = CatcherHttpClient(HttpClientConfig(
    baseUrl: 'https://api.example.com',
    connectTimeoutMs: 5000,
    responseTimeoutMs: 30000,
    keepAlive: true,
    retry: RetryConfig(
      maxAttempts: 3,
      backoff: 'exponential',
    ),
    circuitBreaker: CircuitBreakerConfig(
      failureThreshold: 5,
      resetTimeoutMs: 30000,
    ),
  ));

  // GET
  final channels = await client.get('/channels');

  // POST
  final result = await client.post('/messages', body: {'text': 'hello'});

  // 带 query 参数
  final search = await client.get('/search',
    queryParams: {'q': 'test', 'page': '1'},
  );

  // 取消请求
  final token = CancelToken();
  Future.delayed(Duration(seconds: 5), () => token.cancel());
  final resp = await client.get('/slow',
    cancelToken: token,
  );

  // 释放资源
  client.dispose();
}
```

---

## 四、WebSocket 客户端

```dart
final ws = CatcherWsClient(WsClientConfig(
  urls: ['wss://cn.example.com', 'wss://sg.example.com'],  // 多区域竞速
  perMessageDeflate: true,
  reconnect: ReconnectConfig(
    initialDelayMs: 1000,
    maxDelayMs: 30000,
    maxAttempts: 20,
  ),
));

// 事件流
ws.events.listen((event) {
  switch (event.type) {
    case WsEventType.open:
      print('connected to ${event.endpoint}');
    case WsEventType.message:
      print('received: ${event.data}');
    case WsEventType.close:
      print('closed: ${event.code}');
    case WsEventType.error:
      print('error: ${event.error}');
  }
});

ws.sendText('hello');
ws.sendBinary(Uint8List.fromList([1, 2, 3]));
ws.close();
```

---

## 五、二进制编解码

```dart
import 'package:catcher_core/catcher_core.dart';

// msgpack 编码（通过 Rust core）
final packed = pack({'event': 'message', 'data': {'text': 'hello'}});
ws.sendBinary(packed);

// msgpack 解码
final unpacked = unpack(packed);  // Map<String, dynamic>
```

---

## 六、平台构建

### 交叉编译目标

| 平台 | Rust target | 产物 |
|------|------------|------|
| Android arm64 | `aarch64-linux-android` | `libcatcher_core.so` |
| iOS arm64 | `aarch64-apple-ios` | static linking |
| macOS arm64 | `aarch64-apple-darwin` | `libcatcher_core.dylib` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `catcher_core.dll` |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `libcatcher_core.so` |

### Flutter 3.38+ native_assets

```bash
flutter create --template=package_ffi catcher_core
cd catcher_core
flutter pub get
flutter build apk   # 自动编译 Rust + 打包 .so
```

编译产物自动嵌入 app bundle，Dart 侧通过 `DynamicLibrary.process()` 加载。

---

## 七、与 Node.js 的 API 对应

| Node.js | Flutter |
|---------|---------|
| `createHttpClient(config)` | `CatcherHttpClient(config)` |
| `client.get(url)` | `client.get(path)` |
| `client.post(url, body)` | `client.post(path, body: data)` |
| `createResilientWS(options)` | `CatcherWsClient(config)` |
| `ws.addEventListener('message', fn)` | `ws.events.listen(fn)` |
| `ws.send(data)` | `ws.sendText(data)` / `ws.sendBinary(data)` |
| `pack(obj)` / `unpack(buf)` | `pack(obj)` / `unpack(buf)` |
| `client.interceptors.request.use(fn)` | Dart 拦截器（规划中）|
| `client.circuitBreakerState()` | `client.circuitBreakerState` |
| `client.queueDepth()` | `client.queueDepth` |

---

## 八、内存管理

Dart 侧通过 `Finalizer` 自动释放 Rust 侧资源，无需手动管理：

```dart
class CatcherHttpClient {
  late final ffi.Pointer<ffi.Void> _handle;
  static final _finalizer = Finalizer<ffi.Pointer<ffi.Void>>((handle) {
    _catcherHttpClientDestroy(handle);
  });

  CatcherHttpClient(HttpClientConfig config) {
    _handle = _catcherHttpClientCreate(configJson);
    _finalizer.attach(this, _handle, detach: this);
  }
}
```

---

## 九、当前限制

| 功能 | 状态 |
|------|------|
| HTTP GET/POST/PUT/DELETE/PATCH | 📋 规划 |
| keepAlive + DNS 缓存 | 📋 规划 |
| retry + 退避 | 📋 规划 |
| circuitBreaker | 📋 规划 |
| per-request options | 📋 规划 |
| 动态拦截器 | ❌ 暂不支持（dart:ffi 回调限制） |
| WebSocket + push 事件 | 📋 规划 |

> Flutter 使用方式依赖于 Rust crate 层的实现。Node.js 侧的所有韧性特性最终都会通过 C ABI → dart:ffi 通路在 Flutter 侧可用。
