# Rust 使用指南

> 状态：✅ 已发布 — `catcher-http` / `catcher-ws` / `catcher-core` crates.io  
> 代码位置：`packages/catcher-http/` `packages/catcher-ws/` `packages/catcher-core/`

---

## 一、Crate 概览

| Crate | crates.io | 说明 |
|-------|-----------|------|
| `catcher-core` | `catcher_core` | 共享类型、错误定义、SSE 类型、编解码 trait |
| `catcher-http` | `catcher_http` | HTTP 传输层（reqwest + 重试中间件 + 熔断器 + 自适应超时） |
| `catcher-ws` | `catcher_ws` | WebSocket 传输层（指数退避重连 + 心跳 + 多端点竞速） |
| `catcher-ffi` | `catcher_ffi` | cdylib umbrella，导出 16 个 C ABI 符号用于跨语言绑定 |
| `catcher-uniffi` | `catcher_uniffi` | UniFFI 绑定，面向 Kotlin / Swift |

依赖顺序：`catcher-core` → `catcher-http` / `catcher-ws` → `catcher-ffi` / `catcher-uniffi`

---

## 二、依赖声明

```toml
# Cargo.toml
[dependencies]
catcher-http = "0.3.0"
catcher-ws = "0.3.0"
catcher-core = "0.3.0"
tokio = { version = "1", features = ["full"] }
```

---

## 三、HTTP 客户端

### 3.1 创建与配置

```rust
use catcher_http::{HttpTransport, types::http::*};
use catcher_core::types::resilience::{RetryConfig, BackoffKind, CircuitBreakerConfig};

let config = HttpClientConfig {
    base_url: "https://api.example.com".into(),
    connect_timeout_ms: 5000,
    response_timeout_ms: 30_000,
    pool: PoolConfig {
        keep_alive: true,
        max_idle_per_host: 10,
        idle_timeout_secs: 60,
        keep_alive_interval_secs: 30,
    },
    retry: Some(RetryConfig {
        max_attempts: 3,
        backoff: BackoffKind::Exponential,
        min_backoff_ms: 500,
        max_backoff_ms: 30_000,
        jitter: true,
    }),
    circuit_breaker: Some(CircuitBreakerConfig {
        failure_threshold: 5,
        success_threshold: 2,
        reset_timeout_ms: 30_000,
        half_open_max_requests: 5,
    }),
    ..Default::default()
};

let transport = HttpTransport::new(config)?;
```

### 3.2 发送请求

```rust
use catcher_http::types::http::{HttpRequest, HttpMethod};
use std::collections::HashMap;

// 使用便捷方法
let resp = transport.get("/users/1", None, None).await?;
let resp = transport.post("/messages", Some(&body), Some(&headers)).await?;

// 使用通用 execute
let request = HttpRequest {
    method: HttpMethod::Get,
    url: "/channels".into(),
    headers: HashMap::new(),
    body: None,
    content_type: None,
    timeout_ms: Some(5000),
};
let resp = transport.execute(request).await?;

println!("Status: {}, Body: {:?}", resp.status, resp.body);
```

### 3.3 熔断器状态

```rust
use catcher_core::types::resilience::CbState;

match transport.circuit_breaker_state() {
    Some(CbState::Closed) => println!("正常"),
    Some(CbState::Open) => println!("熔断中"),
    Some(CbState::HalfOpen) => println!("半开探测中"),
    None => println!("未启用熔断器"),
}
```

---

## 四、WebSocket 客户端

### 4.1 连接与事件

```rust
use catcher_ws::{WsTransport, types::ws::{WsClientConfig, WsEvent}};

let config = WsClientConfig {
    urls: vec!["wss://cn.example.com".into(), "wss://sg.example.com".into()],
    reconnect: Some(ReconnectConfig {
        initial_delay_ms: 1000,
        max_delay_ms: 30_000,
        backoff_multiplier: 2.0,
        max_attempts: 20,
    }),
    heartbeat: Some(HeartbeatConfig {
        interval_ms: 30_000,
        adaptive: true,
    }),
    ..Default::default()
};

let (handle, mut rx) = WsTransport::connect("wss://cn.example.com", &config).await?;

// 接收事件
while let Some(event) = rx.recv().await {
    match event {
        WsEvent::Connected { url, latency_ms } => {
            println!("已连接 {} ({}ms)", url, latency_ms);
        }
        WsEvent::Message { data, is_binary } => {
            if is_binary {
                let decoded = catcher_ws::unpack_value(&data)?;
                println!("收到: {:?}", decoded);
            } else {
                println!("收到: {}", String::from_utf8_lossy(&data));
            }
        }
        WsEvent::Disconnected { code, reason } => {
            println!("断开: {} {}", code, reason);
        }
        WsEvent::Error { message } => {
            eprintln!("错误: {}", message);
        }
        WsEvent::Reconnecting { attempt, delay_ms } => {
            println!("重连中: 第{}次, {}ms后", attempt, delay_ms);
        }
        WsEvent::HeartbeatRtt { rtt_ms } => {
            println!("心跳 RTT: {}ms", rtt_ms);
        }
    }
}
```

