# 09 — FFI 接口契约

> 对应源文件：`crates/catcher-ffi/` (cdylib umbrella)，以及 napi-rs / dart:ffi 绑定包
> 能力缺口详见：[../issues/ffi-uniffi-capability-gaps.md](../issues/ffi-uniffi-capability-gaps.md)
> 技术设计方案：[../plan/10-ffi-capability-gap-design.md](../plan/10-ffi-capability-gap-design.md)

---

## FFI 设计原则

| 原则 | 说明 |
|------|------|
| **C ABI 唯一事实来源** | 所有导出用 `extern "C"` + `#[repr(C)]` |
| **字符串** | C 字符串指针 + 长度，避免 `CString` 跨边界 |
| **二进制** | `*const u8` + `len`，零拷贝 `Buffer` / `Uint8List` |
| **异步回调** | 函数指针 + `user_data` 观察者模式 |
| **错误** | 统一 `FfiResult` 结构体 |
| **JSON 配置透传** | 复杂配置通过 JSON 字符串跨边界，避免 `#[repr(C)]` 结构体膨胀 |

---

## FFI 基础类型 (`src/ffi/types_ffi.rs`)

```rust
use std::ffi::{c_char, c_void};

/// FFI 安全的结果类型
#[repr(C)]
pub struct FfiResult {
    pub error_code: i32,         // 0 = 成功
    pub error_message: *mut c_char,
    pub data: *mut c_void,
    pub data_len: usize,
}

#[repr(C)]
pub struct FfiString {
    pub data: *mut c_char,
    pub len: usize,
}

#[repr(C)]
pub struct FfiBytes {
    pub data: *const u8,
    pub len: usize,
    pub free_fn: Option<extern "C" fn(*mut c_void)>,
    pub free_ctx: *mut c_void,
}

pub type EventCallback = extern "C" fn(
    event_type: *const c_char,
    event_data: *const u8,
    event_data_len: usize,
    user_data: *mut c_void,
);
```

---

## HTTP C ABI (`catcher-http/src/ffi/http_ffi.rs`)

### 客户端生命周期

```rust
#[no_mangle]
pub extern "C" fn catcher_http_client_create(
    config_json: *const c_char,  // HttpClientConfig JSON（含 tls/dns/proxy/redirect/auth/default_headers）
) -> *mut c_void

#[no_mangle]
pub extern "C" fn catcher_http_client_destroy(handle: *mut c_void)
```

### 请求执行（支持 per-request headers + timeout）

```rust
/// 通用执行入口 — 支持 per-request headers + timeout
#[no_mangle]
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,            // "GET" | "POST" | "PUT" | "DELETE" | "PATCH"
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,  // JSON {"k":"v",...} (NULL = no extra headers)
    timeout_ms: u32,              // 0 = use transport default
    callback: EventCallback,
    user_data: *mut c_void,
)
```

headers 优先级：per-request headers > config.default_headers

### 便捷方法（基于 execute 的快捷封装）

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_http_get(
    handle: *mut c_void, url: FfiString,
    headers_json: *const c_char,   // per-request headers (JSON or NULL)
    timeout_ms: u32,               // per-request timeout (0 = default)
    callback: EventCallback, user_data: *mut c_void,
) { ... }

#[no_mangle]
pub unsafe extern "C" fn catcher_http_post(
    handle: *mut c_void, url: FfiString,
    body: *const u8, body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,   // per-request headers (JSON or NULL)
    timeout_ms: u32,               // per-request timeout (0 = default)
    callback: EventCallback, user_data: *mut c_void,
) { ... }
```

> **注意**：PUT / DELETE / PATCH 没有独立便捷方法，请使用 `catcher_http_execute` 传入对应 HTTP method。

### 运行时控制

```rust
/// 取消该客户端所有飞行请求（页面退出场景）
#[no_mangle]
pub extern "C" fn catcher_http_client_cancel_all(
    handle: *mut c_void,
)

/// 查询熔断器状态
#[no_mangle]
pub extern "C" fn catcher_http_circuit_breaker_state(
    handle: *mut c_void,
) -> *mut c_char  // JSON: {"state":"open","failure_count":5,"success_count":0}

/// 查询运行时指标
#[no_mangle]
pub extern "C" fn catcher_http_metrics(
    handle: *mut c_void,
) -> *mut c_char  // JSON: MetricsSnapshot

