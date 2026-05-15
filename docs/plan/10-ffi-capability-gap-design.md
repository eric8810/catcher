# FFI 能力缺口 — 技术设计方案

> 对应 Issues: [ffi-uniffi-capability-gaps.md](../issues/ffi-uniffi-capability-gaps.md) FFI-01 ~ FFI-12
> 调研基础：全量 Rust 源码 (51 .rs 文件) 逐项对照 C ABI (25 符号) + UniFFI + Napi + Dart FFI
> 日期：2026-06
> 
> **实现状态（更新）**：FFI-01~FFI-07, FFI-09~FFI-10, FFI-12 已完成。C ABI 从 16 符号扩展到 25 符号。
> 仅 FFI-08（PriorityQueue wiring）和 FFI-11（UniFFI SSE/codec/quality）待完成。

---

## 问题总览

C ABI 25 符号全部被 Dart 绑定 ✅。FFI-01~FFI-07、FFI-09~FFI-10、FFI-12 已完成。待完成：FFI-08（PriorityQueue）、FFI-11（UniFFI SSE/codec/quality）。

按优先级分为三层：

| 优先级 | 数量 | 影响 |
|:------:|:----:|------|
| 🔴 P0 | 3 | 阻塞 Flutter 实际使用 |
| 🟡 P1 | 4 | 影响生产可用性 |
| 🟢 P2 | 5 | 增强能力 |

---

## 能力现状矩阵（摘要）

```
能力覆盖度（更新后）： C ABI / Dart FFI > Napi > UniFFI

C ABI / Dart FFI 独有 (vs Napi):
  ✅ WS binary send
  ✅ msgpack codec (pack/unpack)
  ✅ Network quality evaluate + history
  ✅ SSE persistent + one-shot stream

Napi 独有 (vs C ABI):
  ✅ PUT / DELETE / PATCH 便捷方法

UniFFI:
  ✅ WS binary send
  ✅ WsEventObserver callback_interface
  ❌ SSE / codec / quality — 仍未导出
```

---

## 🔴 P0 — 阻塞 Flutter 实际使用

### FFI-01: `catcher_http_execute` 缺少 per-request headers 参数

#### 现状

```rust
// C ABI 当前签名 (http_ffi.rs:151-160)
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,       // ✅
    url: FfiString,          // ✅
    body: *const u8,         // ✅
    body_len: usize,         // ✅
    content_type: FfiString, // ✅
    callback: EventCallback, // ✅
    user_data: *mut c_void,  // ✅
)                            // ❌ 无 headers 参数
```

#### 对比 napi（已支持）

```rust
// napi (catcher-napi-http/src/lib.rs:36-41)
pub struct RequestOptions {
    pub headers: Option<HashMap<String, String>>,  // ✅ 有
    pub timeout_ms: Option<u32>,                   // ✅ 有
    pub content_type: Option<String>,              // ✅ 有
}
```

#### 与 Rust 内核能力对齐

```rust
// transport/http_client.rs:161-167
// apply default_headers from config
for (k, v) in &self.config.default_headers { req = req.header(k, v); }
// per-request headers override
for (k, v) in &request.headers { req = req.header(k, v); }
```

Rust `HttpTransport::execute` 已完整支持 per-request headers + config.default_headers 两阶段注入，headers 优先级为 per-request > config.default_headers。C ABI 只需透传即可。

#### 修复方案

**C ABI 签名扩展**：增加 `headers_json: *const c_char`（null-terminated JSON `{"k":"v",...}`），同时增加 `timeout_ms: u32`（0 表示使用 transport 默认值）：

```rust
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,  // 新增：JSON 格式 per-request headers
    timeout_ms: u32,              // 新增：0 = 使用默认值
    callback: EventCallback,
    user_data: *mut c_void,
)
```

**内部实现要点**：

1. `headers_json` 为 NULL 时跳过，不覆盖 default_headers
2. 解析 JSON → `HashMap<String, String>`，作为 per-request headers 传入
3. `timeout_ms > 0` 时覆盖 transport 默认 timeout
4. headers 优先级：per-request headers > config.default_headers

**Dart 侧封装**：

```dart
// lib/src/http_client.dart
Future<HttpResponse> get(String url, {Map<String, String>? headers}) async {
  final headersJson = headers != null ? jsonEncode(headers) : null;
  // 传入 headersJson 到 FFI 调用
}
```

