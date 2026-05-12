# 03 — Phase 2: Transport Layer (HTTP + WebSocket)

> 对应 arch 文档：`04-transport.md`
> 工期：7 人天
> 目标：HTTP 和 WebSocket 的真实收发可用，不包含重试/熔断（Phase 3）

---

## 1. 模块概览

```
src/transport/
├── mod.rs             # re-export HttpTransport, WsTransport
├── http_client.rs     # HttpTransport: reqwest + reqwest-middleware 封装
├── ws_client.rs       # WsTransport: stream-tungstenite 封装
├── tls.rs             # build_tls_config: TlsConfig → reqwest ClientBuilder
└── dns.rs             # build_dns_resolver: DnsConfig → hickory-resolver

src/ws/
├── mod.rs
├── reconnect.rs       # ReconnectManager: 重连状态机
├── heartbeat.rs       # HeartbeatManager: 自适应心跳
├── multi_endpoint.rs  # EndpointRacer: 多端点竞速
└── compression.rs     # DeflateConfig 适配 tungstenite
```

---

## 2. 实现步骤

### Step 2.1 — `src/transport/tls.rs`

**参考**：`arch-rs/04-transport.md`

```rust
use crate::types::http::TlsConfig;
use crate::error::CatcherError;
use reqwest::ClientBuilder;

/// 将 TlsConfig 应用到 reqwest ClientBuilder
pub fn build_tls_config(
    builder: &mut ClientBuilder,
    config: &TlsConfig,
) -> Result<(), CatcherError> {
    if !config.reject_unauthorized {
        // ⚠️ 仅测试环境使用
        builder.danger_accept_invalid_certs(true);
    }
    // CA 证书
    if let Some(ref pem) = config.ca_cert_pem {
        let cert = reqwest::Certificate::from_pem(pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(e.to_string()))?;
        builder.add_root_certificate(cert);
    }
    // 客户端证书
    if let (Some(ref cert_pem), Some(ref key_pem)) =
        (&config.client_cert_pem, &config.client_key_pem) {
        let key = reqwest::Identity::from_pem(
            format!("{}{}", cert_pem, key_pem).as_bytes()
        ).map_err(|e| CatcherError::TlsError(e.to_string()))?;
        builder.identity(key);
    }
    Ok(())
}
```

**测试**：`tests/transport/tls.rs`
- `default_config_reject_unauthorized_true`
- `insecure_config_accepts_invalid_cert`（需 wiremock TLS）

### Step 2.2 — `src/transport/dns.rs`

**参考**：`arch-rs/04-transport.md`

```rust
use crate::types::http::DnsConfig;
use crate::error::CatcherError;
use hickory_resolver::TokioAsyncResolver;

/// 根据 DnsConfig 构建 hickory-resolver
/// 
/// 仅在 feature = "hickory-dns" 时编译
pub fn build_dns_resolver(
    config: &DnsConfig,
) -> Result<Option<TokioAsyncResolver>, CatcherError> {
    #[cfg(feature = "hickory-dns")]
    {
        // 构建 hickory resolver 配置
        let mut resolver_config = hickory_resolver::config::ResolverConfig::new();
        if !config.nameservers.is_empty() {
            // 自定义 DNS 服务器
            let group = hickory_resolver::config::NameServerConfigGroup::from(
                config.nameservers.iter().map(|ns| {
                    let socket_addr: std::net::SocketAddr = ns.parse()
                        .map_err(|_| CatcherError::InvalidConfig(
                            format!("invalid nameserver: {ns}")
                        ))?;
                    Ok(hickory_resolver::config::NameServerConfig {
                        socket_addr,
                        protocol: hickory_resolver::config::Protocol::Udp,
                        tls_dns_name: None,
                        trust_negative_responses: true,
                        bind_addr: None,
                    })
                }).collect::<Result<Vec<_>, CatcherError>>()?
            );
            resolver_config = hickory_resolver::config::ResolverConfig::from_parts(
                None, vec![], group
            );
        }
        let resolver = TokioAsyncResolver::tokio(
            resolver_config,
            hickory_resolver::config::ResolverOpts::default(),
        );
        Ok(Some(resolver))
    }
    #[cfg(not(feature = "hickory-dns"))]
    {
        // 没有 hickory-dns feature，回退到系统 DNS (reqwest 默认行为)
        let _ = config;
        Ok(None)
    }
}
```

**测试**：`tests/transport/dns.rs`
- `resolver_returns_none_without_hickory_dns_feature`
- `resolver_parses_custom_nameservers`

### Step 2.3 — `src/transport/http_client.rs`

**参考**：`arch-rs/04-transport.md`（完整 `HttpTransport::new` + `execute`）

实现 `HttpTransport` struct：

```rust
pub struct HttpTransport {
    client: reqwest::Client,        // Phase 2 直接用 reqwest::Client
    config: HttpClientConfig,       // Phase 3 升级为 reqwest_middleware
}

impl HttpTransport {
    pub fn new(config: HttpClientConfig) -> Result<Self, CatcherError> { /* todo */ }
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> { /* todo */ }
    pub async fn get(&self, url: &str) -> Result<HttpResponse, CatcherError> { /* todo */ }
    pub async fn post(&self, url: &str, body: &[u8], content_type: &str) -> Result<HttpResponse, CatcherError> { /* todo */ }
}
```

