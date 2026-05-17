# 17 — Dart FFI 配置类型对齐

> 状态：RFC（待实施）
> 范围：`packages/catcher_core/`（Dart FFI 绑定）
> 关联：`13-dart-ffi.md`（Dart FFI 绑定层架构）、`16-napi-ts-wrapper.md`（napi TS wrapper 对标参考）

---

## 1. 现状分析

### 1.1 架构概况

Dart FFI 包已有的架构是**正确的**：

```
Dart 用户
  └─ CatcherHttpClient(HttpClientConfig(...))    ← 类型化 Dart class
       └─ config.toJson()                         ← snake_case JSON 序列化
            └─ dart:ffi → catcher_http_client_create(json)
                 └─ Rust: serde_json::from_str(&json)
```

与 napi 包不同，Dart 包不存在"手写 .d.ts 漂移"问题——配置类型本身就是 Dart 源码，编译器强制类型和实现一致。

### 1.2 当前配置类型清单

| Dart class | 对应 Rust struct | 文件 |
|-----------|-----------------|------|
| `HttpClientConfig` | `catcher_http::HttpClientConfig` | `http_client.dart:1044` |
| `RetryConfig` | `catcher_core::RetryConfig` | `http_client.dart:870` |
| `CircuitBreakerConfig` | `catcher_core::CircuitBreakerConfig` | `http_client.dart:894` |
| `PoolConfig` | `catcher_http::PoolConfig` | `http_client.dart:916` |
| `TlsConfig` | `catcher_http::TlsConfig` | `http_client.dart:938` |
| `DnsConfig` | `catcher_http::DnsConfig` | `http_client.dart:978` |
| `ProxyConfig` | `catcher_http::ProxyConfig` | `http_client.dart:1010` |
| `RedirectConfig` | `catcher_http::RedirectConfig` | `http_client.dart:1029` |
| `WsClientConfig` | `catcher_ws::WsClientConfig` | `ws_client.dart:341` |
| `WsReconnectConfig` | `catcher_ws::ReconnectConfig` | `ws_client.dart:299` |
| `WsHeartbeatConfig` | `catcher_ws::HeartbeatConfig` | `ws_client.dart:320` |
| `SseClientConfig` | `catcher_core::SseClientConfig` | `sse_client.dart:103` |
| `SseReconnectConfig` | `catcher_core::SseReconnectConfig` | `sse_client.dart:79` |

所有配置 class 已实现 `toJson()` 将 camelCase Dart 字段序列化为 snake_case JSON。

---

## 2. 问题清单

### P1: 默认值与 Rust 不一致

| Dart 字段 | Dart 默认 | Rust 默认 | Dart 文件:行 | Rust 文件:行 |
|-----------|----------|----------|-------------|-------------|
| `PoolConfig.idleTimeoutSecs` | **90** | **30** | `http_client.dart:924` | `http.rs:84` |
| `PoolConfig.keepAliveIntervalSecs` | **60** | **20** | `http_client.dart:926` | `http.rs:89` |
| `WsClientConfig.deflateThresholdBytes` | **256** | **1024** | `ws_client.dart:363` | `ws.rs:171` |

**影响**：用户省略这些字段时，行为与 Rust 默认不一致，可能导致连接复用率降低或压缩行为异常。

---

### P2: `SseReconnectConfig` 字段结构与 Rust 完全不同

Dart（`sse_client.dart:79-100`）：
```dart
class SseReconnectConfig {
  final bool enabled;          // ❌ Rust 中不存在
  final int maxAttempts;
  final String backoffKind;    // ❌ Rust 是 f64 backoff_multiplier，不是 kind 字符串
  final int initialBackoffMs;  // ❌ 字段名与 Rust 不同
  final int maxBackoffMs;      // ❌ 字段名与 Rust 不同
}
```

Rust `SseReconnectConfig`（`catcher-core/src/types/sse.rs:53-61`）：
```rust
pub struct SseReconnectConfig {
    pub max_retries: u32,           // 默认 10
    pub initial_delay_ms: u64,      // 默认 1000
    pub max_delay_ms: u64,          // 默认 30000
    pub backoff_multiplier: f64,    // 默认 2.0
}
```

差异汇总：