**验收标准**：
- [ ] Dart 侧 `client.get('/path', headers: {'Authorization': 'Bearer xxx'})` 可传自定义头
- [ ] headers 优先级: per-request > config.default_headers
- [ ] `headers_json` 为 NULL 时不报错，使用 default_headers
- [ ] UniFFI `HttpClient::get` 同样支持 headers

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/catcher-ffi/src/http.rs` | `catcher_http_execute` 签名增加 headers_json + timeout_ms 参数 |
| `crates/catcher-ffi/src/http.rs` | 新增 `catcher_http_execute_with_options` 或扩展原函数 |
| `packages/catcher_core/lib/src/ffi_bindings.dart` | 更新 C 函数签名绑定 |
| `packages/catcher_core/lib/src/http_client.dart` | Dart 层透传 headers |
| `crates/catcher-uniffi/src/lib.rs` | UniFFI HTTP binding 增加 headers 支持 |

**工作量**: S

---

### FFI-02: SSE C ABI 完全缺失

#### 现状

Rust `catcher-http` 已有完整 SSE 实现：

- `SseClient` — 带 auto-reconnect, `Last-Event-ID`, cancel 通道, `SseReadyState`
- `SseStream` — 一次性流消费，实现 `tokio_stream::Stream` trait
- `route_line()` — 标准 SSE line 路由 (data/event/id/retry/comment)
- **单元测试**: SseClient 6 个全绿, SseStream 7 个全绿

**但 `#[no_mangle] pub extern "C"` 符号为零。**

#### SSE 协议要点

```
POST /v1/chat/completions (SSE 流)
  → 不维护长连接（请求完成即断开）
  → POST body 发 JSON payload
  → 响应为 text/event-stream

GET /v1/events (持久 SSE 订阅)
  → 长连接，需要自动重连
  → 使用 Last-Event-ID 断点续传
  → 支持 cancel
```

catcher-rs SSE 的两种模式天然覆盖上述场景：
- `SseStream` → POST SSE（一次性流）
- `SseClient` → 持久 SSE + 自动重连

#### 修复方案

新增 6 个 C ABI 符号（覆盖 SseClient + SseStream 两种模式）：

**1. SseStream（一次性流，适用于 POST SSE）**

```rust
// POST SSE 流 — 一次性消费，拿到所有行后完成
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_stream(
    handle: *mut c_void,        // HttpTransport handle
    method: FfiString,          // GET 或 POST
    url: FfiString,
    body: *const u8,
    body_len: usize,
    headers_json: *const c_char, // 请求头（如 Content-Type, Authorization）
    callback: EventCallback,    // 每收到一行 SSE 推送一次
    user_data: *mut c_void,
)
```

callback 的 `event_type` 取值：`"sse_line"`（一行数据）, `"sse_done"`（流结束）, `"sse_error"`（流错误）。

**2. SseClient（持久连接 + 自动重连）**

```rust
// 创建 SseClient 持久连接
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_connect(
    config_json: *const c_char,    // SseClientConfig JSON
    event_callback: EventCallback, // 每行 SSE 数据推送
    user_data: *mut c_void,
) -> *mut c_void                   // 返回 SseClient handle

// 获取连接状态
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_ready_state(
    sse_handle: *mut c_void,
) -> i32  // 0=Connecting, 1=Open, 2=Closed

// 获取 Last-Event-ID
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_last_event_id(
    sse_handle: *mut c_void,
) -> *mut c_char  // 调用方通过 catcher_free_data 释放

// 关闭连接
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_close(
    sse_handle: *mut c_void,
)

// 销毁句柄
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_destroy(
    sse_handle: *mut c_void,
)
```

#### SseClientConfig 设计

```rust
// Rust 侧已有此类型，需确保可 JSON 反序列化
pub struct SseClientConfig {
    pub url: String,
    pub method: String,             // 默认 "GET"
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub reconnect: SseReconnectConfig,
}

pub struct SseReconnectConfig {
    pub enabled: bool,
    pub max_attempts: u32,          // 默认 5
    pub backoff_ms: u64,            // 初始退避，默认 1000ms
    pub max_backoff_ms: u64,        // 最大退避，默认 30000ms
    pub backoff_kind: BackoffKind,  // fixed / exponential / decorrelated
}
```

#### 时序图

```
Dart                    Rust (C ABI)              Rust (tokio)
 │                         │                         │
 │── sse_connect(config)──▶│                         │
 │                         │── SseClient::connect()─▶│
 │                         │                         │── reqwest get + stream
 │   ◀── callback("open")──│◀── ready_state=Open ────│
 │                         │                         │── read next SSE line
 │   ◀── callback("data")──│◀── line parsed ─────────│
 │   ◀── callback("data")──│◀── line parsed ─────────│
 │                         │                         │── connection lost
 │                         │                         │── auto reconnect w/ Last-Event-ID
 │   ◀── callback("open")──│◀── reconnected ─────────│
 │                         │                         │
 │── sse_close(handle)────▶│── cancel_tx.send() ────▶│
 │   ◀── callback("close")─│◀── closed ──────────────│
```

#### SseEvent 协议

```json
// callback 收到的 event_data (UTF-8 JSON)
{
  "type": "data",       // "open" | "data" | "error" | "close"
  "data": "...",        // SSE line 的 data 部分
  "event": "message",   // SSE line 的 event 字段
  "id": "42",           // SSE line 的 id 字段
  "retry_ms": 3000      // SSE line 的 retry 字段
}
```

#### 验收标准

