# FFI / UniFFI 能力缺口全面复核

> 日期：2026-05-15（复核更新：2026-06）
> 来源：全量 Rust 源码 (51 .rs 文件) 逐项对照 C ABI (25 符号) + UniFFI + Napi + Dart FFI  
> 结论：**C ABI 25 符号全部被 Dart 绑定 ✅**。SSE / per-request headers / CB state / metrics / adaptive timeout 已补齐。

---

## 对比总表

> 约定：✅ = 可用, ❌ = Rust 实现了但无导出, — = 不适用

### catcher-core — 类型 & 错误

| Rust 能力 | C ABI | UniFFI | Dart FFI | Napi | 说明 |
|-----------|:-----:|:------:|:--------:|:----:|------|
| `CatcherError` + `ErrorCategory` | ❌ | ❌ (flat enum) | ❌ | ❌ | 错误仅通过 JSON `{"error":"..."}` 字符串传递 |
| `FfiResult` / `FfiString` / `EventCallback` | ✅ | — | ✅ | — | 内部 FFI 脚手架 |
| `RetryConfig` | ✅ JSON | ✅ JSON | ✅ | ✅ JSON | |
| `CircuitBreakerConfig` | ✅ JSON | ✅ JSON | ✅ | ✅ JSON | |
| `CbState` (熔断器状态) | ✅ http_ffi | ❌ | ✅ | ✅ `circuitBreakerState()` | ✅ 已补齐 |
| `BackoffKind` | ✅ JSON | ✅ JSON | ✅ | ✅ JSON | |
| `QueueConfig` | ❌ | ❌ | ❌ | ❌ | 只在 Rust 内部 |
| `ConcurrencyMode` | ❌ | ❌ | ❌ | ❌ | 只在 Rust 内部 |
| `Priority` | ❌ | ❌ | ❌ | ❌ | 只在 Rust 内部 |
| `NetworkQualityLevel` | ✅ quality_ffi | ❌ | ✅ | ❌ | |
| `ConnectionType` | ✅ quality_ffi | ❌ | ✅ | ❌ | |
| `RttSnapshot` | ❌ | ❌ | ❌ | ❌ | 只在 Rust 内部 |
| `SseClientConfig` | ✅ sse_ffi | ❌ | ✅ | ❌ | SSE 已补齐 |
| `SseReconnectConfig` | ✅ JSON | ❌ | ✅ | ❌ | |
| `SseMethod` | ✅ sse_ffi | ❌ | ✅ | ❌ | |

### catcher-http — HTTP + 韧性

| Rust 能力 | C ABI | UniFFI | Dart FFI | Napi | 说明 |
|-----------|:-----:|:------:|:--------:|:----:|------|
| `HttpTransport::new(config)` | ✅ | ✅ | ✅ | ✅ | |
| `HttpTransport::destroy()` | ✅ | ✅ | ✅ | ✅ | |
| GET / POST / PUT / DELETE / PATCH | ✅ | ✅ | ✅ | ✅ | |
| **Per-request headers** | ✅ `headers_json` | ❌ | ✅ | ✅ `RequestOptions` | ✅ 已补齐 |
| **Per-request timeout** | ✅ `timeout_ms` | ❌ | ✅ | ✅ `RequestOptions` | ✅ 已补齐 |
| Per-request content-type | ✅ | ✅ | ✅ | ✅ | |
| `PoolConfig` | ✅ JSON | ✅ JSON | ✅ | ✅ JSON | |
| **`TlsConfig` (完整 TLS 配置)** | ✅ JSON | ❌ | ✅ | ❌ | ✅ 通过 HttpClientConfig JSON 透传 |
| **`ProxyConfig` (HTTP/SOCKS5 代理)** | ✅ JSON | ❌ | ✅ | ❌ | ✅ |
| **`DnsConfig` (DNS 服务器/缓存)** | ✅ JSON | ❌ | ✅ | ❌ | ✅ |
| **`RedirectConfig`** | ✅ JSON | ❌ | ✅ | ❌ | ✅ |
| **`Auth` (Basic/Bearer)** | ✅ JSON | ❌ | ✅ | ❌ | ✅ |
| **`default_headers`** | ✅ JSON | ❌ | ✅ | ❌ | ✅ |
| **`hostname_override`** | ✅ JSON | ❌ | ✅ | ❌ | |
| `RetryConfig` (重试中间件) | ✅ | ✅ | ✅ | ✅ | |
| `CircuitBreaker` (熔断器) | ✅ | ✅ | ✅ | ✅ | |
| **`circuit_breaker_state()`** | ✅ | ❌ | ✅ | ✅ | ✅ 已补齐 |
| **`AdaptiveTimeout`** | ✅ | ❌ | ✅ | ❌ | ✅ 已补齐 (`catcher_http_adaptive_timeout_config`) |
| **`PriorityRequestQueue`** | ❌ | ❌ | ❌ | ❌ | 完整实现但未接入 transport |
| **`MetricsCollector` + `MetricsSnapshot`** | ✅ `catcher_http_metrics` | ❌ | ✅ | ❌ | ✅ 已补齐 |
| **`NetworkQualityEvaluator` (滑动窗口)** | ✅ quality_ffi | ❌ | ✅ `evaluateQuality()` | ❌ | ✅ 含历史查询 |