| 维度 | Dart | Rust |
|------|------|------|
| `enabled` 字段 | ✅ 有 | ❌ 无 — 通过 `Option<SseReconnectConfig>` 表示启用 |
| 退避控制 | `backoffKind` (String: `"fixed"`/`"exponential"`/`"decorrelated"`) | `backoff_multiplier` (f64) |
| 重试次数 | `maxAttempts` | `max_retries` |
| 延迟字段 | `initialBackoffMs` / `maxBackoffMs` | `initial_delay_ms` / `max_delay_ms` |

`toJson()` 当前输出的 key 全部与 Rust 反序列化字段名不匹配：
```dart
// 当前输出
{"enabled": true, "max_attempts": 5, "backoff_kind": "exponential", ...}
// Rust 期望
{"max_retries": 10, "initial_delay_ms": 1000, "max_delay_ms": 30000, "backoff_multiplier": 2.0}
```

**影响**：严重 — 用户配置 `SseReconnectConfig` 后，Rust 侧因 key 不匹配全部使用默认值，用户配置完全无效。

---

### P3: 缺少字段

| Dart class | 缺少字段 | Rust 来源 | Rust 文件:行 |
|-----------|---------|----------|-------------|
| `HttpClientConfig` | `hostnameOverride` | `hostname_override: Option<String>` | `http.rs:293` |
| `SseClientConfig` | `circuitBreaker` | `circuit_breaker: Option<CircuitBreakerConfig>` | `sse.rs:30` |
| `TlsConfig` | `clientIdentityPfx` | `client_identity_pfx: Option<Vec<u8>>` | `http.rs:139` |
| `TlsConfig` | `clientIdentityPassword` | `client_identity_password: Option<String>` | `http.rs:142` |
| `TlsConfig` | `maxTlsVersion` | `max_tls_version: Option<TlsVersion>` | `http.rs:151` |

---

### P4: SSE 事件类型名与 Rust 不一致

Dart SSE 事件使用 `open` / `data` / `error` / `close`（`sse_client.dart:287-302` 和 `http_client.dart` 的 `sseStream()` 方法第 332-346 行均有相同逻辑）：

```dart
// sse_client.dart:287-302
case 'open':   _eventController.add(SseOpenEvent());
case 'data':   _eventController.add(SseDataEvent.fromJson(parsed));
case 'error':  _eventController.add(SseErrorEvent.fromJson(parsed));
case 'close':  _eventController.add(SseCloseEvent());
```

Rust SSE 序列化使用 `Line` / `Error` / `End`（`catcher-napi-http/src/sse.rs:178-187`）：

```rust
// 实际 JSON 输出
{"type": "Line", "data": "..."}
{"type": "Error", "message": "..."}
{"type": "End"}
```

napi TS 类型定义也使用 `Line` / `Error` / `End`。Dart 的 `open` / `data` / `close` 与 Rust 实际输出完全不匹配。

**影响**：严重 — SSE 事件解析失败。当前代码可能从未与 Rust SSE 实际输出对测试过。

> **注**：需确认 Rust `catcher-ffi` 的 SSE 回调实际发出的 JSON type 值。如果 catcher-ffi 的 SSE 实现使用了不同于 napi-http `sse.rs` 的序列化逻辑（例如 `open`/`data`/`close`），则以 catcher-ffi 实际输出为准。本问题标记为待验证。

---

### P5: `HttpClientConfig.baseUrl` 为 required，但 Rust 为可选

Dart（`http_client.dart:1061`）：
```dart
const HttpClientConfig({required this.baseUrl, ...})
```

Rust（`http.rs:251-253`）：
```rust
#[serde(default)]
pub base_url: String,  // 默认为空字符串
```

**影响**：低 — Dart 用户必须显式传 `baseUrl`，无法省略。Rust 侧允许省略（默认空串，适合只发绝对 URL 的场景）。建议去掉 `required`，添加默认值 `''`，与 Rust 行为一致。

---

### P6: 分隔线风格违规

`ws_client.dart` 和 `sse_client.dart` 使用 `// ═══` 分隔线：

```dart
// ═══════════════════════════════════════════════════════════════
// WebSocket event types
// ═══════════════════════════════════════════════════════════════
```

RUST_STYLE_GUIDE.md 规定：区段分隔线统一使用 `// ──`，禁止 `// ═══`。

> 注：Dart 文件虽非 Rust，但跨语言保持注释风格一致有助于可维护性。

---

## 3. 设计目标

