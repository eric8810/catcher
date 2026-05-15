# 13 — Dart FFI 绑定层架构

> 代码位置：`packages/catcher_core/`
> Rust FFI：`crates/catcher-ffi/`（cdylib umbrella，桥接 catcher-http + catcher-ws）
> 目标平台：Android / iOS / macOS / Windows / Linux

---

## 设计原则

| 原则 | 说明 |
|------|------|
| **C ABI 唯一事实来源** | Dart 侧只通过 `dart:ffi` 调用 `src/ffi/` 导出的 `extern "C"` 函数，不重复定义业务逻辑 |
| **Dart-idiomatic wrapper** | C ABI 是"系统级"API，Dart 侧封一层 `HttpClient` / `WsClient` / `Codec` 类，对上层暴露 Future / Stream |
| **零序列化中转** | 二进制数据 (`Uint8List` ↔ `*const u8`) 零拷贝，只传指针+长度 |
| **异步桥接** | Rust 的 async 通过 `Isolate` + `ReceivePort` 桥接到 Dart 的 `Future` |
| **Native Assets** | 编译产物通过 Flutter 3.38+ `native_assets` 系统自动打包，不手动拷贝 .so |

---

## 决策：dart:ffi vs flutter_rust_bridge

| 维度 | dart:ffi (✅ 选定) | flutter_rust_bridge |
|------|-------------------|---------------------|
| 构建时间 | ~9min (cargo + dart) | ~18min+ (codegen + 双编译) |
| 依赖 | Dart 内置，零额外依赖 | FRB codegen + LLVM + cmake |
| 已有 C ABI 复用 | ✅ 直接复用 `src/ffi/` | ❌ 需按 FRB 风格重写 Rust API |
| 异步 | 手动封 Isolate/ReceivePort | 内置 async |
| Stream | 手动 `dart:async` Stream | 内置 `StreamSink` |
| 平台兼容性 | 无已知问题 | Nix/macOS 有已知兼容问题 |
| Flutter 3.38+ | 官方推荐 `package_ffi` 模板 | 非官方推荐 |
| 社区案例 | Signal (libsignal), FFI 论坛多数案例 | 少数大型项目 |

---

## 整体架构图

```
┌─────────────────────────────────────────────────────────────┐
│                    Dart / Flutter 应用                        │
│                                                              │
│  import 'package:catcher_core/catcher_core.dart';            │
│  final client = CatcherHttpClient(config);                   │
│  final resp = await client.get('/messages');                 │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────┴──────────────────────────────────┐
│              catcher_core (Dart package)                     │
│                                                              │
│  lib/                                                        │
│  ├── catcher_core.dart        # 公开 API                     │
│  ├── src/                                                     │
│  │   ├── http_client.dart     # CatcherHttpClient             │
│  │   ├── ws_client.dart       # CatcherWsClient               │
│  │   ├── codec.dart           # pack / unpack                 │
│  │   ├── quality.dart         # evaluateQuality               │
│  │   ├── ffi_bindings.dart    # 自动生成的 C 函数签名绑定       │
│  │   └── native_loader.dart   # 平台动态库加载                 │
│  └── catcher_core.dart                                        │
│                                                              │
│  Dart 层封装：C ABI → Future / Stream / Uint8List            │
└──────────────────────────┬──────────────────────────────────┘
                           │ dart:ffi
┌──────────────────────────┴──────────────────────────────────┐
│         libcatcher_ffi.so / .dylib / .dll (Rust)           │
│                                                              │
│  crates/catcher-ffi/src/                                     │
│  ├── types.rs           # FfiResult, FfiString, FfiBytes     │
│  ├── http.rs            # catcher_http_*                     │
│  ├── ws.rs              # catcher_ws_*                       │
│  ├── codec.rs           # catcher_pack / catcher_unpack      │
│  └── quality.rs         # catcher_evaluate_quality           │
└─────────────────────────────────────────────────────────────┘
```

---

## C ABI 类型映射

### 基础类型