### catcher-http — SSE 流

| Rust 能力 | C ABI | UniFFI | Dart FFI | Napi | 说明 |
|-----------|:-----:|:------:|:--------:|:----:|------|
| **`SseClient` (自动重连 SSE)** | ✅ `catcher_sse_connect` | ❌ | ✅ `CatcherSseClient` | ❌ | ✅ 6 个 C ABI 符号已导出 |
| **`SseStream` (一次性 SSE)** | ✅ `catcher_sse_stream` | ❌ | ✅ `sseStream()` | ❌ | ✅ 已补齐 |
| `route_line` / `RouteAction` | ✅ | ❌ | ✅ | ❌ | SSE 行解析器 |
| `SseReadyState` | ✅ `catcher_sse_ready_state` | ❌ | ✅ | ❌ | ✅ 已补齐 |

### catcher-ws — WebSocket

| Rust 能力 | C ABI | UniFFI | Dart FFI | Napi | 说明 |
|-----------|:-----:|:------:|:--------:|:----:|------|
| `WsTransport::connect` | ✅ | ✅ | ✅ | ✅ | |
| Send text / binary | ✅ | ✅ text + binary | ✅ text + binary | ✅ text only | |
| Close | ✅ | ✅ | ✅ | ✅ | |
| `WsEvent` (6种事件) | ✅ JSON | ✅ DTO enum | ✅ Dart classes | ❌ JSON | Napi 丢失事件 |
| `ReconnectConfig` | ✅ | ✅ | ✅ | ✅ | |
| `HeartbeatConfig` | ✅ | ✅ | ✅ | ✅ | |
| **`protocols` (子协议)** | ✅ JSON | ❌ | ✅ | ❌ | ✅ 已补齐 |
| **`headers` (WS per-request)** | ✅ JSON | ❌ | ✅ | ❌ | ✅ 已补齐 |
| **`deflate_threshold_bytes`** | ✅ JSON | ❌ | ✅ | ❌ | ✅ 已补齐 |
| **`race_count` / multi_endpoint** | ✅ JSON | ❌ | ✅ | ❌ | ✅ 已补齐 |

### catcher-ffi — 编解码 umbrella

| Rust 能力 | C ABI | UniFFI | Dart FFI | Napi | 说明 |
|-----------|:-----:|:------:|:--------:|:----:|------|
| `catcher_pack` (msgpack 编码) | ✅ | ❌ | ✅ | ❌ | |
| `catcher_unpack` (msgpack 解码) | ✅ | ❌ | ✅ | ❌ | |
| `catcher_free_data` | ✅ | ❌ | ✅ | ❌ | |

---

## 关键发现：各绑定层差异已大幅缩小

前期 C ABI 是能力最少的绑定层。经过补齐后，C ABI 和 Dart FFI 已具备绝大部分能力。

