# 04 — 传输层

> 对应源文件：`src/transport/`
> 连接池：reqwest 内置基于 `hyper-util` 的连接池，通过 `pool_max_idle_per_host()` / `pool_idle_timeout()` 暴露配置
> 相关 Issue：[N-01~N-03](../issues/native-layer-capability-gaps.md)

---

## HttpTransport (`src/transport/http_client.rs`)

```rust
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;
use retry_policies::policies::ExponentialBackoff;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use crate::error::CatcherError;
use crate::types::http::*;
use crate::types::resilience::RetryConfig;
use crate::transport::tls::build_tls_config;
use crate::transport::dns::build_stale_aware_resolver;

/// HTTP 传输层 — 真实收发 HTTP 请求
pub struct HttpTransport {
    client: ClientWithMiddleware,
    config: HttpClientConfig,
}

impl HttpTransport {
    /// 根据配置构建 HttpTransport
    pub fn new(config: HttpClientConfig) -> Result<Self, CatcherError> {
        let mut reqwest_builder = Client::builder()
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms))
            .pool_max_idle_per_host(config.pool.max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(config.pool.idle_timeout_secs))
            .tcp_keepalive(
                config.pool.keep_alive.then(||
                    Duration::from_secs(config.pool.keep_alive_interval_secs)
                )
            );

        build_tls_config(&mut reqwest_builder, &config.tls)?;

        let resolver = build_stale_aware_resolver(&config.dns)?;
        reqwest_builder = reqwest_builder.dns_resolver(resolver);

        let reqwest_client = reqwest_builder.build()
            .map_err(|e| CatcherError::Internal(format!("reqwest build error: {e}")))?;

        let mut client_builder = ClientBuilder::new(reqwest_client);

        if let Some(ref retry) = config.retry {
            let policy = build_retry_policy(retry);
            client_builder = client_builder
                .with(RetryTransientMiddleware::new_with_policy(policy, "catcher-http"));
        }

        Ok(Self { client: client_builder.build(), config })
    }

    /// 发起 HTTP 请求
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
        let start = Instant::now();
        let method = match request.method {
            HttpMethod::GET => reqwest::Method::GET,
            HttpMethod::POST => reqwest::Method::POST,
            HttpMethod::PUT => reqwest::Method::PUT,
            HttpMethod::DELETE => reqwest::Method::DELETE,
            HttpMethod::PATCH => reqwest::Method::PATCH,
        };

        let url = if request.url.starts_with("http") {
            request.url
        } else {
            format!("{}{}", self.config.base_url.trim_end_matches('/'), request.url)
        };

        let mut req = self.client.request(method, &url);
        for (k, v) in &self.config.default_headers { req = req.header(k, v); }
        for (k, v) in &request.headers { req = req.header(k, v); }

        if let Some(body) = &request.body { req = req.body(body.clone()); }
        if let Some(ms) = request.timeout_ms { req = req.timeout(Duration::from_millis(ms)); }

        let response = req.send().await.map_err(|e| self.map_reqwest_error(e))?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response.headers().iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response.bytes().await
            .map_err(|e| CatcherError::Internal(format!("read body: {e}")))?;

        Ok(HttpResponse {
            status, headers,
            body: body.to_vec(),
            elapsed_ms: start.elapsed().as_millis() as u64,
        })
    }

    fn map_reqwest_error(&self, e: reqwest::Error) -> CatcherError {
        if e.is_timeout() { CatcherError::RequestTimeout(self.config.response_timeout_ms) }
        else if e.is_connect() { CatcherError::ConnectionTimeout(self.config.connect_timeout_ms) }
        else if e.is_decode() { CatcherError::Internal(format!("decode: {e}")) }
        else if let Some(status) = e.status() {
            CatcherError::HttpError { status: status.as_u16(), body: format!("{e}") }
        } else { CatcherError::Internal(format!("request: {e}")) }
    }
}

fn build_retry_policy(config: &RetryConfig) -> ExponentialBackoff {
    ExponentialBackoff::builder()
        .retry_bounds(
            Duration::from_millis(config.min_backoff_ms),
            Duration::from_millis(config.max_backoff_ms),
        )
        .build_with_max_retries(config.max_attempts)
}
```

---

## WsTransport (`src/transport/ws_client.rs`)

