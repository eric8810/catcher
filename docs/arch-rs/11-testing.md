# 11 — 测试策略

## 测试层级

| 层级 | 工具 | 目标 | 覆盖 |
|------|------|------|------|
| 单元测试 | tokio::test | 每个模块纯逻辑 | 100% 公开 API |
| 集成测试 | wiremock + tokio-tungstenite | Transport 层真实收发 | 正向+错误路径 |
| 韧性测试 | 模拟网络故障 | Retry/CB 状态机 | 状态迁移覆盖 |
| FFI 测试 | napi-test / flutter_test | FFI 边界正确性 | 序列化/回调 |
| 跨平台测试 | CI matrix | 所有目标平台 | 编译+基础功能 |

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

## 集成测试

- 使用 `wiremock` 启动本地模拟 HTTP 服务器，验证 Transport 层完整的请求-响应流程。
- 使用 `tokio-tungstenite` 搭建 WebSocket mock，验证连接建立、心跳、断开重连。
- 覆盖场景：正常响应、超时、5xx 重试、4xx 快速失败、连接被拒绝。

## 韧性测试

- 通过 Toxiproxy 或自定义 fault injector 模拟延迟、丢包、连接重置。
- 验证重试退避策略（指数退避 + 抖动）的实际行为。
- 验证熔断器 OPEN ↔ HALF_OPEN ↔ CLOSED 完整状态迁移。

## 跨平台 CI

- GitHub Actions matrix: `ubuntu-latest`, `macos-latest`, `windows-latest`。
- 每个平台编译 `catcher-rs`、运行单元测试、运行集成测试。
- FFI 绑定在对应平台单独测试（Node.js napi 测试、Flutter widget 测试）。