### 当前仍为 Napi 独有

| Napi 独有能力 | 值 |
|---------------|-----|
| PUT / DELETE / PATCH 便捷方法 | C ABI 需用 `catcher_http_execute` |

### C ABI / Dart 独有 (vs Napi)

| C ABI / Dart 独有能力 | 说明 |
|----------------------|------|
| WS binary send | Napi 仅 text |
| msgpack codec (pack/unpack) | |
| Network quality evaluate + history | |
| SSE persistent + one-shot stream | |

### UniFFI 仍缺失

SSE、codec、quality 模块在 UniFFI 层仍未导出（FFI-11 待完成）。

---

## 按优先级整理的缺陷清单

### 🔴 P0 — 阻塞 Flutter 实际使用 (已完成 ✅)

| # | Issue | 现状 | 工作量 |
|---|-------|------|:------:|
| FFI-01 | ~~`catcher_http_execute` 缺少 headers 参数~~ | ✅ 已补齐 `headers_json` + `timeout_ms` | S |
| FFI-02 | ~~SSE C ABI 完全缺失~~ | ✅ 6 个 SSE C ABI 符号已导出 | M |
| FFI-03 | ~~Dart `WsClientConfig` 缺少 headers / protocols~~ | ✅ 已补齐 | S |

### 🟡 P1 — 影响生产可用性 (已完成 ✅)

| # | Issue | 现状 | 工作量 |
|---|-------|------|:------:|
| FFI-04 | ~~取消/Abort 机制缺失~~ | ✅ `catcher_http_client_cancel_all` 已实现 | M |
| FFI-05 | ~~熔断器状态查询无 C ABI~~ | ✅ `catcher_http_circuit_breaker_state` 已实现 | S |
| FFI-06 | ~~`HttpClientConfig` 不传 `TlsConfig/DnsConfig/ProxyConfig`~~ | ✅ JSON 透传已验证双向对齐 | M |
| FFI-07 | ~~`MetricsCollector` 无 C ABI~~ | ✅ `catcher_http_metrics` 已实现 | M |

### 🟢 P2 — 增强

| # | Issue | 现状 | 工作量 |
|---|-------|------|:------:|
| FFI-08 | **`PriorityRequestQueue` 未接入 transport 且无 FFI** | Rust 完整实现但停在 scheduler 模块，transport 未使用 | M |
| FFI-09 | ~~`NetworkQualityEvaluator` 滑动窗口不可用~~ | ✅ `catcher_quality_history` 已实现 | M |
| FFI-10 | ~~`AdaptiveTimeout` 无 FFI~~ | ✅ `catcher_http_adaptive_timeout_config` 已实现 | S |
| FFI-11 | **UniFFI 缺少 SSE / codec / quality** | Swift/Kotlin 仍无法用 SSE（待实现） | M |
| FFI-12 | ~~Dart `WsClientConfig` 缺少 `race_count` / `deflate_threshold`~~ | ✅ 已补齐 | S |

---

## ~~详细 Issue: FFI-01~~ — ✅ 已修复

`catcher_http_execute`、`catcher_http_get`、`catcher_http_post` 已增加 `headers_json: *const c_char` 和 `timeout_ms: u32` 参数。

~~### 现状~~
~~```rust~~
~~// C ABI 当前签名 (http_ffi.rs:151-160)~~
~~pub unsafe extern "C" fn catcher_http_execute(~~
~~    handle: *mut c_void,~~
~~    method: FfiString,       // ✅~~
~~    url: FfiString,          // ✅~~
~~    body: *const u8,         // ✅~~
~~    body_len: usize,         // ✅~~
~~    content_type: FfiString, // ✅~~
~~    callback: EventCallback, // ✅~~
~~    user_data: *mut c_void,  // ✅~~
~~)                           // ❌ 无 headers 参数~~
~~```~~

