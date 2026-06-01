# 11 — 测试策略

> 测试覆盖复核详见：[../issues/ffi-uniffi-capability-gaps.md](../issues/ffi-uniffi-capability-gaps.md) (测试覆盖复核章节)
> 原生能力缺口测试设计：[../test/native-gap-test-design.md](../test/native-gap-test-design.md)

---

## 测试层级

| 层级 | 工具 | 目标 | 覆盖 |
|------|------|------|------|
| 单元测试 | tokio::test | 每个模块纯逻辑 | 100% 公开 API |
| 集成测试 | wiremock + tokio-tungstenite (dev-only echo server) | Transport 层真实收发 | 正向+错误路径 |
| 韧性测试 | 模拟网络故障 | Retry/CB 状态机 | 状态迁移覆盖 |
| FFI 测试 | cargo test (C ABI) / dart test | FFI 边界正确性 | 序列化/回调 |
| 绑定测试 | napi-test / dart test | Napi / UniFFI / Dart FFI 绑定 | smoke test |
| 跨平台测试 | CI matrix | 所有目标平台 | 编译+基础功能 |

---

## 单元测试

```rust
#[tokio::test]
async fn retry_recovers_on_transient_failure() {
    // 前 2 次返回 ConnectionTimeout，第 3 次成功
}

#[tokio::test]
async fn retry_fails_fast_on_4xx() {
    // 返回 403，不重试，直接返回错误
}

#[tokio::test]
async fn cb_opens_after_consecutive_failures() {
    // 5 次失败 → OPEN → 第 6 次直接返回 CircuitBreakerOpen
}

#[tokio::test]
async fn cb_half_open_to_closed_on_success() {
    // OPEN → 等待 reset_timeout → HALF_OPEN → 成功 → CLOSED
}

#[tokio::test]
async fn endpoint_racing_wins_first_and_cancels_others() {
    // 3 个 endpoint，第 2 个先成功 → 其余关闭
}

#[tokio::test]
async fn adaptive_timeout_adjusts_from_rtt_window() {
    // record [100, 200, 300] → P90 = 300 → timeout = 300 * 5 = 1500ms
}

#[tokio::test]
async fn encode_error_is_non_retryable() {
    // 序列化失败 → 不重试，直接返回
}

#[tokio::test]
async fn decode_error_is_non_retryable() {
    // 反序列化失败 → 不重试，直接返回
}
```

---

## 集成测试

- 使用 `wiremock` 启动本地模拟 HTTP 服务器，验证 Transport 层完整的请求-响应流程。
- 使用 `tokio-tungstenite`（dev-only）搭建 WebSocket echo server，验证 yawc 客户端连接建立、消息收发、断开重连与断线期间消息缓冲重放。
- 覆盖场景：正常响应、超时、5xx 重试、4xx 快速失败、连接被拒绝。

---

## 韧性测试

- 通过 Toxiproxy 或自定义 fault injector 模拟延迟、丢包、连接重置。
- 验证重试退避策略（指数退避 + 抖动）的实际行为。
- 验证熔断器 OPEN ↔ HALF_OPEN ↔ CLOSED 完整状态迁移。

---

## 跨平台 CI

- GitHub Actions matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`。
- 每个平台编译 `catcher-rs`、运行单元测试、运行集成测试。
- FFI 绑定在对应平台单独测试（Node.js napi 测试、Flutter widget 测试）。

---

## 测试覆盖现状与缺口

> 数据来源：全量 Rust 源码 (51 .rs 文件) 逐文件检查

### 覆盖总览

```
Rust 核心逻辑 ──── ✅ 105 个测试
    │
    ├─ C ABI  (http_ffi / ws_ffi / quality_ffi) ─ ❌ 0 测试
    │     │
    │     └─ Dart FFI  ─ ❌ 集成测试无法跑（需手动设环境变量）
    │
    ├─ Napi   (napi-http / napi-ws) ── ❌ 0 测试
    │
    └─ UniFFI (catcher-uniffi) ──────── ❌ 0 测试
