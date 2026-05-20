# Dart/FFI DNS 配置未对齐 Rust 原生能力

## 严重程度：P1

HTTP 的 Rust 传输层已经补上 DNS 缓存和旧缓存兜底，但 Dart 封装层没有完整暴露这些字段；WebSocket 侧则仍然没有 DNS 配置入口。这个问题和 `catcher-napi-dns-cache-not-working.md` 有关联，但不是完全相同的问题。

简单说：

- HTTP：底层缓存已接入，但 Dart 只能配置一部分 DNS 字段
- WS：Rust WS 和 Dart WS 都没有 DNS 配置入口

## 位置

- `packages/catcher_core/lib/src/http_client.dart` — Dart `DnsConfig` 只暴露 `cacheTtlSecs` / `nameservers` / `hostMapping`
- `packages/catcher_core/lib/src/ws_client.dart` — Dart `WsClientConfig` 没有 `dns`
- `packages/catcher-http/src/types/http.rs` — Rust HTTP `DnsConfig` 已支持完整 DNS 缓存字段
- `packages/catcher-http/src/transport/http_client.rs` — HTTP 默认创建 `StaleAwareDnsResolver`
- `packages/catcher-ws/src/types/ws.rs` — Rust `WsClientConfig` 没有 `dns`
- `packages/catcher-ws/src/transport/ws_client.rs` — WS 直接调用 `tokio_tungstenite::connect_async_with_config`

## 现象

### HTTP：Dart 只能配置部分 DNS 能力

Rust HTTP 当前支持：

```rust
pub struct DnsConfig {
    pub cache_size: u64,
    pub cache_ttl_secs: u32,
    pub negative_ttl_secs: u32,
    pub stale_ttl_secs: u32,
    pub stale_on_error: bool,
    pub nameservers: Vec<String>,
    pub host_mapping: HashMap<String, String>,
}
```

Dart 只暴露：

```dart
class DnsConfig {
  final int cacheTtlSecs;
  final List<String> nameservers;
  final Map<String, String> hostMapping;
}
```

因此 Dart 用户不能配置：

- `cache_size`
- `negative_ttl_secs`
- `stale_ttl_secs`
- `stale_on_error`

这不是“HTTP DNS 缓存完全不生效”。HTTP 传输层现在会默认创建 `StaleAwareDnsResolver`，所以默认缓存路径是存在的。问题是 Dart 无法完整控制缓存策略。

### WS：没有 DNS 配置入口

Rust `WsClientConfig` 没有 `dns` 字段，Dart `WsClientConfig` 也没有 `dns` 字段。

WS 连接路径直接走：

```rust
tokio_tungstenite::connect_async_with_config(request, Some(ws_config), true)
```

这意味着 WebSocket 连接没有接入 catcher 的 DNS 缓存、`host_mapping`、自定义 nameservers，也没有 DNS 失败时使用旧缓存兜底的能力。

## 影响

1. Flutter/Dart HTTP 用户不能像 NAPI HTTP 一样完整调 DNS 缓存策略。
2. Flutter/Dart WS 用户完全无法配置 DNS 缓存。
3. WS 在 DNS 抖动、系统 DNS 慢、DNS 暂时失败时，不能复用 catcher HTTP 已有的 DNS 保护能力。
4. Dart 文档和 API 容易让人误以为原生层能力已经全部透出。

## 根因

### 1. Dart 封装层落后于 Rust HTTP 的 DNS 修复

Rust HTTP 已经从早期的空壳 DNS 逻辑升级为 `StaleAwareDnsResolver`，但 Dart `DnsConfig` 仍停留在较早字段集。

### 2. WS 传输层本身没有 DNS 解析器设计

HTTP 可以通过 reqwest 的 `dns_resolver()` 接入 DNS 解析器。WS 使用 tokio-tungstenite，不能只加一个 `dns` 字段就完成修复，需要单独设计连接流程。

## 建议修复

### HTTP Dart

补齐 Dart `DnsConfig` 字段：

```dart
class DnsConfig {
  final int cacheSize;
  final int cacheTtlSecs;
  final int negativeTtlSecs;
  final int staleTtlSecs;
  final bool staleOnError;
  final List<String> nameservers;
  final Map<String, String> hostMapping;
}
```

`toJson()` 传给 Rust 时使用 snake_case：

- `cache_size`
- `cache_ttl_secs`
- `negative_ttl_secs`
- `stale_ttl_secs`
- `stale_on_error`
- `nameservers`
- `host_mapping`

### WS Rust + Dart

需要先补 Rust WS 能力，再补 Dart 封装层：

1. Rust `WsClientConfig` 增加 `dns: Option<DnsConfig>`
2. WS 连接时使用带 DNS 配置的连接流程
3. Dart `WsClientConfig` 增加 `DnsConfig? dns`
4. Dart `toJson()` 透传 `dns`

注意：WS 不能只在 Dart 侧加字段。Rust WS 当前没有接收和使用 DNS 配置的地方。

## 测试建议

- Dart `DnsConfig.toJson()` 覆盖所有字段
- Dart HTTP 创建 client 时传入完整 DNS 配置，确认 Rust 能解析成功
- Dart HTTP `hostMapping` 集成测试：假域名映射到 `127.0.0.1`
- WS DNS 修复后补充：
  - WS `hostMapping` 测试
  - WS 自定义 nameserver 测试
  - WS 旧缓存兜底测试

## 验收标准

- [ ] Dart HTTP `DnsConfig` 字段与 Rust HTTP `DnsConfig` 对齐
- [ ] Dart HTTP 可以配置 `stale_on_error`
- [ ] Rust WS 支持 DNS 配置
- [ ] Dart WS 可以透传 DNS 配置
- [ ] HTTP / WS 都有对应 Dart 测试