1. **字段精确对齐**：所有配置 class 的字段、类型、默认值与 Rust struct 1:1 对应
2. **SSE 事件类型统一**：Dart SSE 事件 type 值与 Rust 实际输出一致
3. **SseReconnectConfig 重写**：字段结构完全对齐 Rust `SseReconnectConfig`
4. **默认值修正**：所有默认值与 Rust `Default` impl 或 `#[serde(default = "...")]` 一致
5. **向后兼容**：`toJson()` 输出 JSON key 不变（snake_case），与 Rust serde 字段名保持一致

---

## 4. 方案设计

### 4.1 默认值修正

```dart
// http_client.dart — PoolConfig
const PoolConfig({
  this.maxIdlePerHost = 10,
  this.idleTimeoutSecs = 30,      // 修正: 90 → 30
  this.keepAlive = true,
  this.keepAliveIntervalSecs = 20, // 修正: 60 → 20
});

// ws_client.dart — WsClientConfig
const WsClientConfig({
  ...
  this.deflateThresholdBytes = 1024, // 修正: 256 → 1024
  ...
});
```

---

### 4.2 `SseReconnectConfig` 重写

完全对齐 Rust `SseReconnectConfig`：

```dart
/// SSE 自动重连配置 — 对应 Rust SseReconnectConfig
class SseReconnectConfig {
  final int maxRetries;
  final int initialDelayMs;
  final int maxDelayMs;
  final double backoffMultiplier;

  const SseReconnectConfig({
    this.maxRetries = 10,
    this.initialDelayMs = 1000,
    this.maxDelayMs = 30000,
    this.backoffMultiplier = 2.0,
  });

  Map<String, dynamic> toJson() => {
        'max_retries': maxRetries,
        'initial_delay_ms': initialDelayMs,
        'max_delay_ms': maxDelayMs,
        'backoff_multiplier': backoffMultiplier,
      };
}
```

变更：
- 移除 `enabled`（通过 `SseClientConfig.reconnect != null` 表达启用）
- 移除 `backoffKind`（String）→ 新增 `backoffMultiplier`（double）
- `maxAttempts` → `maxRetries`
- `initialBackoffMs` → `initialDelayMs`
- `maxBackoffMs` → `maxDelayMs`

⚠️ **Breaking change**：API 签名变化，需更新所有调用方并在 CHANGELOG 中标注。

---

### 4.3 补齐缺失字段

**`HttpClientConfig`**：
```dart
class HttpClientConfig {
  final String baseUrl;
  ...
  final String? hostnameOverride;  // 新增

  const HttpClientConfig({
    this.baseUrl = '',       // 与 Rust 一致：默认空串，去掉 required
    ...
    this.hostnameOverride,
  });

  Map<String, dynamic> toJson() => {
    ...
    if (hostnameOverride != null) 'hostname_override': hostnameOverride,
  };
}
```

**`SseClientConfig`**：
```dart
class SseClientConfig {
  ...
  final CircuitBreakerConfig? circuitBreaker;  // 新增

  const SseClientConfig({
    ...
    this.circuitBreaker,
  });

  Map<String, dynamic> toJson() => {
    ...
    if (circuitBreaker != null) 'circuit_breaker': circuitBreaker!.toJson(),
  };
}
```

**`TlsConfig`**：
```dart
class TlsConfig {
  ...
  final Uint8List? clientIdentityPfx;       // 新增
  final String? clientIdentityPassword;     // 新增
  final String? maxTlsVersion;              // 新增

  const TlsConfig({
    ...
    this.clientIdentityPfx,
    this.clientIdentityPassword,
    this.maxTlsVersion,
  });

  Map<String, dynamic> toJson() => {
    ...
    if (clientIdentityPfx != null) 'client_identity_pfx': base64.encode(clientIdentityPfx!),
    if (clientIdentityPassword != null) 'client_identity_password': clientIdentityPassword,
    if (maxTlsVersion != null) 'max_tls_version': maxTlsVersion,
  };
}
```

---

### 4.4 SSE 事件类型名验证与修正

> **待验证**：先确认 `catcher-ffi`（Rust cdylib）的 SSE 回调实际发出的 JSON type 值。如果为 `Line`/`Error`/`End`，则修正如下：

```dart
// sse_client.dart + http_client.dart sseStream() — 修正 switch-case
switch (type) {
  case 'Line':     // 修正: 'open'/'data' → 'Line'
    _eventController.add(SseDataEvent.fromJson(parsed));
    break;
  case 'Error':    // 保持不变
    _eventController.add(SseErrorEvent.fromJson(parsed));
    break;
  case 'End':      // 修正: 'close' → 'End'
    _eventController.add(SseCloseEvent());
    break;
}
```