- [ ] Dart 侧可用 `CatcherSseClient` 接收 OpenAI 兼容 SSE 流
- [ ] 支持 POST SSE（如 Anthropic streaming API）
- [ ] 网络断开后自动重连并携带 `Last-Event-ID`
- [ ] `catcher_sse_ready_state()` 可查询当前连接状态
- [ ] UniFFI 同步暴露 SSE（通过 `block_on_aux_thread`）

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/catcher-ffi/src/sse.rs` | **新增**：6 个 SSE C ABI 符号导出 |
| `crates/catcher-ffi/src/lib.rs` | 注册 SSE 符号 + mod sse |
| `crates/catcher-http/src/sse/client.rs` | `SseClient` 暴露 `last_event_id()` |
| `packages/catcher_core/lib/src/ffi_bindings.dart` | 新增 SSE C 函数签名 |
| `packages/catcher_core/lib/src/sse_client.dart` | **新增**：Dart SSE 客户端封装 |
| `crates/catcher-uniffi/src/lib.rs` | UniFFI 暴露 SSE (block_on_aux_thread) |

**工作量**: M

---

### FFI-03: Dart `WsClientConfig` 缺少 headers / protocols

#### 现状

```rust
// Rust WsClientConfig 已支持 (catcher-ws/src/types/ws.rs)
pub struct WsClientConfig {
    pub urls: Vec<String>,
    pub headers: HashMap<String, String>,   // ✅ Rust 有
    pub protocols: Option<Vec<String>>,     // ✅ Rust 有
    pub deflate_threshold_bytes: usize,     // ✅ Rust 有
    pub race_count: u32,                    // ✅ Rust 有
    // ...
}
```

```dart
// Dart 侧漏绑 (catcher_core/lib/src/models/ws_config.dart)
class WsClientConfig {
  final List<String> urls;
  // ❌ 缺 headers
  // ❌ 缺 protocols
  // ❌ 缺 deflateThresholdBytes
  // ❌ 缺 raceCount
}
```

#### 修复方案

**Dart 类型补充**：

```dart
class WsClientConfig {
  final List<String> urls;
  final Map<String, String>? headers;          // 新增
  final List<String>? protocols;               // 新增
  final int deflateThresholdBytes;             // 新增，默认 256
  final int raceCount;                         // 新增，默认 1
  // ...existing fields...

  String toJson() {
    return jsonEncode({
      'urls': urls,
      'headers': headers,                      // 新增
      'protocols': protocols,                  // 新增
      'deflate_threshold_bytes': deflateThresholdBytes,  // 新增
      'race_count': raceCount,                 // 新增
      // ...existing fields...
    });
  }
}
```

**C ABI 侧**：`catcher_ws_create(config_json, ...)` 已接受 JSON 配置，Rust 侧 `WsClientConfig` 已包含这些字段。Dart 侧补全 `toJson()` 即可，**C ABI 无需改动**。

#### 验收标准

- [ ] `WsClientConfig(urls: [...], headers: {'Authorization': 'Bearer xxx'})` 生效
- [ ] `protocols: ['chat', 'superchat']` 在 WS 握手中传递
- [ ] `raceCount: 3` 触发多端点竞速连接
- [ ] `deflateThresholdBytes` 控制 perMessageDeflate 压缩阈值

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `packages/catcher_core/lib/src/models/ws_config.dart` | 新增 headers/protocols/deflateThresholdBytes/raceCount |
| `packages/catcher_core/test/ws_config_test.dart` | 新增字段序列化测试 |

**工作量**: S

---

## 🟡 P1 — 影响生产可用性

### FFI-04: 取消/Abort 机制

#### 现状

- Dio 有 `CancelToken`, TS wrapper 有 `AbortSignal`
- Rust `HttpTransport::execute` **没有 cancel 机制**
- SSE `SseClient` 内部有 `cancel_tx: mpsc::UnboundedSender` channel，但不对外
- WS `catcher_ws_destroy` 已存在，但 connect 阶段无法中途取消

#### 设计目标

```
取消粒度：

1. 单请求级取消: HTTP 请求中途取消单个飞行请求
2. 客户端级取消: 取消该客户端上所有飞行请求（页面退出场景）
3. SSE 取消: 等同关闭连接
4. WS 取消: 取消正在 connect 阶段的 WS（已有 destroy 负责连接后）
```

#### 修复方案

**方案 A（推荐）：基于 tokio CancellationToken 的每请求取消**

```rust
// 新增：C ABI cancel handle
#[no_mangle]
pub unsafe extern "C" fn catcher_http_cancel_request(
    handle: *mut c_void,
    request_id: u64,              // 由创建请求时返回
) -> i32  // 0=成功, -1=未找到
```

**方案 B（更简单）：客户端级 cancel token**

```rust
// 在 HttpTransport handle 内部维护一个 CancellationToken
// catcher_http_client_create 时初始化
// 每次 execute 时创建 child_token