| Rust C ABI (`src/ffi/types_ffi.rs`) | Dart (`dart:ffi`) | 说明 |
|-------------------------------------|-------------------|------|
| `*mut c_void` (handle) | `ffi.Pointer<ffi.Void>` | 不透明句柄 |
| `*const c_char` (json input) | `ffi.Pointer<ffi.Char>` | C 字符串 |
| `*const u8` + `len` | `ffi.Pointer<ffi.Uint8>` + `int` | 二进制数据 |
| `i32` (error_code) | `int` | 0=成功 |
| `u16` (status code) | `int` | HTTP 状态码 |
| `EventCallback` fn ptr | `ffi.Pointer<ffi.NativeFunction<...>>` | 异步回调 |

### 复合类型

| Rust `#[repr(C)]` | Dart | 转换方式 |
|-------------------|------|---------|
| `FfiResult` | `FfiResult` (Dart class) | 按字段逐一映射 |
| `FfiString` | `String` | `ffi.Char` pointer → `dart:ffi.Utf8` → `String` |
| `FfiBytes` | `Uint8List` | `ffi.Uint8` pointer + length → `Uint8List.sublistView()` |

### Dart 侧 FfiResult 定义

```dart
// lib/src/ffi_bindings.dart — 与 Rust #[repr(C)] FfiResult 对齐
final class FfiResult extends ffi.Struct {
  @ffi.Int32()
  external int errorCode;

  external ffi.Pointer<ffi.Char> errorMessage;

  external ffi.Pointer<ffi.Void> data;

  @ffi.Size()
  external int dataLen;
}
```

---

## 动态库加载 (`native_loader.dart`)

```dart
import 'dart:ffi';
import 'dart:io';

DynamicLibrary loadCatcherLibrary() {
  if (Platform.isAndroid) {
    return DynamicLibrary.open('libcatcher_ffi.so');
  } else if (Platform.isIOS) {
    // iOS: static linking via Native Assets
    return DynamicLibrary.process();
  } else if (Platform.isMacOS) {
    return DynamicLibrary.open('libcatcher_ffi.dylib');
  } else if (Platform.isWindows) {
    return DynamicLibrary.open('catcher_ffi.dll');
  } else if (Platform.isLinux) {
    return DynamicLibrary.open('libcatcher_ffi.so');
  }
  throw UnsupportedError('Unsupported platform: ${Platform.operatingSystem}');
}
```

> Flutter 3.38+ `native_assets` 系统可以替代手动 `DynamicLibrary.open()`，
> 编译产物自动打包到 app bundle，Dart 侧直接用 `DynamicLibrary.process()` 加载。

---

## C 函数签名绑定 (`ffi_bindings.dart`)

```dart
import 'dart:ffi';

// ── 类型别名 ──
typedef EventCallbackNative = ffi.Void Function(
  ffi.Pointer<ffi.Char> eventType,
  ffi.Pointer<ffi.Uint8> eventData,
  ffi.Size eventDataLen,
  ffi.Pointer<ffi.Void> userData,
);
typedef EventCallbackDart = void Function(
  ffi.Pointer<ffi.Char> eventType,
  ffi.Pointer<ffi.Uint8> eventData,
  int eventDataLen,
  ffi.Pointer<ffi.Void> userData,
);

// ── HTTP ──
typedef CatcherHttpClientCreateNative = ffi.Pointer<ffi.Void> Function(
  ffi.Pointer<ffi.Char> configJson,
);
typedef CatcherHttpClientDestroyNative = ffi.Void Function(
  ffi.Pointer<ffi.Void> handle,
);

// ── Codec ──
typedef CatcherPackNative = FfiResult Function(
  ffi.Pointer<ffi.Char> jsonInput,
);
typedef CatcherUnpackNative = FfiResult Function(
  ffi.Pointer<ffi.Uint8> data,
  ffi.Size len,
);
```

---

## 异步桥接设计