```rust
use yawc::{Frame, OpCode};
use tokio::sync::mpsc;
use crate::error::CatcherError;
use crate::types::ws::*;

/// WebSocket 传输层
pub struct WsTransport;

/// WebSocket 连接句柄 — 跨重连保持有效
#[derive(Clone)]
pub struct WsHandle {
    url: String,
    cmd_tx: mpsc::UnboundedSender<WsCommand>,
}

enum WsCommand {
    Text(String),
    Binary(Vec<u8>),
    Close { code: u16, reason: String },
}

impl WsTransport {
    /// 建立 WebSocket 连接，返回句柄和事件接收器。
    /// 支持多端点竞速（EndpointRacer）、自动重连（ReconnectManager）、心跳。
    pub async fn connect(
        config: &WsClientConfig,
    ) -> Result<(WsHandle, mpsc::UnboundedReceiver<WsEvent>), CatcherError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<WsCommand>();

        // 使用 endpoint racer 多端点竞速建立初始连接
        let racer = EndpointRacer::new(config.urls.clone(), config.race_count);
        let dns_resolver = build_dns_resolver(config)?;
        let (url, stream, latency_ms) = racer.race(config, &dns_resolver).await?;

        let handle_url = url.clone();
        let mgr_config = config.clone();

        // 启动连接管理器任务 — 内部循环处理断开、重连、缓冲重放
        tokio::spawn(async move {
            connection_manager(
                url, stream, latency_ms, &mgr_config,
                dns_resolver, event_tx, cmd_rx,
            ).await;
        });

        Ok((WsHandle { url: handle_url, cmd_tx }, event_rx))
    }
}

impl WsHandle {
    /// 发送文本消息（断线期间自动缓冲，重连后重放）
    pub fn send_text(&self, text: &str) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::Text(text.to_string()))
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006, reason: "connection closed".into(),
            })
    }

    /// 发送二进制消息
    pub fn send_binary(&self, data: &[u8]) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::Binary(data.to_vec()))
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006, reason: "connection closed".into(),
            })
    }

    /// 关闭连接（发送 Close frame）
    pub fn close(&self, code: u16, reason: &str) -> Result<(), CatcherError> {
        self.cmd_tx
            .send(WsCommand::Close { code, reason: reason.to_string() })
            .map_err(|_| CatcherError::WsDisconnected {
                code: 1006, reason: "connection closed".into(),
            })
    }
}
```

---

## TLS (`src/transport/tls.rs`)

```rust
use reqwest::ClientBuilder;
use crate::config::TlsConfig;
use crate::error::CatcherError;

pub fn build_tls_config(
    builder: &mut ClientBuilder,
    config: &TlsConfig,
) -> Result<(), CatcherError> {
    if !config.reject_unauthorized {
        builder.danger_accept_invalid_certs(true);
    }

    if let Some(ref ca_path) = config.ca_cert_path {
        let ca_bytes = std::fs::read(ca_path)
            .map_err(|e| CatcherError::InvalidConfig(format!("read CA: {e}")))?;
        let cert = reqwest::Certificate::from_pem(&ca_bytes)
            .map_err(|e| CatcherError::InvalidConfig(format!("parse CA: {e}")))?;
        builder.add_root_certificate(cert);
    }

    Ok(())
}
```

---

## DNS (`src/transport/dns.rs`)

```rust
#[cfg(feature = "hickory-dns")]
use hickory_resolver::TokioAsyncResolver;
use std::time::Duration;
use crate::config::DnsConfig;
use crate::error::CatcherError;

// StaleAwareDnsResolver wraps hickory-resolver with moka async cache.
// Always injected into reqwest via dns_resolver() — caching is enabled by default.
pub fn build_stale_aware_resolver(config: &DnsConfig) -> Result<Arc<StaleAwareDnsResolver>, CatcherError>
```

## 代理与 DNS 的配合

HTTP 和 WebSocket 使用同一套网络配置：`proxy`、`dns`、`tls`
的语义必须一致。网络环境切换后的主动恢复由 `networkChanged()` API 承担
（清 DNS 缓存、热重建连接池、重置熔断器），见 `docs/user-manual/resilience.md`。

当调用方传入 `proxy.url = "socks5://..."` 时，Catcher 内部必须按
`socks5h://...` 交给 reqwest。原因是：

1. `socks5://` 会让本地先解析目标域名。
2. Clash fake-ip、VPN 分流、按域名规则转发时，目标域名不能提前变成 IP。
3. 代理路径中，业务目标域名应交给代理解析；Catcher DNS 仍可用于非代理路径、
   host mapping、缓存和自定义 nameserver。

因此，以下行为是传输层契约：

| 场景 | 期望行为 |
|------|----------|
| `dns` 未配置 | 不注入 Catcher DNS，使用 reqwest 默认解析路径 |
| `dns: { cache_ttl_secs: 300 }` | 默认仍是 Catcher DNS |
| `dns.mode = "native"` | 显式使用 reqwest 原生解析路径 |
| `proxy.url = "socks5://..."` | 内部按 `socks5h://...` 使用，让代理解析目标域名 |
| `proxy.url = "socks5h://..."` | 原样使用 |
| 代理和 Catcher DNS 同时配置 | 代理连接目标不能被 Catcher DNS 提前解析成 IP |

该契约必须由自动化测试锁住：

- `catcher-test-support` 提供假的 SOCKS5 代理，记录客户端发来的 CONNECT 目标。
- `catcher-http/tests/proxy_dns_behavior_test.rs` 验证 HTTP 在启用 Catcher DNS
  时，`socks5://` 仍把 `example.com` 交给代理；HTTPS 走 HTTP proxy
  时，CONNECT 目标仍是 `example.com:443`；命中 `no_proxy` 时不连接代理。