// 新增 C ABI
#[no_mangle]
pub unsafe extern "C" fn catcher_http_client_cancel_all(
    handle: *mut c_void,
)
```

**推荐方案 B**，原因：
1. 实现简单，利用 `tokio_util::CancellationToken` 的 `child_token()` 机制
2. 覆盖 P0 场景（页面退出时取消所有飞行请求）
3. 后续可扩展为方案 A（在 Dart 侧维护 request_id 映射）

#### Rust 层实现

```rust
// transport/http_client.rs
use tokio_util::sync::CancellationToken;

pub struct HttpTransport {
    client: Client,
    config: HttpClientConfig,
    cancel_token: CancellationToken,  // 新增
    // ...
}

impl HttpTransport {
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
        let cancel = self.cancel_token.child_token();
        tokio::select! {
            result = self.execute_inner(request) => result,
            _ = cancel.cancelled() => Err(CatcherError::Cancelled),
        }
    }

    pub fn cancel_all(&self) {
        self.cancel_token.cancel();
        // 替换新的 token 以允许后续请求
        // self.cancel_token = CancellationToken::new();
    }
}
```

#### 影响范围

`catcher_ws_create` 的 connect 异步阶段同样可以用 CancellationToken 包装。WS 的已有 `catcher_ws_destroy` 在连接成功后可以关闭，但连接中不可取消 —— 需要用 child_token 包装 connect future。

#### 验收标准

- [ ] `catcher_http_client_cancel_all(handle)` → 所有飞行请求返回 `Cancelled` 错误
- [ ] 取消后新请求不受影响
- [ ] WS connect 阶段取消：`catcher_ws_destroy` 在 connect 阶段也可生效

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/catcher-http/src/transport/http_client.rs` | HttpTransport 增加 CancellationToken + cancel_all() |
| `crates/catcher-ffi/src/http.rs` | 新增 `catcher_http_client_cancel_all` |
| `crates/catcher-ffi/src/ws.rs` | WS connect 阶段加入 cancellation |
| `packages/catcher_core/lib/src/ffi_bindings.dart` | 新增 cancel C 函数签名 |
| `packages/catcher_core/lib/src/http_client.dart` | Dart 层暴露 `cancelAll()` |

**工作量**: M

---

### FFI-05: 熔断器状态查询无 C ABI

#### 现状

- Rust `HttpTransport` 已有 `pub fn circuit_breaker_state(&self) -> CbState`
- Napi 已暴露：`JsHttpClient::circuitBreakerState()`
- **C ABI 无此接口** → Dart 不可用

#### 修复方案

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_http_circuit_breaker_state(
    handle: *mut c_void,
) -> *mut c_char  // JSON: {"state":"open","failure_count":5,"success_count":0,...}
```

Dart 侧：

```dart
class CatcherHttpClient {
  CircuitBreakerState get circuitBreakerState {
    final json = _catcherHttpCircuitBreakerState(_handle).toDartString();
    return CircuitBreakerState.fromJson(jsonDecode(json));
  }
}
```

#### 验收标准

- [ ] Dart 侧可查询当前熔断器状态（Closed / HalfOpen / Open）
- [ ] 状态包含 failure_count / success_count
- [ ] 返回 JSON 字符串，由 Dart 侧解析

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/catcher-ffi/src/http.rs` | 新增 `catcher_http_circuit_breaker_state` |
| `packages/catcher_core/lib/src/ffi_bindings.dart` | 新增 C 函数签名 |
| `packages/catcher_core/lib/src/http_client.dart` | Dart 层封装 |

**工作量**: S

---

### FFI-06: `HttpClientConfig` 不传 `TlsConfig/DnsConfig/ProxyConfig`

#### 现状

Rust 类型定义完整但 C ABI 的 `catcher_http_client_create(config_json)` 传入的 JSON 未包含这些配置项：

| 配置 | Rust 类型 | C ABI JSON | 状态 |
|------|----------|:----------:|:----:|
| `TlsConfig` | 含 ca/cert/key/SNI/版本控制 | ❌ 不传 | 只传 `reject_unauthorized` |
| `DnsConfig` | 含 nameservers/host_mapping | ❌ 不传 | 只传 `dns_cache_ttl` |
| `ProxyConfig` | 含 url/auth/noProxy | ❌ 不传 | 完全缺失 |
| `RedirectConfig` | 含 follow/max_redirects | ❌ 不传 | 完全缺失 |
| `Auth` / `bearer_token` | 含在 `HttpClientConfig` | ❌ 不传 | 完全缺失 |
| `default_headers` | 含在 `HttpClientConfig` | ❌ 不传 | 完全缺失 |

#### 修复方案

**Rust 侧**：确保 `HttpClientConfig` 的 `serde::Deserialize` 覆盖全部字段，C ABI 的 `catcher_http_client_create(config_json)` 直接透传。

当前 C ABI 已经接受 JSON 字符串，问题是 JSON 解析时未使用完整字段。修复步骤：

1. 核对 `HttpClientConfig` 的 `#[derive(Deserialize)]` 是否包含所有字段
2. `catcher_http_client_create` 的 `config_json` 解析逻辑需验证所有字段都被消费
3. Dart 侧的 `HttpClientConfig.toJson()` 需补全缺失字段

