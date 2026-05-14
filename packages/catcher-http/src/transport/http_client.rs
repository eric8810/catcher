use reqwest::Client;
use reqwest_middleware::{ClientBuilder as MiddlewareBuilder, ClientWithMiddleware};
use reqwest_retry::RetryTransientMiddleware;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use catcher_core::CatcherError;
use crate::resilience::backoff::build_retry_policy;
use crate::resilience::circuit_breaker::CircuitBreaker;
use crate::transport::dns::build_dns_resolver;
use crate::transport::tls::build_tls_config;
use crate::types::http::*;
use catcher_core::types::resilience::CbState;

/// HTTP 传输层 — 真实收发 HTTP 请求，带重试中间件 + 熔断器
///
/// Phase 3: 使用 reqwest-middleware + RetryTransientMiddleware + CircuitBreaker
pub struct HttpTransport {
    client: ClientWithMiddleware,
    config: HttpClientConfig,
    circuit_breaker: Option<CircuitBreaker>,
}

impl HttpTransport {
    /// 根据 HttpClientConfig 构建 HttpTransport
    pub fn new(config: HttpClientConfig) -> Result<Self, CatcherError> {
        let mut reqwest_builder = Client::builder()
            .connect_timeout(Duration::from_millis(config.connect_timeout_ms))
            .pool_max_idle_per_host(config.pool.max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(config.pool.idle_timeout_secs))
            .tcp_keepalive(
                config
                    .pool
                    .keep_alive
                    .then(|| Duration::from_secs(config.pool.keep_alive_interval_secs)),
            );

        // G8: TLS configuration
        reqwest_builder = build_tls_config(reqwest_builder, &config.tls)?;

        // G7: DNS resolution: build_dns_resolver validates config but custom
        // nameservers are not yet wired into reqwest (requires hickory-dns
        // feature integration). System DNS is used as fallback.
        if let Some(ref dns) = config.dns {
            build_dns_resolver(dns)?;
        }

        // G4: Proxy configuration
        if let Some(ref proxy_config) = config.proxy {
            let mut proxy = reqwest::Proxy::all(&proxy_config.url)
                .map_err(|e| CatcherError::InvalidConfig(format!("invalid proxy URL: {e}")))?;
            if let Some(ref auth) = proxy_config.auth {
                proxy = proxy.basic_auth(&auth.username, &auth.password);
            }
            // G4: noProxy — exclude matching hostnames from proxying
            if !proxy_config.no_proxy.is_empty() {
                let no_proxy_str = proxy_config.no_proxy.join(",");
                let no_proxy = reqwest::NoProxy::from_string(&no_proxy_str);
                proxy = proxy.no_proxy(no_proxy);
            }
            reqwest_builder = reqwest_builder.proxy(proxy);
        }

        // G6: Redirect policy
        reqwest_builder = match &config.redirect {
            Some(r) if !r.follow => reqwest_builder.redirect(reqwest::redirect::Policy::none()),
            Some(r) => reqwest_builder.redirect(reqwest::redirect::Policy::limited(r.max_redirects as usize)),
            None => reqwest_builder,
        };

        // G12: Auth — set default headers for basic auth and bearer token
        if let Some(ref auth) = config.auth {
            let encoded = base64_encode(&format!("{}:{}", auth.username, auth.password));
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "Authorization",
                format!("Basic {}", encoded).parse()
                    .map_err(|e| CatcherError::InvalidConfig(format!("invalid basic auth header: {e}")))?,
            );
            reqwest_builder = reqwest_builder.default_headers(headers);
        }
        if let Some(ref token) = config.bearer_token {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                "Authorization",
                format!("Bearer {}", token).parse()
                    .map_err(|e| CatcherError::InvalidConfig(format!("invalid bearer token header: {e}")))?,
            );
            reqwest_builder = reqwest_builder.default_headers(headers);
        }

        let reqwest_client = reqwest_builder
            .build()
            .map_err(|e| CatcherError::Internal(format!("reqwest build error: {e}")))?;

        // Phase 3: Wrap with middleware (retry)
        let mut client_builder = MiddlewareBuilder::new(reqwest_client);
        if let Some(ref retry) = config.retry {
            let policy = build_retry_policy(retry);
            client_builder = client_builder.with(RetryTransientMiddleware::new_with_policy(policy));
        }

        let circuit_breaker = config
            .circuit_breaker
            .as_ref()
            .map(|cb| CircuitBreaker::new(cb.clone()));

        Ok(Self {
            client: client_builder.build(),
            config,
            circuit_breaker,
        })
    }

    /// 发起 HTTP 请求（带熔断器检查）
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
        if let Some(ref cb) = self.circuit_breaker {
            cb.before_request()?;
        }

        let result = self.do_execute(request).await;

        match &result {
            Ok(_) => {
                if let Some(ref cb) = self.circuit_breaker {
                    cb.on_success();
                }
            }
            Err(_) => {
                if let Some(ref cb) = self.circuit_breaker {
                    cb.on_failure();
                }
            }
        }

        result
    }

    async fn do_execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
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
            format!(
                "{}{}",
                self.config.base_url.trim_end_matches('/'),
                request.url
            )
        };

        let mut req = self.client.request(method, &url);
        // Apply default headers from config (per-request headers override)
        for (k, v) in &self.config.default_headers {
            req = req.header(k, v);
        }
        for (k, v) in &request.headers {
            req = req.header(k, v);
        }
        if let Some(body) = &request.body {
            req = req.body(body.clone());
        }
        if let Some(content_type) = &request.content_type {
            req = req.header("Content-Type", content_type);
        }

        let timeout_ms = request
            .timeout_ms
            .unwrap_or(self.config.response_timeout_ms);
        req = req.timeout(Duration::from_millis(timeout_ms));

        let response = req
            .send()
            .await
            .map_err(|e| map_middleware_error(e, &self.config))?;

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes()
            .await
            .map_err(|e| CatcherError::Internal(format!("read body: {e}")))?;

        let elapsed_ms = start.elapsed().as_millis() as u64;

        if status >= 400 {
            return Err(CatcherError::HttpError {
                status,
                body: String::from_utf8_lossy(&body).to_string(),
            });
        }

        Ok(HttpResponse {
            status,
            headers,
            body: body.to_vec(),
            elapsed_ms,
        })
    }

    /// GET 快捷方法
    pub async fn get(&self, url: &str) -> Result<HttpResponse, CatcherError> {
        let request = HttpRequest {
            method: HttpMethod::GET,
            url: url.to_string(),
            headers: HashMap::new(),
            body: None,
            content_type: None,
            timeout_ms: None,
        };
        self.execute(request).await
    }

    /// POST 快捷方法
    pub async fn post(
        &self,
        url: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<HttpResponse, CatcherError> {
        let request = HttpRequest {
            method: HttpMethod::POST,
            url: url.to_string(),
            headers: HashMap::new(),
            body: Some(body.to_vec()),
            content_type: Some(content_type.to_string()),
            timeout_ms: None,
        };
        self.execute(request).await
    }

    /// 返回熔断器状态（用于 metrics）
    pub fn circuit_breaker_state(&self) -> Option<CbState> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }
}

fn map_middleware_error(e: reqwest_middleware::Error, config: &HttpClientConfig) -> CatcherError {
    let msg = format!("{e}");
    if msg.contains("timeout") || msg.contains("timed out") {
        return CatcherError::RequestTimeout(config.response_timeout_ms);
    }
    if msg.contains("connect") || msg.contains("connection") {
        return CatcherError::ConnectionTimeout(config.connect_timeout_ms);
    }
    CatcherError::Internal(format!("request: {e}"))
}

/// Simple base64 encoding for Basic auth (no external dependency needed)
fn base64_encode(input: &str) -> String {
    use std::fmt::Write;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let mut n = 0u32;
        for (i, &byte) in chunk.iter().enumerate() {
            n |= (byte as u32) << (16 - i * 8);
        }
        for i in 0..4 {
            if i <= chunk.len() {
                let idx = ((n >> (18 - i * 6)) & 0x3F) as usize;
                result.write_char(CHARSET[idx] as char).unwrap();
            } else {
                result.write_char('=').unwrap();
            }
        }
    }
    result
}