~~### 对比 napi (已支持)~~
~~```rust~~
~~// napi (catcher-napi-http/src/lib.rs:36-41)~~
~~pub struct RequestOptions {~~
~~    pub headers: Option<HashMap<String, String>>,  // ✅ 有~~
~~    pub timeout_ms: Option<u32>,                   // ✅ 有~~
~~    pub content_type: Option<String>,              // ✅ 有~~
~~}~~
~~```~~

~~### 对比 Rust `HttpTransport::execute`~~
~~```rust~~
~~// transport/http_client.rs:161-167~~
~~// apply default_headers from config~~
~~for (k, v) in &self.config.default_headers { req = req.header(k, v); }~~
~~// per-request headers override~~
~~for (k, v) in &request.headers { req = req.header(k, v); }~~
~~```~~

~~### 修复方案~~
C ABI 签名增加 `headers_json: *const c_char` (null-terminated JSON `{\"k\":\"v\",...}`):

```rust
pub unsafe extern "C" fn catcher_http_execute(
    handle: *mut c_void,
    method: FfiString,
    url: FfiString,
    body: *const u8,
    body_len: usize,
    content_type: FfiString,
    headers_json: *const c_char,  // ✅ 已实现
    timeout_ms: u32,              // ✅ 已实现
    callback: EventCallback,
    user_data: *mut c_void,
)
```

### 验收标准
- [x] Dart 侧 `client.get('/path', headers: {'Authorization': 'Bearer xxx'})` 可传自定义头
- [x] headers 优先级: per-request > config.default_headers
- [ ] UniFFI `HttpClient::get` 同样支持 headers（FFI-11 待实现）

---

## ~~详细 Issue: FFI-02~~ — ✅ 已修复 (SSE C ABI 6 符号)

已在 `catcher-http/src/ffi/sse_ffi.rs` 新增 6 个 C ABI 符号:
1. `catcher_sse_connect(config_json, event_callback, user_data) → handle`
2. `catcher_sse_stream(handle, method, url, body, body_len, headers_json, callback, user_data)` — one-shot
3. `catcher_sse_ready_state(handle) → i32`
4. `catcher_sse_last_event_id(handle) → *mut c_char`
5. `catcher_sse_close(handle)`
6. `catcher_sse_destroy(handle)`

Dart 侧已有 `CatcherSseClient` + `sseStream()` 完整绑定。

### 验收标准
- [x] Dart 侧可用 `CatcherSseClient` 接收 OpenAI 兼容 SSE 流
- [x] 支持 POST SSE (如 Anthropic streaming API)
- [x] 网络断开后自动重连并携带 `Last-Event-ID`
- [ ] UniFFI 同步暴露 SSE (通过 `block_on_aux_thread`) — FFI-11 待实现

---

## ~~详细 Issue: FFI-04~~ — ✅ 已修复

`catcher_http_client_cancel_all` 已实现。SSE `catcher_sse_close` 对内部 cancel 通道发送信号。

~~### 现状~~
~~- Dio 有 `CancelToken`, TS wrapper 有 `AbortSignal`~~
~~- Rust `HttpTransport::execute` 没有 cancel 机制~~
~~- SSE `SseClient` 内部有 `cancel_tx: mpsc::UnboundedSender` channel，但不对外~~

~~### 目标~~
~~1. HTTP: `catcher_http_client_abort(handle)` — 取消该客户端所有飞行请求~~
~~2. WS: `catcher_ws_destroy` 已存在，但中途取消 `catcher_ws_create` 的 connect 阶段不可行~~
~~3. SSE: `catcher_sse_close` 对内部 cancel 通道发送信号~~

---

## 跨绑定层差异总结

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

---

## 测试覆盖复核

### 测试覆盖总览

| 层级 | 测试文件数 | 测试数(约) | 状态 |
|------|:---------:|:--------:|:----:|
| Rust #[cfg(test)] inline | 17/34 源文件 | ~105 | 核心逻辑好，FFI 零 |
| TS 单元 (vitest) | ~28 文件 | ~220 | ✅ 很全 |
| TS 集成 (integration) | 4 文件 | ~12 | ✅ |
| TS E2E | 2 文件 | ~38 | ✅ |
| TS Chaos | 2 文件 | ~10 | ✅ |
| TS Benchmark | 1 文件 | ? | ✅ |
| **Dart 单元** | 1 文件 | 20 | 🟡 仅测 config/toJson |
| **Dart 集成** | 1 文件 | 8 | 🔴 需 `CATCHER_FFI_PATH` 环境变量 |