### 4.2 发送消息

```rust
// 发送文本
handle.send_text("hello").await?;

// 发送二进制 (msgpack)
let packed = catcher_ws::pack(&serde_json::json!({
    "event": "message",
    "data": {"text": "hello"}
}))?;
handle.send_binary(&packed).await?;

// 关闭
handle.close(1000, "normal").await?;
```

---

## 五、独立使用韧性组件

### 5.1 熔断器

```rust
use catcher_http::CircuitBreaker;
use catcher_core::types::resilience::CircuitBreakerConfig;

let cb = CircuitBreaker::new(CircuitBreakerConfig {
    failure_threshold: 3,
    success_threshold: 2,
    reset_timeout_ms: 30_000,
    half_open_max_requests: 5,
});

// 请求前检查
cb.before_request()?;

// 标注结果
if success {
    cb.on_success();
} else {
    cb.on_failure();
}

// 查询状态
println!("State: {:?}", cb.state());

// 手动重置
cb.reset();
```

### 5.2 重试

```rust
use catcher_http::retry_with_backoff;
use catcher_core::types::resilience::{RetryConfig, BackoffKind};
use catcher_core::CatcherError;

let config = RetryConfig {
    max_attempts: 3,
    backoff: BackoffKind::DecorrelatedJitter,
    min_backoff_ms: 100,
    max_backoff_ms: 10_000,
    jitter: true,
};

let result = retry_with_backoff(
    &config,
    || async {
        // 你的网络操作
        Ok::<_, CatcherError>(42)
    },
    |e| matches!(e, CatcherError::ConnectionTimeout(_)),
    |attempt, error| {
        eprintln!("重试 #{}, 错误: {}", attempt, error);
    },
).await;
```

### 5.3 自适应超时

```rust
use catcher_http::AdaptiveTimeout;
use std::time::Duration;

// 滑动窗口 P90 RTT × multiplier，限制在 [min, max]
let mut timeout = AdaptiveTimeout::new(
    500,     // min_timeout_ms
    30_000,  // max_timeout_ms
    3.0,     // multiplier
    100,     // window_size
);

// 记录每次请求的 RTT
timeout.record(120);
timeout.record(250);
timeout.record(180);

// 获取当前自适应超时
let dur: Duration = timeout.compute();
println!("自适应超时: {}ms", dur.as_millis());

// 获取滑动窗口快照
let snapshot = timeout.snapshot();
println!("P50 RTT: {}ms, P90: P90 from sorted window", snapshot.avg_rtt_ms);
```

---

## 六、二进制编解码 (msgpack)

```rust
use catcher_ws::codec::{pack, unpack, unpack_value};

// 编码
let data = serde_json::json!({"event": "ping", "ts": 1234567890u64});
let encoded: Vec<u8> = pack(&data)?;

// 解码到指定类型
#[derive(serde::Deserialize)]
struct PingEvent { event: String, ts: u64 }
let decoded: PingEvent = unpack(&encoded)?;

// 解码为 serde_json::Value
let value: serde_json::Value = unpack_value(&encoded)?;
```

---

## 七、可观测性 (Metrics)

```rust
use catcher_http::{MetricsCollector, MetricsSnapshot};

let mut metrics = MetricsCollector::new();

// 记录请求
metrics.record_request(200, 42);  // status, latency_ms
metrics.record_retry();
metrics.record_circuit_breaker_open();

let snapshot: MetricsSnapshot = metrics.snapshot();
println!("总请求: {}, 成功率: {:.1}%",
    snapshot.total_requests,
    snapshot.success_rate,
);
```

---

## 八、跨平台差异

| 能力 | Rust | TypeScript (napi) |
|------|:----:|:-----------------:|
| 底层网络 | reqwest | reqwest (via napi) |
| 重试 | backon | p-retry |
| 熔断器 | 自实现状态机 | cockatiel |
| 编解码 | rmpv | msgpackr |
| 拦截器 | ❌ | ✅ TS 版 |
| SSE | ✅ `SseClient` / `SseStream` | ✅ `createSSEStream` / `createSSEClient` |

---

## 九、错误处理

Rust 使用 `catcher_core::CatcherError` 枚举：

```rust
match error {
    CatcherError::ConnectionTimeout(ms) => eprintln!("连接超时: {}ms", ms),
    CatcherError::CircuitBreakerOpen => eprintln!("熔断器开启，快速失败"),
    CatcherError::RetryExhausted { attempts, last_error } =>
        eprintln!("重试 {} 次后失败: {}", attempts, last_error),
    CatcherError::HttpError { status, message } =>
        eprintln!("HTTP {}: {}", status, message),
    _ => eprintln!("其他错误: {}", error),
}

// 错误可重试性判断
match error.category() {
    ErrorCategory::Retryable => { /* 可以重试 */ }
    ErrorCategory::NonRetryable => { /* 比如 4xx */ }
    ErrorCategory::Fatal => { /* 配置错误等，不应重试 */ }
}
```

详细错误处理见 [`error-handling.md`](./error-handling.md)。
