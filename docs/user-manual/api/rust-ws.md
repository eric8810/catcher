# catcher-ws API Reference

> Rust WebSocket 传输层 crate — `catcher-ws` 0.2.x

```toml
[dependencies]
catcher-ws = "0.3"
tokio = { version = "1", features = ["full"] }
```

---

## 模块结构

```
catcher-ws
├── transport  → WsTransport, WsHandle
├── ws         → EndpointRacer, HeartbeatManager, ReconnectManager, build_ws_options
├── codec      → pack, unpack, unpack_value
├── ffi        → C ABI (内部使用)
└── types      → 类型定义
```

## 公开 API

```rust
use catcher_ws::{
    // 传输层
    WsTransport,
    WsHandle,

    // 韧性管理器
    EndpointRacer,
    HeartbeatManager,
    ReconnectManager,
    build_ws_options,

    // 编解码
    pack,
    unpack,
    unpack_value,
};

use catcher_ws::types::ws::{
    WsClientConfig,
    WsEvent,
    WsState,
    HeartbeatConfig,
    ReconnectConfig,
};
```

---

## WsTransport

```rust
pub struct WsTransport;

impl WsTransport {
    /// 连接 WebSocket 服务器
    /// 返回 (WsHandle, mpsc::Receiver<WsEvent>)
    pub async fn connect(
        url: &str,
        config: &WsClientConfig,
    ) -> Result<(WsHandle, tokio::sync::mpsc::UnboundedReceiver<WsEvent>), CatcherError>;
}
```

### WsHandle

```rust
pub struct WsHandle { /* private */ }

impl WsHandle {
    /// 发送文本消息
    pub fn send_text(&self, data: &str) -> Result<(), CatcherError>;

    /// 发送二进制消息
    pub fn send_binary(&self, data: &[u8]) -> Result<(), CatcherError>;

    /// 关闭连接
    pub fn close(&self, code: u16, reason: &str) -> Result<(), CatcherError>;
}
```

### WsClientConfig

所有配置字段均支持 `snake_case` 和 `camelCase` 两种命名（通过 `#[serde(alias)]`）。

```rust
pub struct WsClientConfig {
    pub urls: Vec<String>,                        // 多端点 URL 列表
    pub protocols: Option<Vec<String>>,           // 子协议
    pub headers: HashMap<String, String>,         // 默认 {}
    pub per_message_deflate: bool,                // 默认 true
    pub deflate_threshold_bytes: u32,             // 默认 1024
    pub handshake_timeout_ms: u64,                // 默认 15000
    pub max_payload_bytes: u64,                   // 默认 67108864 (64MB)
    pub reconnect: Option<ReconnectConfig>,
    pub heartbeat: Option<HeartbeatConfig>,
    pub race_count: u32,                          // 默认 1
}

pub struct ReconnectConfig {
    pub initial_delay_ms: u64,      // 默认 500
    pub max_delay_ms: u64,          // 默认 30000
    pub backoff_multiplier: f64,    // 默认 2.0
    pub max_attempts: u32,          // 默认 20
}

pub struct HeartbeatConfig {
    pub interval_ms: u64,           // 默认 30000
    pub adaptive: bool,             // 默认 true — 基于 RTT 动态调整
    pub pong_timeout_ms: u64,       // 默认 10000 — pong 无响应视为断线
    pub max_missed_pongs: u32,      // 默认 3 — 连续丢失 pong 判定断线
}
```

### WsEvent

使用 `#[serde(tag = "type")]` 标签枚举序列化，JSON 输出含 `"type"` 字段。

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    Connected { url: String, latency_ms: u64 },
    Disconnected { code: u16, reason: String },
    Message { data: Vec<u8>, is_binary: bool },
    Error { message: String },
    Reconnecting { attempt: u32, delay_ms: u64 },
    HeartbeatRtt { rtt_ms: u64 },
}
```

> `WsEvent::Message` 在 FFI 层序列化时使用 `data_base64`（base64 编码）替代 `data`（`Vec<u8>`），
> 避免二进制展开为 JSON 数字数组。

### 示例

```rust
use catcher_ws::{WsTransport, types::ws::*};
use std::collections::HashMap;

let config = WsClientConfig {
    urls: vec![
        "wss://cn.example.com".into(),
        "wss://sg.example.com".into(),
    ],
    reconnect: Some(ReconnectConfig {
        initial_delay_ms: 500,
        max_delay_ms: 30_000,
        max_attempts: 20,
        ..Default::default()
    }),
    heartbeat: Some(HeartbeatConfig {
        interval_ms: 30_000,
        adaptive: true,
        ..Default::default()
    }),
    ..WsClientConfig::default()
};

