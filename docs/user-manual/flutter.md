# Flutter 使用指南

> 状态：✅ 已发布 — `catcher_core` [pub.dev](https://pub.dev/packages/catcher_core) v0.1.0 + dart:ffi 绑定层  
> 代码位置：`packages/catcher_core/`  
> 架构文档：[`arch-rs/13-dart-ffi.md`](../arch-rs/13-dart-ffi.md)

---

## 一、架构概览

```
Flutter App (Dart)
      │ dart:ffi — 直接调用 C ABI
      ▼
libcatcher_ffi.so / .dylib (catcher-ffi cdylib umbrella)
      │ 25 C ABI symbols — HTTP + WS + pack/unpack
      ▼
catcher-http + catcher-ws (Rust)
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
| Android arm64 | `aarch64-linux-android` | `libcatcher_ffi.so` |
| iOS arm64 | `aarch64-apple-ios` | static linking |
| macOS arm64 | `aarch64-apple-darwin` | `libcatcher_ffi.dylib` |
| Windows x86_64 | `x86_64-pc-windows-msvc` | `catcher_ffi.dll` |
| Linux x86_64 | `x86_64-unknown-linux-gnu` | `libcatcher_ffi.so` |

### Flutter 3.38+ native_assets

```bash
flutter create --template=package_ffi catcher_core
cd catcher_core
flutter pub get
flutter build apk   # 自动编译 Rust + 打包 .so
```

编译产物自动嵌入 app bundle，Dart 侧通过 `DynamicLibrary.process()` 加载。

### 手动构建 Android / iOS native 二进制

当需要在本地重新生成 `catcher_core` 的 Android `.so` 或 iOS `XCFramework` 时，从仓库根目录执行。以下步骤只覆盖 Android 和 iOS，不发布 pub.dev 包。

#### 前置依赖

- Rust toolchain: `cargo`、`rustup`
- Android: Android SDK + NDK，建议使用 release workflow 同版本 `27.0.12077973`
- iOS: Xcode command line tools，包含 `xcodebuild`、`lipo`、`install_name_tool`

```bash
rustup target add \
  aarch64-linux-android \
  armv7-linux-androideabi \
  i686-linux-android \
  x86_64-linux-android \
  aarch64-apple-ios \
  aarch64-apple-ios-sim \
  x86_64-apple-ios
```

#### Android

`packages/catcher_core/scripts/build_native.sh android` 会设置 Cargo linker，但部分 Rust 依赖（例如 `ring` 通过 `cc-rs` 编译 C/ASM）还会读取 `CC_*`。NDK 新版本只提供带 API 后缀的 clang，例如 `aarch64-linux-android24-clang`，因此手动构建时需要显式设置 `CC_*`。

```bash
cd /path/to/catcher

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/27.0.12077973}"
export ANDROID_API="${ANDROID_API:-24}"

case "$(uname -s)" in
  Darwin) HOST_TAG="darwin-x86_64" ;;
  Linux) HOST_TAG="linux-x86_64" ;;
  *) echo "Unsupported Android build host: $(uname -s)" >&2; exit 1 ;;
esac

TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"

export CC_aarch64_linux_android="$TOOLCHAIN/aarch64-linux-android${ANDROID_API}-clang"
export CC_armv7_linux_androideabi="$TOOLCHAIN/armv7a-linux-androideabi${ANDROID_API}-clang"
export CC_i686_linux_android="$TOOLCHAIN/i686-linux-android${ANDROID_API}-clang"
export CC_x86_64_linux_android="$TOOLCHAIN/x86_64-linux-android${ANDROID_API}-clang"

export AR_aarch64_linux_android="$TOOLCHAIN/llvm-ar"
export AR_armv7_linux_androideabi="$TOOLCHAIN/llvm-ar"
export AR_i686_linux_android="$TOOLCHAIN/llvm-ar"
export AR_x86_64_linux_android="$TOOLCHAIN/llvm-ar"

packages/catcher_core/scripts/build_native.sh android
```

输出位置：

```text
packages/catcher_core/android/src/main/jniLibs/arm64-v8a/libcatcher_ffi.so
packages/catcher_core/android/src/main/jniLibs/armeabi-v7a/libcatcher_ffi.so
packages/catcher_core/android/src/main/jniLibs/x86/libcatcher_ffi.so
packages/catcher_core/android/src/main/jniLibs/x86_64/libcatcher_ffi.so
```

#### iOS

仓库脚本的 `apple` target 会同时构建 iOS 和 macOS。如果只需要 iOS，可按下面步骤编译 device/simulator 三个 target，并组装 `catcher_ffi.xcframework`。

```bash
cd /path/to/catcher

export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-15.0}"

rustup target add \
  aarch64-apple-ios \
  aarch64-apple-ios-sim \
  x86_64-apple-ios

cargo build --release -p catcher-ffi --target aarch64-apple-ios --manifest-path packages/Cargo.toml
cargo build --release -p catcher-ffi --target aarch64-apple-ios-sim --manifest-path packages/Cargo.toml
cargo build --release -p catcher-ffi --target x86_64-apple-ios --manifest-path packages/Cargo.toml

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