Rust C ABI 使用 `EventCallback` 函数指针实现异步回调。Dart 侧需要把这个回调模型转为 `Future`：

```
┌──────────┐  create(config)   ┌──────────────┐
│  Dart    │ ────────────────▶ │  Rust        │
│          │                   │              │
│  Future  │ ◀── ReceivePort ──│  spawn thread│
│  .then() │     .send(result) │  block_on    │
└──────────┘                   └──────────────┘
```

```dart
// lib/src/http_client.dart
class CatcherHttpClient {
  late final ffi.Pointer<ffi.Void> _handle;
  final _catcherHttpClientCreate = _lib
      .lookup<ffi.NativeFunction<CatcherHttpClientCreateNative>>(
        'catcher_http_client_create',
      )
      .asFunction();

  CatcherHttpClient(HttpClientConfig config) {
    final configJson = jsonEncode(config.toJson()).toNativeUtf8();
    _handle = _catcherHttpClientCreate(configJson.cast<ffi.Char>());
    malloc.free(configJson);
  }

  Future<HttpResponse> get(String url) async {
    final receivePort = ReceivePort();
    final urlNative = url.toNativeUtf8();

    // 调用 Rust async + 回调 → 桥接到 Dart Future
    _catcherHttpGet(
      _handle,
      urlNative.cast<ffi.Char>(),
      // 回调函数：Rust 完成后调用
      ffi.Pointer.fromFunction(_onHttpResult),
      receivePort.sendPort.nativePort,
    );

    final result = await receivePort.first as Map<String, dynamic>;
    receivePort.close();
    return HttpResponse.fromJson(result);
  }
}
```

### 时序图

```
Dart                    Rust (C ABI)              Rust (tokio)
 │                         │                         │
 │── http_get(handle,url)─▶│                         │
 │                         │── spawn(async { }) ────▶│
 │                         │                         │── reqwest.get().await
 │   (Dart event loop      │                         │   ...
 │    continues,            │                         │── result
 │    not blocked)          │                         │
 │                         │◀── callback(result) ────│
 │◀── ReceivePort.send ────│                         │
 │   Future completes       │                         │
```

---

## WebSocket Stream 桥接

WS 的推送模型用 Dart `Stream` 表达：

```dart
class CatcherWsClient {
  late final StreamController<WsEvent> _eventController;

  Stream<WsEvent> get events => _eventController.stream;

  CatcherWsClient(WsClientConfig config) {
    _eventController = StreamController<WsEvent>.broadcast();

    // Rust 侧通过 EventCallback 持续推送事件
    // Dart 侧转发到 StreamController
    final callback = ffi.Pointer.fromFunction(_onWsEvent);
    _handle = _catcherWsCreate(configJson, callback, ...);
  }

  void _onWsEvent(
    ffi.Pointer<ffi.Char> eventType,
    ffi.Pointer<ffi.Uint8> eventData,
    int eventDataLen,
    ffi.Pointer<ffi.Void> userData,
  ) {
    final json = eventData.cast<ffi.Utf8>().toDartString(length: eventDataLen);
    final event = WsEvent.fromJson(jsonDecode(json));
    _eventController.add(event);
  }
}
```

---

## 内存管理

| 对象 | 生命周期 | 清理方式 |
|------|---------|---------|
| Rust handle (`*mut c_void`) | 创建至显式 destroy | `catcher_http_client_destroy(handle)` |
| `FfiResult.error_message` | Rust 创建，调用方释放 | `FfiResult` Drop 时 `CString::from_raw` → Dart 侧 `calloc.free()` |
| `FfiResult.data` (二进制) | Rust 创建，Dart 侧复制后释放 | `Uint8List.sublistView()` 复制 → `calloc.free()` |
| Dart `ReceivePort` | Future 完成时关闭 | `receivePort.close()` |
| Dart `StreamController` | WS 关闭时 | `streamController.close()` |