**关键：这些不是新功能，是透传已存在的 Rust 能力到 FFI 边界。**

#### TLS 配置透传

```json
// Dart HttpClientConfig.toJson() 需补全
{
  "tls": {
    "reject_unauthorized": false,
    "ca_cert_pem": "-----BEGIN CERTIFICATE-----\n...",
    "client_cert_pem": "-----BEGIN CERTIFICATE-----\n...",
    "client_key_pem": "-----BEGIN RSA PRIVATE KEY-----\n...",
    "tls_sni_override": "custom.sni.name",
    "min_tls_version": "tls1_2"
  },
  "dns": {
    "dns_cache_ttl": 300,
    "nameservers": ["8.8.8.8", "1.1.1.1"],
    "host_mapping": {"api.internal": "10.0.0.5"}
  },
  "proxy": {
    "url": "socks5://proxy:1080",
    "auth": {"username": "user", "password": "pass"},
    "no_proxy": ["localhost", "127.0.0.1"]
  },
  "redirect": {
    "follow": true,
    "max_redirects": 5
  },
  "auth": {
    "username": "admin",
    "password": "secret"
  },
  "bearer_token": "eyJhbGciOi...",
  "default_headers": {
    "X-Custom-Header": "value"
  }
}
```

#### 验收标准

- [ ] TLS full config（CA cert、mTLS cert+key、SNI override）透传到 reqwest
- [ ] DNS custom nameservers + host_mapping 生效
- [ ] HTTP/SOCKS5 proxy 配置生效
- [ ] Redirect 策略生效
- [ ] default_headers 自动注入所有请求

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/catcher-http/src/types/http.rs` | 核对 Deserialize 完整性 |
| `crates/catcher-ffi/src/http.rs` | 核对 config_json 解析使用全部字段 |
| `packages/catcher_core/lib/src/models/http_config.dart` | toJson() 补全所有缺失字段 |
| `crates/catcher-http/src/transport/http_client.rs` | 确认 proxy/dns/redirect 构建逻辑正确 |

**工作量**: M

---

### FFI-07: `MetricsCollector` 无 C ABI

#### 现状

Rust `catcher-http` 有完整的 `MetricsCollector`（atomic 指标收集），但：
- 无 C ABI 导出
- Dart 无法获取任何运行时指标

#### MetricsCollector 内已收集的指标

```rust
pub struct MetricsSnapshot {
    pub total_requests: u64,
    pub total_success: u64,
    pub total_errors: u64,
    pub total_retries: u64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p90_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub active_connections: u64,
    pub circuit_breaker_trips: u64,
}
```

#### 修复方案

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_http_metrics(
    handle: *mut c_void,
) -> *mut c_char  // JSON: MetricsSnapshot
```

Dart 侧：

```dart
class CatcherHttpClient {
  MetricsSnapshot get metrics {
    final json = _catcherHttpMetrics(_handle).toDartString();
    return MetricsSnapshot.fromJson(jsonDecode(json));
  }
}
```

#### 验收标准

- [ ] 可获取 total_requests / total_success / total_errors
- [ ] 可获取 avg / p50 / p90 / p99 延迟
- [ ] 可获取 active_connections 和 circuit_breaker_trips

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/catcher-ffi/src/http.rs` | 新增 `catcher_http_metrics` |
| `packages/catcher_core/lib/src/ffi_bindings.dart` | 新增 C 函数签名 |
| `packages/catcher_core/lib/src/http_client.dart` | Dart 层 `metrics` getter |

**工作量**: S

---

## 🟢 P2 — 增强

### FFI-08: `PriorityRequestQueue` 未接入 transport 且无 FFI

#### 现状

Rust 端 `scheduler/priority_queue.rs` 完整实现但停在 scheduler 模块，`HttpTransport` 未使用。

#### 建议

暂时跳过 C ABI 暴露，原因：
1. `HttpTransport` 未接入优先级队列，暴露 FFI 也无意义
2. 需要先在 `HttpTransport` 内部接入，再考虑 FFI 导出
3. Dart 侧可以用 `p-queue` 临时替代

**建议在 Phase 5 处理，先完成 internal wiring 再暴露。**

---

### FFI-09: `NetworkQualityEvaluator` 滑动窗口不可用

#### 现状

- `catcher_evaluate_quality(host, callback, user_data)` ✅ 已有
- 但只做一次性 RTT 评估，历史滑动窗口数据不可查询

#### 修复方案

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_quality_history(
    host: FfiString,
) -> *mut c_char  // JSON: {"rtt_samples": [...], "current_level": "good", "trend": "improving"}
```

需要在 `NetworkQualityEvaluator` 内部维持 host → sliding window 的持久化存储（当前是一次性评估后丢弃）。

**工作量**: M

---

### FFI-10: `AdaptiveTimeout` 无 FFI

#### 现状

`resilience/timeout.rs` 实现了 P90 RTT 自适应超时，但仅在 Rust 内部使用，FFI 不可控。