let (handle, mut rx) = WsTransport::connect("wss://cn.example.com", &config).await?;

// 接收事件
while let Some(event) = rx.recv().await {
    match event {
        WsEvent::Connected { url, latency_ms } =>
            println!("Connected to {} ({}ms)", url, latency_ms),
        WsEvent::Message { data, is_binary } => {
            if is_binary {
                let decoded = catcher_ws::unpack_value(&data)?;
                println!("Received: {:?}", decoded);
            } else {
                println!("Received: {}", String::from_utf8_lossy(&data));
            }
        }
        WsEvent::Disconnected { code, reason } =>
            println!("Disconnected: {} {}", code, reason),
        WsEvent::Reconnecting { attempt, delay_ms } =>
            println!("Reconnecting attempt {} in {}ms", attempt, delay_ms),
        WsEvent::HeartbeatRtt { rtt_ms } =>
            println!("Heartbeat RTT: {}ms", rtt_ms),
        WsEvent::Error { message } =>
            eprintln!("Error: {}", message),
    }
}

// 发送消息
handle.send_text("hello").await?;
handle.send_binary(b"binary data").await?;

// 关闭
handle.close(1000, "normal").await?;
```

---

## EndpointRacer

```rust
pub struct EndpointRacer { /* private */ }

impl EndpointRacer {
    pub fn new(config: WsClientConfig, race_count: usize) -> Self;

    /// 竞速连接，返回第一个成功的连接
    pub async fn race(&self) -> Result<(WsHandle, String), CatcherError>;
}
```

并发连接所有端点，使用第一个成功的结果，关闭其余连接。

---

## HeartbeatManager

```rust
pub struct HeartbeatManager { /* private */ }

impl HeartbeatManager {
    pub fn new(config: HeartbeatConfig) -> Self;

    /// 记录 RTT 样本，自适应调整心跳间隔
    pub fn record_rtt(&mut self, rtt_ms: u64);

    /// 获取下一次心跳间隔
    pub fn next_interval(&self) -> Duration;
}
```

自适应间隔算法：网络良好时心跳间隔拉长，弱网时缩短以快速检测断线。

---

## ReconnectManager

```rust
pub struct ReconnectManager { /* private */ }

impl ReconnectManager {
    pub fn new(config: ReconnectConfig) -> Self;

    /// 计算下一次重连延迟。返回 None 表示超过最大次数
    pub fn next_delay(&mut self) -> Option<Duration>;

    /// 重置重连计数器（成功连接后调用）
    pub fn reset(&mut self);

    /// 当前已尝试次数
    pub fn attempt_count(&self) -> u32;
}
```

退避算法：`delay = min(initial × multiplier^(attempt-1), max) + jitter(±25%)`

---

## 编解码 API

### pack

```rust
pub fn pack<T: Serialize>(value: &T) -> Result<Vec<u8>, CatcherError>
```

将可序列化值编码为 msgpack 二进制。

### unpack

```rust
pub fn unpack<T: DeserializeOwned>(data: &[u8]) -> Result<T, CatcherError>
```

从 msgpack 二进制解码为指定类型。

### unpack_value

```rust
pub fn unpack_value(data: &[u8]) -> Result<serde_json::Value, CatcherError>
```

从 msgpack 解码为 `serde_json::Value`（无 schema）。

### 示例

```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
struct Position { lat: f64, lng: f64 }

// 编码
let pos = Position { lat: 22.3, lng: 114.1 };
let encoded = catcher_ws::pack(&pos)?;

// 解码到结构体
let decoded: Position = catcher_ws::unpack(&encoded)?;

// 解码到通用 Value
let value: serde_json::Value = catcher_ws::unpack_value(&encoded)?;
```

---

## build_ws_options

```rust
pub fn build_ws_options(config: &WsClientConfig) -> yawc::Options
```

从 catcher 配置构建底层 yawc 配置。内部使用，但公开可调用。

---

## 默认值速查

| 参数 | 默认值 |
|------|--------|
| `per_message_deflate` | `true` |
| `deflate_threshold_bytes` | `1024` |
| `handshake_timeout_ms` | `15000` |
| `max_payload_bytes` | `67108864` (64MB) |
| `race_count` | `1` |
| `reconnect.initial_delay_ms` | `500` |
| `reconnect.max_delay_ms` | `30000` |
| `reconnect.backoff_multiplier` | `2.0` |
| `reconnect.max_attempts` | `20` |
| `heartbeat.interval_ms` | `30000` |
| `heartbeat.adaptive` | `true` |
| `heartbeat.pong_timeout_ms` | `10000` |
| `heartbeat.max_missed_pongs` | `3` |