/// 配置自适应超时（基于 P90 RTT）
#[no_mangle]
pub extern "C" fn catcher_http_adaptive_timeout_config(
    handle: *mut c_void,
    enable: i32,              // 0 = disable, 1 = enable
    initial_timeout_ms: u32,
    max_timeout_ms: u32,
    decay_factor: f64,        // e.g. 0.9
)
```

### HttpClientConfig JSON 格式

```json
{
  "base_url": "https://api.example.com",
  "connect_timeout_ms": 5000,
  "response_timeout_ms": 10000,
  "keep_alive": true,
  "pool": {"max_idle": 10, "idle_timeout_ms": 90000},

  "tls": {
    "reject_unauthorized": true,
    "ca_cert_pem": "-----BEGIN CERTIFICATE-----\n...",
    "client_cert_pem": "...",
    "client_key_pem": "...",
    "tls_sni_override": null,
    "min_tls_version": "tls1_2"
  },

  "dns": {
    "dns_cache_ttl": 300,
    "nameservers": ["8.8.8.8"],
    "host_mapping": {"api.internal": "10.0.0.5"}
  },

  "proxy": {
    "url": "http://proxy:8080",
    "auth": {"username": "user", "password": "pass"},
    "no_proxy": ["localhost"]
  },

  "redirect": {
    "follow": true,
    "max_redirects": 5
  },

  "auth": {"username": "admin", "password": "secret"},
  "bearer_token": "eyJhbGciOi...",
  "default_headers": {"X-Custom": "value"},

  "retry": {
    "max_attempts": 3,
    "backoff_kind": "exponential",
    "initial_backoff_ms": 1000,
    "max_backoff_ms": 30000,
    "retryable_errors": ["timeout", "connection", "server_error"]
  },

  "circuit_breaker": {
    "failure_threshold": 5,
    "success_threshold": 3,
    "reset_timeout_ms": 30000,
    "half_open_max_requests": 1
  }
}
```

---

## SSE C ABI (`catcher-http/src/ffi/sse_ffi.rs`) — ✅ 已实现

> Rust `catcher-http` 已有完整实现（`SseClient` + `SseStream`，13 个单元测试全绿）。
> 6 个 C ABI 符号已全部导出。

### SseStream（一次性流 — 适用于 POST SSE）

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_stream(
    handle: *mut c_void,        // HttpTransport handle
    method: FfiString,          // "GET" | "POST"
    url: FfiString,
    body: *const u8,
    body_len: usize,
    headers_json: *const c_char,
    callback: EventCallback,    // 每收到一行 SSE 推送一次
    user_data: *mut c_void,
)
```

### SseClient（持久连接 + 自动重连）

```rust
#[no_mangle]
pub unsafe extern "C" fn catcher_sse_connect(
    config_json: *const c_char,    // SseClientConfig JSON
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void

#[no_mangle]
pub unsafe extern "C" fn catcher_sse_ready_state(
    sse_handle: *mut c_void,
) -> i32  // 0=Connecting, 1=Open, 2=Closed

#[no_mangle]
pub unsafe extern "C" fn catcher_sse_last_event_id(
    sse_handle: *mut c_void,
) -> *mut c_char  // 调用方通过 catcher_free_data 释放

#[no_mangle]
pub unsafe extern "C" fn catcher_sse_close(sse_handle: *mut c_void)

#[no_mangle]
pub unsafe extern "C" fn catcher_sse_destroy(sse_handle: *mut c_void)
```

### SseEvent 回调协议

```json
// callback event_type="sse_event", event_data (UTF-8 JSON):
{
  "type": "open",        // "open" | "data" | "error" | "close"
  "data": "...",         // SSE line 的 data 部分
  "event": "message",    // SSE line 的 event 字段（可选）
  "id": "42",            // SSE line 的 id 字段（可选）
  "retry_ms": 3000       // SSE line 的 retry 字段（可选）
}
```

---

## WebSocket C ABI (`catcher-ws/src/ffi/ws_ffi.rs`)

```rust
#[no_mangle]
pub extern "C" fn catcher_ws_create(
    config_json: *const c_char,
    event_callback: EventCallback,
    user_data: *mut c_void,
) -> *mut c_void

#[no_mangle]
pub extern "C" fn catcher_ws_send_text(
    handle: *mut c_void,
    message: FfiString,
) -> FfiResult

#[no_mangle]
pub extern "C" fn catcher_ws_send_binary(
    handle: *mut c_void,
    data: *const u8,
    len: usize,
) -> FfiResult

#[no_mangle]
pub extern "C" fn catcher_ws_close(
    handle: *mut c_void,
    code: u16,
    reason: FfiString,
)

#[no_mangle]
pub extern "C" fn catcher_ws_destroy(handle: *mut c_void)
```

### WsClientConfig JSON 格式（完整）