#### 修复方案

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_http_adaptive_timeout_config(
    handle: *mut c_void,
    enabled: bool,
    min_timeout_ms: u32,
    max_timeout_ms: u32,
    scaling_factor: f64,   // 默认 3.0，timeout = P90_RTT * scaling_factor
)
```

**工作量**: S

---

### FFI-11: UniFFI 能力缺口全面补齐

#### 现状

`packages/catcher-uniffi/src/lib.rs` (370行) 当前导出了 HTTP 客户端（5个 HTTP method）+ WS 客户端（6种事件 DTO + callback_interface observer），但存在两个层面的缺口：

**A. 整个功能模块缺失（3个模块）**：
- SSE 流（SseClient + SseStream）— 零导出
- msgpack codec（pack / unpack）— 零导出
- 网络质量评估（evaluate_quality）— 零导出

**B. 已导出模块内能力缺失（HTTP + WS）**：

| 缺失能力 | 位置 | 当前代码 |
|---------|------|---------|
| Per-request headers | `get/post/put/delete/patch` | `headers: Default::default()` 硬编码空 |
| Per-request timeout | 同上 | `timeout_ms: None` 硬编码 |
| Response headers | `HttpResponseDto` | 只有 status/body/elapsed_ms，无 headers |
| 熔断器状态查询 | `HttpClient` | 无 `circuit_breaker_state()` |
| 运行时指标 | `HttpClient` | 无 `metrics()` |
| 请求取消 | `HttpClient` | 无 `cancel_all()`（依赖 FFI-04） |
| WS headers/protocols/race | `WsClient::new` | config JSON 不传这4个字段 |
| 多端点竞速 | `WsClient::new` | `urls.first()` 只连第一个 URL |

#### 修复方案

##### A. 新增模块导出

**1. SSE 模块**

UniFFI 0.28 不支持 async methods，SSE 的异步流消费通过 `block_on_aux_thread` 桥接。设计分为一次性流（SseStream）和持久连接（SseClient）：

```rust
// ── SSE 事件 DTO ──
#[derive(Debug, Clone, uniffi::Enum)]
pub enum SseEventDto {
    Open,
    Data {
        data: String,
        event: Option<String>,
        id: Option<String>,
        retry_ms: Option<u32>,
    },
    Error {
        message: String,
    },
    Close,
}

// ── callback_interface: SSE 事件观察者 ──
#[uniffi::export(callback_interface)]
pub trait SseEventObserver: Send + Sync {
    fn on_event(&self, event: SseEventDto);
}

// ── SseClient (持久连接 + 自动重连) ──
#[derive(uniffi::Object)]
pub struct SseClient {
    inner: Arc<catcher_http::sse::client::SseClient>,
    _event_task: tokio::task::JoinHandle<()>,
}

#[uniffi::export]
impl SseClient {
    #[uniffi::constructor]
    pub fn connect(
        config_json: String,
        observer: Box<dyn SseEventObserver>,
    ) -> Result<Self, CatcherError> {
        // 1. 解析 SseClientConfig
        // 2. block_on_aux_thread 调用 SseClient::connect()
        // 3. spawn event forwarding task 将 SSE 行转为 SseEventDto 回调
    }

    #[uniffi::method]
    pub fn ready_state(&self) -> u32  // 0=Connecting, 1=Open, 2=Closed

    #[uniffi::method]
    pub fn last_event_id(&self) -> Option<String>

    #[uniffi::method]
    pub fn close(&self)
}
```

**SseClientConfig JSON 格式**：
```json
{
  "url": "https://api.example.com/v1/events",
  "method": "GET",
  "headers": {"Authorization": "Bearer xxx"},
  "body": null,
  "reconnect": {
    "enabled": true,
    "max_attempts": 5,
    "backoff_ms": 1000,
    "max_backoff_ms": 30000,
    "backoff_kind": "exponential"
  }
}
```

**SseStream（一次性流）**：通过 `HttpClient` 扩展方法实现，在 `HttpClient` 上新增一个 `sse_stream` 方法：

```rust
#[uniffi::export]
impl HttpClient {
    // ...existing methods...

    /// POST SSE 流（适用于 OpenAI/Anthropic streaming API）
    /// 返回收集到的所有 SSE 事件列表
    #[uniffi::method]
    pub fn sse_stream(
        &self,
        method: String,
        url: String,
        body: Option<Vec<u8>>,
        headers_json: Option<String>,
    ) -> Result<Vec<SseEventDto>, CatcherError> {
        // block_on_aux_thread → SseStream → collect all events
    }
}
```

**2. Codec 模块**

```rust
/// msgpack 编码：JSON string → msgpack bytes
#[uniffi::export]
pub fn catcher_pack(json_input: String) -> Result<Vec<u8>, CatcherError> {
    catcher_ffi::codec::pack(&json_input)
        .map_err(|e| CatcherError::Config(e.to_string()))
}