### Rust 层：有测试 vs 无测试

**有 `#[cfg(test)]` 的 17 个文件（总共 ~105 个测试）：**

| 文件 | 测试数 | 质量 |
|------|:-----:|------|
| `sse/router.rs` | 24 | ✅ 重 |
| `ws/codec.rs` | 9 | ✅ |
| `transport/http_client.rs` | 7 | ✅ 含 mock server |
| `sse/stream.rs` | 7 | ✅ 含 mock server |
| `sse/client.rs` | 6 | ✅ 含 mock server |
| `transport/tls.rs` | 6 | ✅ |
| `ws/heartbeat.rs` | 6 | ✅ |
| `ws/reconnect.rs` | 6 | ✅ |
| `resilience/timeout.rs` | 5 | ✅ |
| `resilience/circuit_breaker.rs` | 5 | ✅ 状态机全覆盖 |
| `transport/dns.rs` | 4 | ✅ |
| `resilience/retry.rs` | 4 | ✅ |
| `observability/network_quality.rs` | 4 | ✅ |
| `resilience/backoff.rs` | 3 | ✅ |
| `scheduler/priority_queue.rs` | 3 | ✅ |
| `scheduler/concurrency.rs` | 3 | ✅ |
| `observability/metrics.rs` | 3 | ✅ |

**零测试的 17 个源文件：**

| 文件 | 原因 | 风险 |
|------|------|:---:|
| `catcher-core/src/error.rs` | ❌ `CatcherError` 无测试 | 🟡 |
| `catcher-core/src/ffi_types.rs` | ❌ FfiResult/FfiString/EventCallback 无测试 | 🟡 |
| `catcher-http/src/ffi/http_ffi.rs` | ✅ 7 tests | `catcher-ffi/tests/http_test.rs` |
| `catcher-http/src/ffi/quality_ffi.rs` | ✅ 4 tests (codec+quality) | `catcher-ffi/tests/codec_quality_test.rs` |
| `catcher-ws/src/ffi/ws_ffi.rs` | ❌ **所有 5 个 C 符号无测试** | 🔴 |
| `catcher-ws/src/transport/ws_client.rs` | ❌ **WsTransport 无测试** | 🔴 |
| `catcher-ws/src/ws/multi_endpoint.rs` | ❌ **多端点竞速无测试** | 🔴 |
| `catcher-ws/src/ws/compression.rs` | ❌ **perMessageDeflate 无测试** | 🟡 |
| `catcher-http/src/ffi/sse_ffi.rs` | ✅ 3 tests | `catcher-ffi/tests/sse_test.rs` |
| `catcher-ffi/src/lib.rs` | ✅ 14 tests total | `catcher-ffi/tests/` |
| `catcher-napi-http/src/lib.rs` | ❌ **napi HTTP binding 无测试** | 🔴 |
| `catcher-napi-ws/src/lib.rs` | ❌ **napi WS binding 无测试** | 🔴 |
| `catcher-uniffi/src/lib.rs` | ❌ **UniFFI 360行的绑定无测试** | 🔴 |
| `catcher-core/src/types/*` (4 files) | 纯类型定义，可接受 | 🟢 |
| `catcher-http/src/types/http.rs` | 纯类型，可接受 | 🟢 |
| `catcher-ws/src/types/ws.rs` | 纯类型，可接受 | 🟢 |

**结论：核心 Rust 逻辑层 (resilience/transport/sse/scheduler/observability) 测试质量高。但每个 FFI 绑定层 (C ABI / Napi / UniFFI) 测试为 0。**

### Dart 层：看起来有测试，实际严重不足