**关键实现要点**：

1. `new()` 中将 `HttpClientConfig` 转换为 `reqwest::ClientBuilder` 调用链：
   - `connect_timeout`, `pool_max_idle_per_host`, `pool_idle_timeout`, `tcp_keepalive`
   - 调用 `build_tls_config()` 和 `build_dns_resolver()`
2. `execute()` 中：
   - 将 `HttpMethod` 映射为 `reqwest::Method`
   - 拼接 base_url + path
   - 设置 headers / body / content-type
   - 计时 `elapsed_ms`
   - 处理 HTTP 错误状态码 (4xx/5xx → `HttpError`)
3. Phase 2 **不包含 retry middleware**（Phase 3 升级为 `reqwest_middleware::ClientWithMiddleware`）

**测试**：`tests/transport/http_client_test.rs`

使用 `wiremock` 启动本地 HTTP mock server：

| 测试 | 描述 |
|------|------|
| `get_200_returns_response` | 正常 GET 请求，返回 200 + body |
| `get_404_returns_http_error` | 404 响应 → `CatcherError::HttpError { status: 404 }` |
| `get_500_returns_http_error` | 500 响应 → `CatcherError::HttpError { status: 500 }` |
| `post_json_roundtrip` | POST JSON → 服务端回显 → 验证一致性 |
| `connect_timeout_error` | 连接超时 → `CatcherError::ConnectionTimeout` |
| `response_timeout_error` | 响应超时 → `CatcherError::RequestTimeout` |
| `keepalive_reuse_connection` | 两次 GET 使用同一连接 |
| `elapsed_ms_is_set` | `HttpResponse.elapsed_ms > 0` |
| `custom_headers_forwarded` | 自定义 header 被发送到服务端 |
| `base_url_prefix` | 相对路径自动拼接 base_url |

wiremock 配置：
```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

let server = MockServer::start().await;
Mock::given(method("GET"))
    .and(path("/channels"))
    .respond_with(ResponseTemplate::new(200).set_body_json(json!({...})))
    .mount(&server)
    .await;

let config = HttpClientConfig {
    base_url: server.uri(),
    connect_timeout_ms: 5000,
    response_timeout_ms: 10000,
    ..Default::default()
};
let transport = HttpTransport::new(config)?;
let resp = transport.get("/channels").await?;
assert_eq!(resp.status, 200);
```

### Step 2.4 — `src/transport/ws_client.rs`

**参考**：`arch-rs/04-transport.md`

使用 `stream-tungstenite` 实现 `WsTransport`：

```rust
use stream_tungstenite::{StreamTungstenite, Event as WsStreamEvent};

pub struct WsTransport {
    inner: Option<StreamTungstenite>,
    config: WsClientConfig,
    event_tx: mpsc::UnboundedSender<WsEvent>,
}

impl WsTransport {
    pub fn new(config: WsClientConfig, event_tx: mpsc::UnboundedSender<WsEvent>) -> Self;
    pub async fn connect(&mut self, url: &str) -> Result<(), CatcherError>;
    pub async fn send_text(&mut self, text: &str) -> Result<(), CatcherError>;
    pub async fn send_binary(&mut self, data: &[u8]) -> Result<(), CatcherError>;
    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), CatcherError>;
}
```

**关键实现要点**：

1. `connect()` 使用 `stream-tungstenite` 的 `connect_with_config()`，传入 WebSocketConfig（perMessageDeflate 配置来自 `ws/compression.rs`）
2. 连接成功后 spawn 一个 tokio task 驱动 `StreamTungstenite` 的 event loop：
   - `WsStreamEvent::Text/ Binary` → event_tx.send(WsEvent::Message { ... })
   - `WsStreamEvent::Connected` → event_tx.send(WsEvent::Connected { ... })
   - `WsStreamEvent::Disconnected` → event_tx.send(WsEvent::Disconnected { ... })
3. `send_text/send_binary` 通过 `StreamTungstenite::send()` 发送

**Phase 2 关注点**：连接建立 + 收发帧。重连/心跳/多端点竞速在 Step 2.5-2.8 实现。

### Step 2.5 — `src/ws/compression.rs`

将 `WsClientConfig` 的 `per_message_deflate` / `deflate_threshold_bytes` 转换为 tungstenite 的 `WebSocketConfig`：

```rust
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

pub fn build_ws_config(config: &WsClientConfig) -> WebSocketConfig {
    WebSocketConfig {
        max_send_queue: None,
        max_message_size: Some(config.max_payload_bytes as usize),
        max_frame_size: Some(config.max_payload_bytes as usize),
        // 其它字段按 per_message_deflate / deflate_threshold 设置
        ..Default::default()
    }
}
```

### Step 2.6 — `src/ws/reconnect.rs`

**参考**：`arch-rs/12-state-machines.md` (WS 连接状态机)