/// msgpack 解码：msgpack bytes → JSON string
#[uniffi::export]
pub fn catcher_unpack(data: Vec<u8>) -> Result<String, CatcherError> {
    catcher_ffi::codec::unpack(&data)
        .map_err(|e| CatcherError::Config(e.to_string()))
}
```

**3. Network Quality 模块**

```rust
/// 一次性 RTT 评估
#[uniffi::export]
pub fn evaluate_quality(host: String) -> Result<String, CatcherError> {
    // block_on_aux_thread → NetworkQualityEvaluator::evaluate()
    // 返回 JSON: {"level":"good","rtt_ms":45,"jitter_ms":3,...}
}
```

##### B. 已导出模块能力补齐

**1. HTTP 方法增加 per-request headers + timeout**

核心改动：HttpClient 的 get/post/put/delete/patch 全部增加 `headers_json` 和 `timeout_ms` 参数。

```rust
#[uniffi::export]
impl HttpClient {
    /// GET request（扩展后签名）
    #[uniffi::method]
    pub fn get(
        &self,
        url: String,
        headers_json: Option<String>,     // 📐: JSON {"k":"v",...}
        timeout_ms: Option<u32>,          // 📐: None = 使用默认值
    ) -> Result<HttpResponseDto, CatcherError> {
        let headers = parse_headers_json(headers_json)?;
        let inner = self.inner.clone();
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::GET,
                url,
                headers,                     // ← 不再 Default::default()
                body: None,
                content_type: None,
                timeout_ms,                  // ← 不再 None
            }).await
        });
        // ...
    }

    // post/put/delete/patch 同样扩展 headers_json + timeout_ms 参数
}
```

**2. `HttpResponseDto` 增加 response headers**

```rust
#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpResponseDto {
    pub status: u16,
    pub headers: Vec<String>,  // 📐: ["content-type: application/json", ...]
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}
```

> UniFFI 不支持 `HashMap<String, String>` 作为 Record 字段（跨语言映射问题），使用 `Vec<String>` 格式 `"key: value"` 替代。Swift/Kotlin 侧各自转换为原生 Map 类型。

**3. 熔断器状态 + 指标查询**

```rust
#[uniffi::export]
impl HttpClient {
    /// 查询熔断器状态
    #[uniffi::method]
    pub fn circuit_breaker_state(&self) -> String {
        // 返回 JSON: {"state":"closed","failure_count":0,"success_count":0}
        serde_json::to_string(&self.inner.circuit_breaker_state()).unwrap_or_default()
    }

    /// 查询运行时指标
    #[uniffi::method]
    pub fn metrics(&self) -> String {
        // 返回 JSON: MetricsSnapshot
        serde_json::to_string(&self.inner.metrics()).unwrap_or_default()
    }
}
```

**4. WS `WsClient::new` 支持完整配置 + 多端点竞速**

当前只连 `urls.first()`，需要改为遍历 urls + race：

```rust
#[uniffi::constructor]
pub fn new(
    config_json: String,
    observer: Box<dyn WsEventObserver>,
) -> Result<Self, CatcherError> {
    let config: WsClientConfig = serde_json::from_str(&config_json)
        .map_err(|e| CatcherError::Config(e.to_string()))?;

    // 📐: 遍历 urls，竞速连接（第一个成功即返回）
    let handle = block_on_aux_thread(async move {
        let mut last_error = None;
        for url in &config.urls {
            match WsTransport::connect(url, &config).await {
                Ok(result) => return Ok(result),
                Err(e) => last_error = Some(e),
            }
        }
        Err(last_error.unwrap_or_else(|| /* fallback error */))
    });
    // ...
}
```

#### 改动文件清单

| 文件 | 改动 |
|------|------|
| `packages/catcher-uniffi/src/lib.rs` | 新增 SseClient/SseEvent/SseEventObserver + HttpClient.sse_stream + pack/unpack + evaluate_quality |
| `packages/catcher-uniffi/src/lib.rs` | HttpClient 方法增加 headers_json/timeout_ms 参数 |
| `packages/catcher-uniffi/src/lib.rs` | HttpResponseDto 增加 headers 字段 |
| `packages/catcher-uniffi/src/lib.rs` | HttpClient 增加 circuit_breaker_state() + metrics() |
| `packages/catcher-uniffi/src/lib.rs` | WsClient::new 增加多端点竞速 + config 字段透传 |
| `packages/catcher-uniffi/Cargo.toml` | 可能需要增加 `catcher-ffi` 依赖（for codec） |
| `crates/catcher-http/src/transport/http_client.rs` | 确保 `headers`/`timeout_ms` 字段在 `HttpRequest` 中可用（依赖 FFI-01） |

#### 验收标准

- [ ] Swift/Kotlin 可用 `SseClient` 接收 OpenAI 兼容 SSE 流
- [ ] `HttpClient.get(url, headers: ["Authorization: Bearer xxx"])` 支持 per-request headers
- [ ] `HttpResponseDto` 包含 response headers
- [ ] `catcher_pack / catcher_unpack` 可在 Swift/Kotlin 中调用
- [ ] `evaluate_quality(host)` 返回 RTT 评估结果
- [ ] `circuit_breaker_state()` 返回当前熔断状态
- [ ] `WsClient` 支持多端点竞速连接
- [ ] SSE 通过 `block_on_aux_thread` 正确桥接，不触发 re-entrance panic

**工作量**: M

---

### FFI-12: Dart `WsClientConfig` 缺少 `race_count` / `deflate_threshold`

同 FFI-03 一起修复。已在 FFI-03 的方案中包含。

---

## 测试补全

> 对应 Issue 文档中的 TEST-01 ~ TEST-10

### Rust FFI 层测试 — ✅ 已部分补齐

25 个 C ABI 函数中，HTTP + SSE + Quality/Codec 共 14 个已有集成测试（`catcher-ffi/tests/`）。WS 5 符号仍无测试。

| 测试文件 | 覆盖 | 状态 |
|---------|------|:----:|
| `catcher-ffi/tests/http_test.rs` | `catcher_http_client_create/destroy`, GET/POST, headers, timeout, cancel, CB state, metrics | ✅ 7 |
| `catcher-ffi/tests/sse_test.rs` | `catcher_sse_stream/connect`, ready_state | ✅ 3 |
| `catcher-ffi/tests/codec_quality_test.rs` | `catcher_pack/unpack`, `catcher_evaluate_quality/quality_history` | ✅ 4 |
| `catcher-ffi/tests/ws_test.rs` | `catcher_ws_create/destroy`, send_text/binary, close, events | ❌ 待做 |

### Dart 集成测试 CI 兼容

当前 Dart 集成测试需要 `CATCHER_FFI_PATH` 环境变量才能运行。需要：

1. 在 CI 中编译 `catcher-ffi` cdylib 后设置 `CATCHER_FFI_PATH` 指向产物路径
2. 或改为 `DynamicLibrary.open()` 在 test 目录的已知路径查找

### Napi / UniFFI 绑定测试

`catcher-napi-http`, `catcher-napi-ws`, `catcher-uniffi` 三个 binding crate 目前零测试。需要为每个 binding 编写 smoke test。

---

## 实施路线图（更新后）

```
Phase 1 — ✅ 打通基本可用 (P0): 已完成
  FFI-01: headers 参数              ✅
  FFI-03: WS headers/protocols      ✅
  FFI-05: CB state query            ✅
  