```json
{
  "urls": ["wss://ws1.example.com", "wss://ws2.example.com"],
  "headers": {"Authorization": "Bearer xxx"},
  "protocols": ["chat", "superchat"],
  "deflate_threshold_bytes": 256,
  "race_count": 3,
  "reconnect": {
    "enabled": true,
    "max_attempts": 5,
    "backoff_kind": "exponential",
    "initial_backoff_ms": 1000,
    "max_backoff_ms": 30000
  },
  "heartbeat": {
    "enabled": true,
    "interval_ms": 30000,
    "timeout_ms": 10000
  }
}
```

---

## Codec C ABI (`catcher-ffi/src/lib.rs`)

```rust
#[no_mangle]
pub extern "C" fn catcher_pack(
    json_input: *const c_char,
) -> FfiResult

#[no_mangle]
pub extern "C" fn catcher_unpack(
    data: *const u8, len: usize,
) -> FfiResult

/// 释放 Rust 侧分配的内存（通用）
#[no_mangle]
pub extern "C" fn catcher_free_data(ptr: *mut c_void, len: usize)
```

---

## Network Quality C ABI (`catcher-http/src/ffi/quality_ffi.rs`)

```rust
/// 一次性 RTT 评估
#[no_mangle]
pub extern "C" fn catcher_evaluate_quality(
    host: FfiString,
    callback: EventCallback,
    user_data: *mut c_void,
)

/// 查询历史滑动窗口数据 ✅
#[no_mangle]
pub extern "C" fn catcher_quality_history() -> *mut c_char  // JSON: {"rtt_samples": {...}, "current_level": "good"}
```

---

## C ABI 符号总览

> ✅ = 已实现

| # | 符号 | 模块 | 状态 |
|---|------|------|:----:|
| 1 | `catcher_http_client_create` | HTTP | ✅ |
| 2 | `catcher_http_client_destroy` | HTTP | ✅ |
| 3 | `catcher_http_execute` | HTTP | ✅ (headers_json + timeout_ms) |
| 4 | `catcher_http_get` | HTTP | ✅ (headers_json + timeout_ms) |
| 5 | `catcher_http_post` | HTTP | ✅ (headers_json + timeout_ms) |
| 6 | `catcher_http_client_cancel_all` | HTTP | ✅ |
| 7 | `catcher_http_circuit_breaker_state` | HTTP | ✅ |
| 8 | `catcher_http_metrics` | HTTP | ✅ |
| 9 | `catcher_http_adaptive_timeout_config` | HTTP | ✅ |
| 10 | `catcher_sse_connect` | SSE | ✅ |
| 11 | `catcher_sse_stream` | SSE | ✅ |
| 12 | `catcher_sse_ready_state` | SSE | ✅ |
| 13 | `catcher_sse_last_event_id` | SSE | ✅ |
| 14 | `catcher_sse_close` | SSE | ✅ |
| 15 | `catcher_sse_destroy` | SSE | ✅ |
| 16 | `catcher_ws_create` | WS | ✅ |
| 17 | `catcher_ws_send_text` | WS | ✅ |
| 18 | `catcher_ws_send_binary` | WS | ✅ |
| 19 | `catcher_ws_close` | WS | ✅ |
| 20 | `catcher_ws_destroy` | WS | ✅ |
| 21 | `catcher_pack` | Codec | ✅ |
| 22 | `catcher_unpack` | Codec | ✅ |
| 23 | `catcher_free_data` | Codec | ✅ |
| 24 | `catcher_evaluate_quality` | Quality | ✅ |
| 25 | `catcher_quality_history` | Quality | ✅ |

**全部 25 个 C ABI 符号已实现 ✅**

> **PUT / DELETE / PATCH** 没有独立便捷方法，请使用 `catcher_http_execute` 传入对应 HTTP method。

---

## napi-rs 绑定层（Node.js）

```
catcher-napi-http/        # npm 包 (已发布)
├── package.json
├── Cargo.toml            # [lib] crate-type = ["cdylib"]
├── build.rs
└── src/
    └── lib.rs

catcher-napi-ws/          # npm 包 (已发布)
├── package.json
├── Cargo.toml            # [lib] crate-type = ["cdylib"]
├── build.rs
└── src/
    └── lib.rs
```