```dart
class CatcherHttpClient {
  void dispose() {
    _catcherHttpClientDestroy(_handle);
    // handle 指针本身通过 malloc 分配，destroy 后释放
  }
}

// 配合 Dart Finalizer 自动清理
final _finalizer = Finalizer<ffi.Pointer<ffi.Void>>((handle) {
  _catcherHttpClientDestroy(handle);
});
```

---

## 包结构

```
packages/catcher_core/
├── pubspec.yaml                 # pub.dev 包配置
├── README.md
├── CHANGELOG.md
├── LICENSE
│
├── rust/                        # Rust 动态库源码
│   ├── Cargo.toml               # [lib] crate-type = ["cdylib"]
│   └── src/
│       └── lib.rs               # re-export catcher-rs ffi symbols
│
├── lib/                         # Dart 源码
│   ├── catcher_core.dart        # 公开 API export
│   └── src/
│       ├── ffi_bindings.dart    # C 函数签名 (dart:ffi)
│       ├── ffi_types.dart       # FfiResult, FfiString, FfiBytes
│       ├── native_loader.dart   # 平台动态库加载
│       ├── http_client.dart     # CatcherHttpClient
│       ├── ws_client.dart       # CatcherWsClient
│       ├── codec.dart           # pack / unpack
│       ├── quality.dart         # evaluateQuality
│       └── models/              # Dart 模型类
│           ├── http_config.dart
│           ├── http_response.dart
│           ├── ws_config.dart
│           └── ws_event.dart
│
├── test/                        # Dart 测试
│   ├── http_client_test.dart
│   ├── ws_client_test.dart
│   ├── codec_test.dart
│   └── quality_test.dart
│
└── example/                     # Flutter 示例应用
    └── lib/main.dart
```

---

## pubspec.yaml

```yaml
name: catcher_core
version: 0.1.0
description: Resilient HTTP/WebSocket client backed by Rust core for Flutter
repository: https://github.com/eric8810/catcher
platforms:
  android:
  ios:
  macos:
  windows:
  linux:

environment:
  sdk: ^3.8.0
  flutter: '>=3.38.0'
```

---

## Rust 动态库 crate

`catcher-ffi` 是 cdylib umbrella crate，桥接 `catcher-http` + `catcher-ws`，导出全部 16 个 C ABI 符号：

```toml
# crates/catcher-ffi/Cargo.toml
[package]
name = "catcher-ffi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
catcher-http = { path = "../catcher-http" }
catcher-ws = { path = "../catcher-ws" }
catcher-core = { path = "../catcher-core" }
tokio = { version = "1", features = ["rt-multi-thread"] }
```

`crates/catcher-ffi/src/lib.rs` — 使用 `block_on_aux_thread` 避免 tokio re-entrance：

```rust
// All 16 C ABI symbols exported:
// - catcher_http_client_create/destroy
// - catcher_http_get/post/put/delete/patch
// - catcher_ws_create/destroy
// - catcher_ws_send_text/send_binary/close
// - catcher_pack/catcher_unpack
// - catcher_evaluate_quality
// - catcher_free_result
```

---

## 构建与编译

```
flutter create --template=package_ffi catcher_core
```

编译流程：

```
1. cargo build --release  →  target/<triple>/release/libcatcher_ffi.so
2. Flutter native_assets   →  自动打包到 app bundle
3. dart:ffi 加载           →  DynamicLibrary.process() (iOS) / DynamicLibrary.open() (Android)
```

### 交叉编译目标

| 平台 | Rust target | 产物 |
|------|------------|------|
| Android arm64 | `aarch64-linux-android` | `libcatcher_ffi.so` |
| Android x86_64 | `x86_64-linux-android` | `libcatcher_ffi.so` |
| iOS arm64 | `aarch64-apple-ios` | static (嵌入) |
| iOS simulator | `aarch64-apple-ios-sim` | static (嵌入) |
| macOS arm64 | `aarch64-apple-darwin` | `libcatcher_ffi.dylib` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `catcher_ffi.dll` |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `libcatcher_ffi.so` |

---

## Dart 公开 API

