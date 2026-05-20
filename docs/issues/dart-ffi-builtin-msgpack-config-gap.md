# Dart/FFI 未暴露内置 msgpack 开关

## 严重程度：P1

Rust HTTP 和 WS 传输层已经支持内置 msgpack 编解码，但 Dart 封装层没有暴露 `msgpack` 配置项。Dart 用户目前只能使用手动 `pack/unpack`，不能启用传输层自动编解码。

这个问题和 `catcher-napi-builtin-msgpack-codec.md` 有关联，但当前底层状态不同：Rust 传输层已经实现了内置 msgpack，缺口主要在 Dart 封装层。

## 位置

- `packages/catcher-http/src/types/http.rs` — Rust `HttpClientConfig` 已有 `msgpack: bool`
- `packages/catcher-http/src/transport/http_client.rs` — HTTP 发送前 JSON → msgpack，接收后 msgpack → JSON
- `packages/catcher-ws/src/types/ws.rs` — Rust `WsClientConfig` 已有 `msgpack: bool`
- `packages/catcher-ws/src/transport/ws_client.rs` — WS 发送文本 / 接收二进制消息路径已处理 msgpack
- `packages/catcher_core/lib/src/http_client.dart` — Dart `HttpClientConfig` 没有 `msgpack`
- `packages/catcher_core/lib/src/ws_client.dart` — Dart `WsClientConfig` 没有 `msgpack`
- `packages/catcher_core/lib/src/codec.dart` — Dart 只暴露手动 `pack/unpack`
- `packages/catcher-ffi/src/lib.rs` — C ABI 只额外提供手动 `catcher_pack` / `catcher_unpack`

## 现象

### Rust 传输层已支持内置 msgpack

HTTP 配置中已有：

```rust
pub msgpack: bool
```

开启后，HTTP 传输层会：

1. 请求发送前把 JSON body 编码成 msgpack
2. 设置 `Content-Type: application/msgpack`
3. 响应是 msgpack 时解码回 JSON bytes

WS 配置中也已有：

```rust
pub msgpack: bool
```

开启后，WS 传输层会：

1. `send_text()` 传入 JSON 字符串时，转成 msgpack binary frame
2. 收到 msgpack binary frame 时，转回 JSON text event

### Dart 封装层没有开关

Dart `HttpClientConfig` 没有 `msgpack` 字段，`toJson()` 也不会输出 `msgpack`。

Dart `WsClientConfig` 同样没有 `msgpack` 字段。

所以 Dart 用户无法通过公开 API 启用 Rust 传输层的内置 msgpack。

### 只能走手动 pack/unpack

Dart 当前只提供：

```dart
Uint8List pack(dynamic value)
dynamic unpack(Uint8List data)
```

这条路径需要：

1. Dart value → JSON 字符串
2. JSON 字符串跨 FFI 传给 Rust
3. Rust 解析 JSON
4. Rust 编码 msgpack

这和传输层内置 msgpack 的目标不同。内置 msgpack 是在请求发送和消息收发路径内部完成编解码，调用方不用手动处理网络传输格式。

## 影响

1. Flutter/Dart 用户无法使用已经实现的 HTTP 内置 msgpack。
2. Flutter/Dart 用户无法使用已经实现的 WS 内置 msgpack。
3. 用户可能误用手动 `pack/unpack`，导致调用方式更复杂，也无法自动处理响应解码。
4. Dart API 与 NAPI API 能力不一致。

## 不是底层 C ABI 完全缺失

原始 C ABI 的 `catcher_http_client_create(config_json)` 和 `catcher_ws_create(config_json, ...)` 都是直接把 JSON 解析成 Rust config。

理论上，如果调用方自己构造 JSON 并传入：

```json
{ "msgpack": true }
```

Rust 侧可以识别。

但 Dart 封装层是公开 API。当前公开的 Dart config 类型没有这个字段，所以正常 Dart 用户用不到这个能力。

## 建议修复

### HTTP Dart

给 `HttpClientConfig` 增加：

```dart
final bool msgpack;
```

默认值：

```dart
this.msgpack = false,
```

`toJson()` 增加：

```dart
'msgpack': msgpack,
```

### WS Dart

给 `WsClientConfig` 增加：

```dart
final bool msgpack;
```

默认值：

```dart
this.msgpack = false,
```

`toJson()` 增加：

```dart
'msgpack': msgpack,
```

### 文档

Dart 文档需要说明：

- `msgpack: false` 是默认值，body/message 原样传输
- `msgpack: true` 时，HTTP body 应是 JSON
- `msgpack: true` 时，WS `sendText()` 应传 JSON 字符串
- 手动 `pack/unpack` 仍保留，但它不是传输层内置编解码

## 测试建议

- Dart HTTP：`msgpack: true` 后，服务端收到 `Content-Type: application/msgpack`
- Dart HTTP：服务端返回 msgpack，Dart `HttpResponse.bodyAsString` 是 JSON 字符串
- Dart WS：`msgpack: true` 后，服务端收到 binary frame
- Dart WS：服务端返回 msgpack binary frame，Dart 收到 text JSON
- Dart config 单元测试：`toJson()` 包含 `msgpack`

## 验收标准

- [ ] Dart `HttpClientConfig` 暴露 `msgpack`
- [ ] Dart `WsClientConfig` 暴露 `msgpack`
- [ ] Dart HTTP 内置 msgpack 集成测试通过
- [ ] Dart WS 内置 msgpack 集成测试通过
- [ ] 文档区分“手动 pack/unpack”和“传输层内置 msgpack”