```rust
use napi::*;
use napi_derive::napi;

#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Buffer,
    pub elapsed_ms: u32,
}

/// Napi 独有：每请求级 options（C ABI 缺失的对应能力）
#[napi(object)]
pub struct RequestOptions {
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u32>,
    pub content_type: Option<String>,
}

#[napi]
pub struct JsHttpClient { inner: Arc<catcher_http::HttpTransport> }

#[napi]
impl JsHttpClient {
    #[napi(constructor)]
    pub fn new(config: String) -> napi::Result<Self> { todo!() }

    #[napi]
    pub async fn get(&self, url: String, options: Option<RequestOptions>) -> napi::Result<JsHttpResponse> { todo!() }

    #[napi]
    pub async fn post(
        &self, url: String, body: Buffer,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> { todo!() }

    /// Napi 独有：熔断器状态查询
    #[napi]
    pub fn circuit_breaker_state(&self) -> napi::Result<String> { todo!() }
}

#[napi] pub fn pack(obj: napi::JsUnknown, env: Env) -> napi::Result<Buffer> { todo!() }
#[napi] pub fn unpack(buffer: Buffer) -> napi::Result<napi::JsUnknown> { todo!() }
```

### Napi 独有能力（与 C ABI 对比）

| 能力 | Napi | C ABI |
|------|:----:|:-----:|
| Per-request headers + timeout (`RequestOptions`) | ✅ | ✅ (via `headers_json` / `timeout_ms`) |
| Circuit breaker state query | ✅ | ✅ |
| PUT / DELETE / PATCH 便捷方法 | ✅ | ❌ (use `catcher_http_execute`) |
| WS binary send | ❌ text only | ✅ |
| msgpack codec | ❌ | ✅ |
| Network quality evaluate | ❌ | ✅ |

---

## dart:ffi 绑定层（Dart / Flutter）

> 决策：dart:ffi ✅, flutter_rust_bridge ❌。详细设计见 [`13-dart-ffi.md`](./13-dart-ffi.md)

```
catcher_core/               # pub.dev 包 (已发布 v0.1.0)
├── pubspec.yaml
├── rust/
│   ├── Cargo.toml            # depends on catcher-ffi
│   └── src/
│       └── lib.rs            # re-export catcher-ffi cdylib symbols
└── lib/
    ├── catcher_core.dart
    └── src/
        ├── ffi_bindings.dart # dart:ffi C 函数签名绑定
        ├── native_loader.dart
        ├── http_client.dart
        ├── ws_client.dart
        ├── sse_client.dart    # ✅ SSE persistent + one-shot stream
        ├── codec.dart
        └── quality.dart
```

Dart 侧通过 `dart:ffi` 直接调用 C ABI。Rust 侧由 `catcher-ffi` cdylib umbrella crate 统一导出全部 C ABI 符号。

---

## UniFFI 绑定层（Swift + Kotlin）— ✅ 已全面补齐

> 详细设计见 [`../plan/10-ffi-capability-gap-design.md`](../plan/10-ffi-capability-gap-design.md) §FFI-11

```
packages/catcher-uniffi/    # Swift + Kotlin bindings (WIP)
├── Cargo.toml              # [lib] crate-type = ["cdylib"]
├── build.rs
├── src/
│   └── lib.rs              # 370行, UniFFI proc-macro 模式
└── generated/
    ├── swift/
    └── kotlin/
```

### 当前导出状态

```rust
// ✅ 已实现
#[derive(uniffi::Object)]
pub struct HttpClient { ... }    // new(config_json), get, post, put, delete, patch
pub struct HttpResponseDto { ... } // status, headers, body, elapsed_ms

#[derive(uniffi::Object)]
pub struct WsClient { ... }       // new(config_json, observer), send_text/binary, close
pub enum WsEventDto { ... }       // 6 种事件（Connected/Disconnected/Reconnecting/Message/Error/HeartbeatRtt）
pub trait WsEventObserver { ... } // callback_interface

pub enum CatcherError { ... }     // flat_error: Network + Config (简化版)
```

### 缺口总览

| 缺口 | 影响 |
|------|------|
| **SSE 全模块缺失** | Swift/Kotlin 无法接收 OpenAI/Anthropic SSE 流 |
| **codec 全模块缺失** | 无 msgpack pack/unpack |
| **quality 全模块缺失** | 无网络质量评估 |
| **HTTP per-request headers/timeout** | 所有方法硬编码 `headers: Default::default()` + `timeout_ms: None` |
| **HttpResponseDto 缺 headers** | 无法获取服务端返回的响应头 |
| **HttpClient 缺 CB state + metrics** | 无法查询运行时熔断器状态和指标 |
| **WsClient 只连第一个 URL** | 多端点竞速不可用 |

### 关键约束

UniFFI 0.28 **不支持 async methods**。所有异步调用通过 `block_on_aux_thread()` 桥接——spawn 一个独立 std thread 运行自己的 tokio runtime，避免 `block_on()` re-entrance panic。