**Dart 单元测试 (20 个)** 全部是：
- `toJson()` 产出正确的 JSON key
- 默认值正确
- WsEvent / HttpResponse / NetworkQualityResult 的 `fromJson` 解析

**这些测试不需要 FFI 库即可运行，本质是用 `dart:test` 测 Dart 类的构造函数。**

**Dart 集成测试 (8 个)**：
```dart
final ffiPath = Platform.environment['CATCHER_FFI_PATH'];
if (!hasLib) {
  print('SKIP: Set CATCHER_FFI_PATH to run FFI integration tests');
  test('FFI integration tests skipped (no CATCHER_FFI_PATH)', () {});
  return;
}
```

**需要手动设置环境变量才执行，CI 中大概率永远跳过。**

### 测试缺口矩阵

按严重程度排列：

| # | 缺口 | 严重度 | 说明 |
|---|------|:------:|------|
| ~~TEST-01~~ | ~~FFI C ABI 层零测试~~ | ~~🔴🔴~~ | ✅ 已补齐 14 个 FFI 集成测试 |
| TEST-02 | **Dart 集成测试需手动激活** | 🔴 | `CATCHER_FFI_PATH` 环境变量，CI 不可跑 |
| TEST-03 | **Napi binding 零测试** | 🔴 | napi-http/ws 只有集成测试的 smoke test |
| TEST-04 | **UniFFI 零测试** | 🔴 | 360行代码无任何测试 |
| TEST-05 | **WsTransport 零测试** | 🔴 | WS 核心传输层无测试 |
| TEST-06 | **multi_endpoint 零测试** | 🔴 | 多端点竞速逻辑无测试 |
| TEST-07 | **compression 零测试** | 🟡 | perMessageDeflate 无测试 |
| TEST-08 | **CatcherError / ErrorCategory 零测试** | 🟡 | 错误类型及其可重试分类逻辑无测试 |
| TEST-09 | **Dart 单元仅测序列化** | 🟡 | 从未测试 CatcherHttpClient/CatcherWsClient 的实际创建和调用 |
| TEST-10 | **TS 测试未覆盖 Napi 路径** | 🟡 | TS 测试只测纯 TS 层，不走 native binding |

### 核心问题

```
Rust 核心逻辑 ──── ✅ 105 个测试
    │
    ├─ C ABI  (http_ffi / ws_ffi / quality_ffi / sse_ffi) ─ ✅ 14 测试 (catcher-ffi)
    │     │
    │     └─ Dart FFI  ─ ❌ 集成测试无法跑（需手动设环境变量）
    │
    ├─ Napi   (napi-http / napi-ws) ── ❌ 0 测试
    │
    └─ UniFFI (catcher-uniffi) ──────── ❌ 0 测试
```

---

## 实施路线图（更新后）

```
Phase 1 — ✅ 打通基本可用 (P0): 已完成
  FFI-01: headers 参数          ✅
  FFI-03: WS headers/protocols  ✅
  FFI-05: CB state query        ✅
  
Phase 2 — ✅ SSE (P0): 已完成
  FFI-02: SSE C ABI (6 符号)    ✅
  
Phase 3 — ✅ 韧性运行时控制 (P1): 已完成
  FFI-04: Abort/cancel          ✅
  FFI-06: TLS/DNS/Proxy 透传    ✅
  FFI-07: Metrics FFI           ✅

Phase 4 — 测试补全: 部分完成
  TEST-01: C ABI FFI tests      ✅ 14 用例
  TEST-02: Dart integ CI 可跑   ⏭ 待做
  TEST-03: Napi binding tests   ⏭ 待做
  TEST-04: UniFFI tests         ⏭ 待做
  TEST-05: WsTransport tests    ⏭ 待做
  TEST-06: multi_endpoint tests ⏭ 待做

Phase 5 — 增强 (P2): 部分完成
  FFI-08: Priority queue wiring ⏭ 延后
  FFI-09: Network quality sliding window ✅
  FFI-10: Adaptive timeout      ✅
  FFI-11: UniFFI SSE/codec/quality ⏭ 待做
  FFI-12: WS race_count/deflate ✅
```