```rust
pub struct ReconnectManager {
    config: ReconnectConfig,
    state: WsState,
    attempt: u32,
    current_delay_ms: u64,
}

impl ReconnectManager {
    pub fn new(config: ReconnectConfig) -> Self;
    
    /// 连接失败时调用，返回需要等待的延迟（ms）
    pub fn on_disconnect(&mut self) -> Option<u64>;
    
    /// 连接成功时调用，重置状态
    pub fn on_connected(&mut self);
    
    /// 是否已耗尽重试次数
    pub fn is_exhausted(&self) -> bool;
}
```

**状态迁移**：
```
DISCONNECTED → CONNECTING → (success) → CONNECTED → (断开) → RECONNECTING
RECONNECTING → CONNECTING → ... → (max_attempts) → DISCONNECTED
```

**测试**：`tests/ws/reconnect_test.rs`
- `first_disconnect_returns_initial_delay`
- `second_disconnect_doubles_delay`（指数退避）
- `exhausted_after_max_attempts`
- `on_connected_resets_state`
- `on_connected_after_reconnect_resets_attempt`

### Step 2.7 — `src/ws/heartbeat.rs`

```rust
pub struct HeartbeatManager {
    config: HeartbeatConfig,
    last_pong: Instant,
    missed_pongs: u32,
    rtt_samples: VecDeque<u64>,
}

impl HeartbeatManager {
    pub fn new(config: HeartbeatConfig) -> Self;
    
    /// 返回当前建议的心跳间隔（ms）
    pub fn interval_ms(&self) -> u64;
    
    /// 收到 pong 时调用
    pub fn on_pong(&mut self, rtt_ms: u64);
    
    /// 超时检查：返回 true 表示应该断开（连续丢失 pong）
    pub fn is_timed_out(&self) -> bool;
}
```

**自适应逻辑**：`interval_ms = adaptive ? max(P90_RTT * 2, min_interval) : config.interval_ms`

### Step 2.8 — `src/ws/multi_endpoint.rs`

```rust
pub struct EndpointRacer {
    urls: Vec<String>,
    race_count: u32,
}

impl EndpointRacer {
    pub fn new(urls: Vec<String>, race_count: u32) -> Self;
    
    /// 并发连接所有端点，返回最先成功的那个
    pub async fn race(&self) -> Result<(String, WsTransport), CatcherError>;
}
```

**实现方式**：`tokio::select!` 对所有端点的 `connect()` 并发等待，取最先返回 `Ok` 者。其余用 `abort_handle` 取消。

---

## 3. 测试清单

### 3.1 HTTP Transport 集成测试（wiremock）

| 测试 | 描述 |
|------|------|
| `get_200` | 正常请求返回正确 body |
| `get_404` | 返回 HttpError(404) |
| `get_500` | 返回 HttpError(500) |
| `post_roundtrip` | POST JSON → 服务端回显 |
| `connect_timeout` | 不可达地址 → ConnectionTimeout |
| `response_timeout` | 延迟响应 → RequestTimeout |
| `keepalive_reuse` | 2 次 GET 走同一连接 |
| `elapsed_ms_tracking` | elapsed_ms 正确计时 |
| `custom_headers` | 自定义 headers 被转发 |
| `base_url_join` | `/path` → `base_url/path` |
| `absolute_url` | `http://...` 不走 base_url |

### 3.2 WS Transport 集成测试（tokio-tungstenite mock）

| 测试 | 描述 |
|------|------|
| `connect_send_receive` | 建立连接 → 发送文本 → 收到 echo |
| `send_binary` | 发送二进制帧 → 收到 echo |
| `close_handshake` | close(code, reason) → 服务端收到 |
| `handshake_timeout` | 无效 endpoint → WsHandshakeTimeout |

### 3.3 WS 高级功能单元测试

| 测试 | 描述 |
|------|------|
| `reconnect_exponential_backoff` | 逐次 delay 翻倍 |
| `reconnect_max_attempts_exhausted` | 超过上限返回 None |
| `reconnect_reset_on_connect` | 连接成功后状态重置 |
| `heartbeat_adaptive_interval` | RTT 影响 interval_ms |
| `heartbeat_timeout_on_missed_pongs` | 3 次丢失 pong → is_timed_out() |
| `endpoint_racer_first_wins` | 3 个端点，最快成功 → 其余取消 |

---

## 4. Phase 2 完成标准

- [ ] `cargo check` 零错误
- [ ] `cargo test` 全部通过（HTTP 11 个 + WS 4 个 + 高级功能 6 个 = ≥21 个）
- [ ] `cargo clippy -- -D warnings` 零警告
- [ ] HTTP Transport 支持 GET/POST/PUT/DELETE/PATCH
- [ ] HTTP Transport 支持 TLS 配置（rustls / native-tls 切换）
- [ ] HTTP Transport 支持 DNS 配置（hickory-resolver 可选）
- [ ] WS Transport 支持连接/收发/关闭
- [ ] WS ReconnectManager 状态机通过测试
- [ ] WS HeartbeatManager 自适应逻辑通过测试
- [ ] WS EndpointRacer 多端点竞速通过测试
