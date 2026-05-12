# 04 — 传输层

> 对应源文件：`src/transport/`
> 连接池：reqwest 内置基于 `hyper-util` 的连接池，通过 `pool_max_idle_per_host()` / `pool_idle_timeout()` 暴露配置

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
use crate::transport::dns::build_dns_resolver;

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

        if let Some(resolver) = build_dns_resolver(&config.dns)? {
            reqwest_builder = reqwest_builder.dns_resolver(Arc::new(resolver));
        }

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
use stream_tungstenite::prelude::*;
use stream_tungstenite::WebSocketClient;
use tokio::sync::mpsc;
use std::sync::Arc;
use crate::error::CatcherError;
use crate::types::ws::*;

/// WebSocket 传输层
pub struct WsTransport;

impl WsTransport {
    pub async fn connect(
        url: &str,
        config: &WsClientConfig,
    ) -> Result<(WsHandle, mpsc::UnboundedReceiver<WsEvent>), CatcherError> {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let tx_clone = event_tx.clone();
        let url_owned = url.to_string();

        let client = WebSocketClient::builder(&url_owned)
            .receive_timeout(std::time::Duration::from_secs(600))
            .build();

        let mut messages = client.subscribe();
        let client_arc = Arc::new(client);

        let client_bg = client_arc.clone();
        tokio::spawn(async move { client_bg.run().await; });

        let tx_bg = tx_clone.clone();
        tokio::spawn(async move {
            while let Ok(msg) = messages.recv().await {
                let event = match msg.as_ref() {
                    tokio_tungstenite::tungstenite::Message::Text(t) =>
                        WsEvent::Message { data: t.as_bytes().to_vec(), is_binary: false },
                    tokio_tungstenite::tungstenite::Message::Binary(d) =>
                        WsEvent::Message { data: d.clone(), is_binary: true },
                    _ => continue,
                };
                let _ = tx_bg.send(event);
            }
        });

        let sender = client_arc.sender().await;

        Ok((WsHandle { url: url_owned, sender, event_tx: tx_clone }, event_rx))
    }
}

#[derive(Clone)]
pub struct WsHandle {
    url: String,
    sender: Option<stream_tungstenite::Sender>,
    event_tx: mpsc::UnboundedSender<WsEvent>,
}

impl WsHandle {
    pub fn send_text(&self, text: &str) -> Result<(), CatcherError> {
        self.sender.as_ref()
            .ok_or(CatcherError::Internal("sender unavailable".into()))?
            .send_text(text)
            .map_err(|e| CatcherError::Internal(format!("ws send: {e}")))
    }

    pub fn send_binary(&self, data: &[u8]) -> Result<(), CatcherError> {
        self.sender.as_ref()
            .ok_or(CatcherError::Internal("sender unavailable".into()))?
            .send_binary(data.to_vec())
            .map_err(|e| CatcherError::Internal(format!("ws send: {e}")))
    }

    pub fn close(&self) { /* drop triggers close */ }
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

#[cfg(feature = "hickory-dns")]
pub fn build_dns_resolver(
    config: &DnsConfig,
) -> Result<Option<TokioAsyncResolver>, CatcherError> {
    let resolver = TokioAsyncResolver::builder_tokio()
        .cache_size(config.cache_size)
        .positive_ttl(Some(Duration::from_secs(config.positive_ttl_secs)))
        .negative_ttl(Some(Duration::from_secs(config.negative_ttl_secs)))
        .build()
        .map_err(|e| CatcherError::InvalidConfig(format!("DNS: {e}")))?;
    Ok(Some(resolver))
}

#[cfg(not(feature = "hickory-dns"))]
pub fn build_dns_resolver(_: &DnsConfig) -> Result<Option<()>, CatcherError> {
    Ok(None) // fallback to system DNS
}
```