- `catcher-ws/tests/proxy_dns_behavior_test.rs` 验证 WebSocket 同样遵守该行为。
- `catcher-http/tests/local_proxy_test.rs` 和 `catcher-ws/tests/local_proxy_test.rs`
  保留为 `#[ignore]`，用于发版前手动连接真实 Clash 或本地代理。

> ⚠️ **对 reqwest 内部行为的隐式依赖（issue #031）**：上述「代理路径目标域名不被本地
> 解析」的保证，建立在 reqwest「走代理时不调用自定义 `dns_resolver`」这一**实现细节**之上，
> 而非其稳定 API 的明文承诺。代码中没有、也无法（resolver 仍需服务 no_proxy/直连 host）
> 显式禁用代理路径上的本地解析。`proxy_dns_behavior_test` 是唯一的回归护栏，随
> `cargo test --workspace`（catcher-http 默认启用 `hickory-dns`，catcher-ws 始终启用
> `reqwest-resolver`）在 CI 运行。**升级 reqwest 版本时，必须确认这两个测试通过**，否则
> 目标域名可能悄悄退回本地解析、以 IP 泄漏给代理，破坏 Clash 域名分流。

---

## 待实现：Per-request Cancel（N-03）

> 设计详见 [../issues/native-layer-capability-gaps.md](../issues/native-layer-capability-gaps.md) N-03

当前 `cancel_token` 是全局单一实例，`cancel_all()` 取消全部飞行请求。需扩展为支持单请求级取消。

### 目标架构

```rust
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use tokio_util::sync::CancellationToken;

pub struct HttpTransport {
    // ...existing fields...
    cancel_token: Arc<Mutex<CancellationToken>>,          // 全局 cancel（保留）
    pending_requests: Mutex<HashMap<u64, CancellationToken>>, // per-request
    next_request_id: AtomicU64,
}

impl HttpTransport {
    /// 执行请求并返回 request_id。
    /// select! 同时监听 per-request token 和 全局 token。
    pub async fn execute(
        &self, request: HttpRequest,
    ) -> (u64, Result<HttpResponse, CatcherError>) { ... }

    /// 取消单个飞行请求
    pub fn cancel_request(&self, request_id: u64) -> bool { ... }

    /// 取消全部（全局 + per-request 双路 cancel）
    pub fn cancel_all(&self) { ... }
}
```

### Ffi 影响

`catcher_http_execute` 返回值从 `void` 变为 `u64`（request_id）。新增 `catcher_http_cancel_request(handle, request_id) → i32`。

---

## 待实现：流式下载（N-02）

> 设计详见 [../issues/native-layer-capability-gaps.md](../issues/native-layer-capability-gaps.md) N-02

当前 `do_execute()` 第 243 行硬编码 `response.bytes().await`，强制全量读入内存。大文件下载有 OOM 风险。

### 目标架构

```rust
impl HttpTransport {
    /// 流式执行 — 逐 chunk 通过 callback 推送
    pub async fn execute_stream(
        &self,
        request: HttpRequest,
        chunk_callback: impl Fn(StreamEvent) + Send + 'static,
    ) -> Result<HttpResponse, CatcherError> { ... }
}

enum StreamEvent {
    Headers { status: u16, headers: HashMap<String, String> },
    Chunk(Vec<u8>),
    Done,
    Error(String),
}
```

内部使用 `response.bytes_stream()` (来自 `futures_util::StreamExt`) 替代 `response.bytes().await`。

### Ffi 影响

新增 `catcher_http_execute_stream(handle, method, url, body, body_len, content_type, headers_json, timeout_ms, chunk_size_hint, callback, user_data)`。

---

## 待实现：Multipart/FormData（N-01）

> 设计详见 [../issues/native-layer-capability-gaps.md](../issues/native-layer-capability-gaps.md) N-01

### P2 方案（调用方编码）

当前 `catcher_http_post` 已接受 `content_type: FfiString`，调用方可自行编码 multipart 后传入 `multipart/form-data; boundary=...`。Rust 层无需改动。

### P3 方案（Rust 原生 MultipartBuilder）

```rust
// catcher-http/src/multipart/builder.rs (新增模块)
pub struct MultipartBuilder {
    boundary: String,
    parts: Vec<MultipartPart>,
}

impl MultipartBuilder {
    pub fn new() -> Self { ... }
    pub fn add_text(&mut self, name: &str, value: &str) -> &mut Self { ... }
    pub fn add_file(&mut self, name: &str, filename: &str,
                    data: Vec<u8>, content_type: &str) -> &mut Self { ... }
    pub fn build(&self) -> (Vec<u8>, String) { ... }
    // → (encoded_body, "multipart/form-data; boundary=...")
}
```

Ffi 暴露 5 个符号：`catcher_multipart_create` / `add_text` / `add_file` / `build` / `destroy`。