```dart
// catcher_core.dart — 对外暴露的唯一入口

export 'src/http_client.dart';
export 'src/ws_client.dart';
export 'src/codec.dart';
export 'src/quality.dart';
export 'src/models/http_config.dart';
export 'src/models/http_response.dart';
export 'src/models/ws_config.dart';
export 'src/models/ws_event.dart';
```

### 使用示例

```dart
import 'package:catcher_core/catcher_core.dart';

void main() async {
  // HTTP
  final client = CatcherHttpClient(HttpClientConfig(
    baseUrl: 'https://api.example.com',
    connectTimeoutMs: 5000,
    responseTimeoutMs: 10000,
    keepAlive: true,
    retry: RetryConfig(maxAttempts: 3, backoff: 'exponential'),
  ));

  final resp = await client.get('/channels');
  print('${resp.status} ${resp.body}');

  // WebSocket
  final ws = CatcherWsClient(WsClientConfig(
    urls: ['wss://ws.example.com'],
    perMessageDeflate: true,
  ));

  ws.events.listen((event) {
    print('WS event: ${event.type}');
  });

  ws.sendText('hello');
  ws.close();

  // Codec
  final packed = pack({'hello': 'world'});
  final unpacked = unpack(packed); // '{"hello":"world"}'
}
```

---

## 测试策略

| 层级 | 框架 | 内容 |
|------|------|------|
| Rust 核心 | `cargo test` | 已有 96 tests，保持不变 |
| Dart 单元 | `dart test` | 20/20 通过 — ffi_bindings 加载 + codec roundtrip + 模型序列化 |
| Dart 集成 | `dart test` | 8/8 通过 — 加载真实 .so，HTTP GET httpbin.org |
| Flutter 集成 | `integration_test` | 完整 Flutter app 中加载 Rust 核心 |

---

## SSE 客户端绑定 (`sse_client.dart`) — 📐 新增

```dart
class CatcherSseClient {
  late final ffi.Pointer<ffi.Void> _handle;
  final StreamController<SseEvent> _eventController =
      StreamController<SseEvent>.broadcast();

  Stream<SseEvent> get events => _eventController.stream;

  /// 0=Connecting, 1=Open, 2=Closed
  SseReadyState get readyState {
    final state = _catcherSseReadyState(_handle);
    return SseReadyState.values[state];
  }

  String? get lastEventId {
    final ptr = _catcherSseLastEventId(_handle);
    if (ptr == ffi.nullptr) return null;
    final id = ptr.cast<ffi.Utf8>().toDartString();
    _catcherFreeData(ptr.cast());
    return id;
  }

  CatcherSseClient(SseClientConfig config) {
    final configJson = jsonEncode(config.toJson()).toNativeUtf8();
    final callback = ffi.Pointer.fromFunction(_onSseEvent);
    _handle = _catcherSseConnect(
      configJson.cast<ffi.Char>(),
      callback,
      ffi.nullptr,
    );
    malloc.free(configJson);
  }

  void close() => _catcherSseClose(_handle);

  void _onSseEvent(
    ffi.Pointer<ffi.Char> eventType,
    ffi.Pointer<ffi.Uint8> eventData,
    int eventDataLen,
    ffi.Pointer<ffi.Void> userData,
  ) {
    final json = eventData.cast<ffi.Utf8>().toDartString(length: eventDataLen);
    final event = SseEvent.fromJson(jsonDecode(json));
    _eventController.add(event);
  }
}

/// POST SSE 一次性流（适用于 Anthropic streaming API 等）
Future<List<SseEvent>> sseStream(
  CatcherHttpClient client,
  String method,
  String url, {
  Uint8List? body,
  Map<String, String>? headers,
}) async {
  final receivePort = ReceivePort();
  final results = <SseEvent>[];
  // ... C ABI callback → ReceivePort
  return results;
}

enum SseReadyState { connecting, open, closed }
```

## 请求取消机制 — 📐 新增

### C ABI 层