lipo -create \
  packages/target/aarch64-apple-ios-sim/release/libcatcher_ffi.dylib \
  packages/target/x86_64-apple-ios/release/libcatcher_ffi.dylib \
  -output "$TMP_DIR/libcatcher_ffi_ios_sim.dylib"

make_framework() {
  local source="$1"
  local framework_dir="$2"
  local bundle_id="$3"

  rm -rf "$framework_dir"
  mkdir -p "$framework_dir"
  cp "$source" "$framework_dir/catcher_ffi"
  install_name_tool -id "@rpath/catcher_ffi.framework/catcher_ffi" "$framework_dir/catcher_ffi"
  cat > "$framework_dir/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>catcher_ffi</string>
  <key>CFBundleIdentifier</key>
  <string>${bundle_id}</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>catcher_ffi</string>
  <key>CFBundlePackageType</key>
  <string>FMWK</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>MinimumOSVersion</key>
  <string>${IPHONEOS_DEPLOYMENT_TARGET}</string>
</dict>
</plist>
PLIST
}

make_framework \
  packages/target/aarch64-apple-ios/release/libcatcher_ffi.dylib \
  "$TMP_DIR/ios-device/catcher_ffi.framework" \
  "com.eric8810.catcher.ffi.ios"

make_framework \
  "$TMP_DIR/libcatcher_ffi_ios_sim.dylib" \
  "$TMP_DIR/ios-simulator/catcher_ffi.framework" \
  "com.eric8810.catcher.ffi.ios-simulator"

rm -rf packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework
mkdir -p packages/catcher_core/ios/Frameworks

xcodebuild -create-xcframework \
  -framework "$TMP_DIR/ios-device/catcher_ffi.framework" \
  -framework "$TMP_DIR/ios-simulator/catcher_ffi.framework" \
  -output packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework
```

输出位置：

```text
packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework
```

#### 校验

```bash
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/27.0.12077973}"
case "$(uname -s)" in
  Darwin) HOST_TAG="darwin-x86_64" ;;
  Linux) HOST_TAG="linux-x86_64" ;;
  *) echo "Unsupported Android build host: $(uname -s)" >&2; exit 1 ;;
esac
TOOLCHAIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"

# Android ABI
file packages/catcher_core/android/src/main/jniLibs/*/libcatcher_ffi.so

# iOS framework slices
lipo -info packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework/ios-arm64/catcher_ffi.framework/catcher_ffi
lipo -info packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework/ios-arm64_x86_64-simulator/catcher_ffi.framework/catcher_ffi

# XCFramework metadata
plutil -p packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework/Info.plist
plutil -extract MinimumOSVersion raw packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework/ios-arm64/catcher_ffi.framework/Info.plist
plutil -extract MinimumOSVersion raw packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework/ios-arm64_x86_64-simulator/catcher_ffi.framework/Info.plist

# Core FFI symbols
"$TOOLCHAIN/llvm-nm" -gD packages/catcher_core/android/src/main/jniLibs/arm64-v8a/libcatcher_ffi.so \
  | rg 'catcher_http_execute$|catcher_ws_create$|catcher_free_result$|catcher_sse_connect$'
nm -gU packages/catcher_core/ios/Frameworks/catcher_ffi.xcframework/ios-arm64/catcher_ffi.framework/catcher_ffi \
  | rg '_catcher_http_execute$|_catcher_ws_create$|_catcher_free_result$|_catcher_sse_connect$'
```

> `android/src/main/jniLibs/` 和 `ios/Frameworks/` 下的二进制产物默认被 git ignore。发布前请确认这些文件实际存在；pub.dev 打包依赖 `.pubignore` 中对平台 bundle 目录的例外规则。

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
| `client.interceptors.request.use(fn)` | ❌ 不支持（dart:ffi 回调限制） |
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

## 九、当前状态

| 功能 | 状态 |
|------|------|
| HTTP GET/POST/PUT/DELETE/PATCH | ✅ 已实现（Rust crate + Dart wrapper） |
| keepAlive + DNS 缓存 | ✅ 已实现（Rust crate 支持） |
| retry + 退避 | ✅ 已实现（Rust crate 支持） |
| circuitBreaker | ✅ 已实现（Rust crate 支持） |
| per-request options | ⏳ Dart wrapper 待补 |
| 动态拦截器 | ❌ 暂不支持（dart:ffi 回调限制） |
| WebSocket + push 事件 | ✅ 已实现（Dart wrapper + Rust FFI） |
| 二进制编解码 (pack/unpack) | ✅ 已实现（Rust codec via FFI） |

> Rust crate 已完整实现所有韧性特性。Dart wrapper 覆盖了 HTTP CRUD、WebSocket 和二进制编解码。
> 测试：20/20 单元测试 + 8/8 集成测试（真实 FFI + httpbin.org）全部通过。