同时更名事件类以匹配 napi TS 和 Rust：
- `SseOpenEvent` → 移除（`Line` 类型即包含数据，无独立 open 事件）
- `SseCloseEvent` → `SseEndEvent`（需同步更新 `catcher_core.dart` 的 export 声明）

> 如果 Rust catcher-ffi 实际使用 `open`/`data`/`close`，则改为修正 Rust 侧（napi sse.rs）以统一到同一套命名。

---

### 4.5 分隔线风格统一

```dart
// ws_client.dart / sse_client.dart / http_client.dart
// ── WebSocket event types ──    （替换 // ═══）
```

---

## 5. 变更清单

### Phase 1 — 默认值 + 字段补齐（低风险，API 兼容）

| 文件 | 变更 |
|------|------|
| `packages/catcher_core/lib/src/http_client.dart` | `PoolConfig`: `idleTimeoutSecs` 默认 90→30, `keepAliveIntervalSecs` 默认 60→20 |
| `packages/catcher_core/lib/src/http_client.dart` | `HttpClientConfig`: `baseUrl` 去掉 `required`，默认 `''`；新增 `hostnameOverride` 字段 + `toJson()` |
| `packages/catcher_core/lib/src/http_client.dart` | `TlsConfig`: 新增 `clientIdentityPfx`, `clientIdentityPassword`, `maxTlsVersion` |
| `packages/catcher_core/lib/src/sse_client.dart` | `SseClientConfig`: 新增 `circuitBreaker` 字段 + `toJson()` |
| `packages/catcher_core/lib/src/ws_client.dart` | `WsClientConfig.deflateThresholdBytes` 默认 256→1024 |

### Phase 2 — SseReconnectConfig 重写（API breaking）

| 文件 | 变更 |
|------|------|
| `packages/catcher_core/lib/src/sse_client.dart` | `SseReconnectConfig`: 字段全部重写（见 4.2），移除 `enabled`/`backoffKind`/`initialBackoffMs`/`maxBackoffMs`，新增 `maxRetries`/`initialDelayMs`/`maxDelayMs`/`backoffMultiplier` |
| `packages/catcher_core/lib/catcher_core.dart` | 更新 export 声明 |
| `packages/catcher_core/CHANGELOG.md` | 标注 breaking change |

### Phase 3 — SSE 事件类型统一（需先验证 Rust 侧实际输出）

| 文件 | 变更 |
|------|------|
| `packages/catcher_core/lib/src/sse_client.dart` | 修正 switch-case type 匹配（`open`→`Line`, `close`→`End`）；移除 `SseOpenEvent`，`SseCloseEvent`→`SseEndEvent` |
| `packages/catcher_core/lib/src/http_client.dart` | `sseStream()` 方法中同样的 switch-case 修正（第 332-346 行） |
| `packages/catcher_core/lib/catcher_core.dart` | 更新 export：移除 `SseOpenEvent`，`SseCloseEvent`→`SseEndEvent` |
| 或 `packages/catcher-napi-http/src/sse.rs` | 如果 Rust catcher-ffi 使用不同命名，则统一 napi 侧 |

### Phase 4 — 注释风格

| 文件 | 变更 |
|------|------|
| `packages/catcher_core/lib/src/ws_client.dart` | `// ═══` → `// ──` |
| `packages/catcher_core/lib/src/sse_client.dart` | `// ═══` → `// ──` |

---

## 6. 与 napi TS 方案的关系

| 维度 | napi TS（16-napi-ts-wrapper） | Dart FFI（本文档） |
|------|------------------------------|-------------------|
| 根本问题 | 手写 .d.ts 与 JS 实现漂移 | 配置 class 字段与 Rust struct 不一致 |
| 架构变更 | 大 — JS→TS + tsup 构建 | 小 — 纯字段/默认值修正 |
| Breaking | 类型层面（回调 string→object） | SseReconnectConfig 字段 API breaking |
| 实施顺序 | Phase 1 已完成设计 | 可在 napi Phase 1 实施后跟进 |

---

## 7. 验证计划

1. **Rust SSE 输出确认**：在 `catcher-ffi` 的 SSE 回调中打印实际 JSON type 值，确认是 `Line`/`End` 还是 `open`/`close`
2. **单元测试**：为每个修正后的 config class 编写 `toJson()` 输出与 Rust 反序列化兼容性测试
3. **集成测试**：通过 `ffi_roundtrip_test.dart` 验证修正后的配置可成功创建 Rust client