```rust
// catcher-ffi/src/http.rs
#[no_mangle]
pub extern "C" fn catcher_http_client_cancel_all(
    handle: *mut c_void,
)
```

### Dart 层封装

```dart
class CatcherHttpClient {
  void cancelAll() {
    _catcherHttpClientCancelAll(_handle);
  }

  void dispose() {
    cancelAll();  // 先取消所有飞行请求
    _catcherHttpClientDestroy(_handle);
  }
}
```

### 取消时序

```
Dart                          Rust (tokio)
 │                               │
 │── get('/slow') ──────────────▶│── reqwest.get() 开始
 │                               │   ...
 │── cancelAll() ───────────────▶│── cancel_token.cancel()
 │                               │   tokio::select! → Err(Cancelled)
 │◀── callback("error", ...) ────│
 │                               │
 │── get('/fast') ──────────────▶│── 新请求正常执行（token 已重置）
```

## WS 配置增强 — 📐

### Dart 类型补全

```dart
class WsClientConfig {
  final List<String> urls;
  final Map<String, String>? headers;          // 📐 新增
  final List<String>? protocols;               // 📐 新增
  final int deflateThresholdBytes;             // 📐 新增，默认 256
  final int raceCount;                         // 📐 新增，默认 1
  // ...existing fields...

  WsClientConfig({
    required this.urls,
    this.headers,
    this.protocols,
    this.deflateThresholdBytes = 256,
    this.raceCount = 1,
    // ...
  });

  Map<String, dynamic> toJson() => {
    'urls': urls,
    if (headers != null) 'headers': headers,
    if (protocols != null) 'protocols': protocols,
    'deflate_threshold_bytes': deflateThresholdBytes,
    'race_count': raceCount,
    // ...existing fields...
  };
}
```

## 取消 / 指标 / 熔断器状态查询 — 📐 新增

```dart
class CatcherHttpClient {
  // ...existing...

  /// 取消该客户端所有飞行请求
  void cancelAll() { ... }

  /// 查询熔断器状态
  CircuitBreakerState get circuitBreakerState {
    final json = _catcherHttpCircuitBreakerState(_handle).toDartString();
    return CircuitBreakerState.fromJson(jsonDecode(json));
  }

  /// 查询运行时指标
  MetricsSnapshot get metrics {
    final json = _catcherHttpMetrics(_handle).toDartString();
    return MetricsSnapshot.fromJson(jsonDecode(json));
  }
}

class CircuitBreakerState {
  final String state;        // "closed" | "half_open" | "open"
  final int failureCount;
  final int successCount;
}

class MetricsSnapshot {
  final int totalRequests;
  final int totalSuccess;
  final int totalErrors;
  final int totalRetries;
  final double avgLatencyMs;
  final double p50LatencyMs;
  final double p90LatencyMs;
  final double p99LatencyMs;
  final int activeConnections;
  final int circuitBreakerTrips;
}
```

---

## 与 napi-rs 绑定包的对应关系

| 概念 | napi-rs (Node.js) | dart:ffi (Dart) |
|------|-------------------|-----------------|
| 加载 | `require('catcher-rs-napi')` | `DynamicLibrary.open()` |
| 异步 | tokio runtime 自动桥接 | Isolate + ReceivePort |
| 回调 | `ThreadsafeFunction` | `ffi.Pointer.fromFunction()` |
| Buffer | `napi::bindgen_prelude::Buffer` | `Uint8List` |
| 销毁 | GC 自动 | `dispose()` + Finalizer |
| 构建 | `cargo build --release` | `cargo build --release` + `native_assets` |
| SSE 客户端 | ❌ 未实现 | 📐 SseClient + SseStream |
| 请求取消 | ❌ 未实现 | 📐 cancelAll() |
| 熔断器状态 | ✅ circuitBreakerState() | 📐 circuitBreakerState |
| 运行时指标 | ❌ 未实现 | 📐 metrics |
| WS headers/protocols | ❌ 未实现 | 📐 headers + protocols |