```

**所有 FFI 层都是"相信它正确"，没有任何自动化验证。**

### Rust 层测试分布

**有测试的 17 个文件（~105 个测试）**：resilience (retry/CB/backoff/timeout)、transport (http/tls/dns)、sse (router/stream/client)、ws (codec/heartbeat/reconnect)、scheduler (priority_queue/concurrency)、observability (metrics/network_quality)

**待新增（N-01~N-04 原生能力缺口）**：transport/http_client +12、observability/network_quality +8、multipart/builder +10、FFI 层 +27、Dart 层 +15 = **+72 用例**

**零测试的 17 个文件**：所有 FFI 绑定层 (http_ffi/ws_ffi/quality_ffi/napi-http/napi-ws/uniffi) + WsTransport + multi_endpoint + compression + CatcherError

### 测试缺口清单（按严重度）

| # | 缺口 | 严重度 | 说明 |
|---|------|:------:|------|
| TEST-01 | **FFI C ABI 层零测试** | 🔴🔴 | http_ffi 8 符号 + ws_ffi 5 符号 + quality_ffi 1 符号 = 14 个 C 函数完全未测试 |
| TEST-02 | **Dart 集成测试需手动激活** | 🔴 | `CATCHER_FFI_PATH` 环境变量，CI 不可跑 |
| TEST-03 | **Napi binding 零测试** | 🔴 | napi-http/ws 只有集成测试的 smoke test |
| TEST-04 | **UniFFI 零测试** | 🔴 | 360 行代码无任何测试 |
| TEST-05 | **WsTransport 零测试** | 🔴 | WS 核心传输层无测试 |
| TEST-06 | **multi_endpoint 零测试** | 🔴 | 多端点竞速逻辑无测试 |
| TEST-07 | **compression 零测试** | 🟡 | perMessageDeflate 无测试 |
| TEST-08 | **CatcherError / ErrorCategory 零测试** | 🟡 | 错误类型及其可重试分类逻辑无测试 |
| TEST-09 | **Dart 单元仅测序列化** | 🟡 | 从未测试 CatcherHttpClient/CatcherWsClient 的实际创建和调用 |
| TEST-10 | **TS 测试未覆盖 Napi 路径** | 🟡 | TS 测试只测纯 TS 层，不走 native binding |

---

## FFI 层测试补全方案 📐

### C ABI 测试 (`crates/catcher-ffi/tests/`)

新增 5 个集成测试文件，使用 `wiremock` / local echo server 验证每个 C ABI 符号：

| 测试文件 | 覆盖符号 | 关键场景 |
|---------|---------|---------|
| `http_test.rs` | `catcher_http_*` (8+3) | client create/destroy, GET 200, POST body, headers 透传, timeout, cancel_all, CB state, metrics |
| `ws_test.rs` | `catcher_ws_*` (5) | connect, send text/binary, receive events, close with code, destroy |
| `sse_test.rs` | `catcher_sse_*` (6) | connect/open/data/close, ready_state transitions, last_event_id, auto-reconnect |
| `quality_test.rs` | `catcher_evaluate_quality` | RTT evaluation callback |
| `codec_test.rs` | `catcher_pack / catcher_unpack` | JSON → msgpack → JSON roundtrip, error handling |

测试模式：C ABI 测试在 Rust 侧以 `#[cfg(test)]` + `extern "C"` 方式直接调用 C 函数签名，通过回调收集结果。

### Dart 集成测试 CI 兼容

当前 Dart 集成测试需要 `CATCHER_FFI_PATH` 环境变量。修复方案：

1. CI 中增加 `cargo build --release -p catcher-ffi` 步骤
2. 设置 `CATCHER_FFI_PATH` 指向编译产物
3. 或改为 `DynamicLibrary.open()` 自动查找 `target/release/` 下的产物

### Napi 绑定测试

`catcher-napi-http` 和 `catcher-napi-ws` 各自增加至少一个 integration test（使用 `ava` 或 `vitest`），验证：
- native module 加载成功
- HttpClient/WsClient 创建
- 基础 GET / WS connect

### UniFFI 绑定测试

`catcher-uniffi` 增加一个 Rust integration test，验证：
- UniFFI scaffolding 编译
- HTTP client 创建/destroy
- WS client 事件回调