Phase 2 — ✅ SSE (P0): 已完成
  FFI-02: SSE C ABI (6 符号)        ✅
  
Phase 3 — ✅ 韧性运行时控制 (P1): 已完成
  FFI-04: Abort/cancel              ✅
  FFI-06: TLS/DNS/Proxy 透传        ✅
  FFI-07: Metrics FFI               ✅

Phase 4 — 测试补全: 部分完成
  TEST-01: C ABI FFI tests          ✅ 14 用例
  TEST-02: Dart integ CI 可跑       ⏭ 待做
  TEST-03: Napi binding tests       ⏭ 待做
  TEST-04: UniFFI tests             ⏭ 待做
  TEST-05: WsTransport tests        ⏭ 待做
  TEST-06: multi_endpoint tests     ⏭ 待做

Phase 5 — 增强 (P2): 部分完成
  FFI-08: Priority queue wiring     ⏭ 延后
  FFI-09: Network quality sliding   ✅
  FFI-10: Adaptive timeout          ✅
  FFI-11: UniFFI 全面补齐           ⏭ 待做
  FFI-12: WS race_count/deflate     ✅
```

> **注**：FFI-11 的 UniFFI 缺口覆盖面比 C ABI 更大。C ABI 当前已实现 25 个符号（从原始 16 个扩展）。UniFFI 当前仅 HTTP 5 方法 + WS 基础连接，缺 SSE (全部)、codec (全部)、quality (全部)、per-request headers/timeout、response headers、CB state、metrics、WS 多端点竞速。详见上方 FFI-11 节。

---

## 与 `09-api-gap-technical-design.md` 的关系

| 维度 | `09-api-gap-technical-design.md` | 本文档 |
|------|----------------------------------|--------|
| 问题来源 | `api-gap-features.md` (G2-G12) | `ffi-uniffi-capability-gaps.md` (FFI-01~FFI-12) |
| 关注层面 | TS/Rust API 功能缺失 | Rust FFI 层导出缺失 |
| 影响范围 | TS + Rust 核心逻辑 | C ABI + UniFFI + Dart FFI + Napi |
| 典型问题 | 拦截器、Cookie、代理等业务功能未实现 | Rust 已实现但 FFI 未导出 |
| 关系 | 互补。TS 侧 G4(proxy)/G7(host_mapping)/G8(TLS) 依赖本文档 FFI-06 透传配置 | |

**关键区别**：`09-api-gap-technical-design.md` 处理的是"功能不存在"，本文档处理的是"功能已存在但 FFI 没有透传"。两文档互补，共同覆盖完整的 API 缺口。
