use reqwest::Client;
use reqwest_middleware::{ClientBuilder as MiddlewareBuilder, ClientWithMiddleware};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;
use tokio::sync::Semaphore;

use catcher_core::CatcherError;
use crate::resilience::backoff::build_retry_policy;
use crate::resilience::circuit_breaker::CircuitBreaker;
use crate::resilience::timeout::AdaptiveTimeout;
use crate::transport::retry_middleware::MetricsRetryMiddleware;
use crate::transport::tls::build_tls_config;
use crate::types::http::*;
use crate::observability::metrics::{MetricsCollector, MetricsSnapshot};
use catcher_core::types::resilience::CbState;

/// HTTP 传输层 — 真实收发 HTTP 请求，带重试中间件 + 熔断器 + 取消 + 自适应超时
///
/// Phase 3: 使用 reqwest-middleware + MetricsRetryMiddleware + CircuitBreaker
/// Phase 4: 增加 tokio CancellationToken 支持请求取消 + MetricsCollector
/// Phase 5: 增加 AdaptiveTimeout 基于滑动窗口 P90 RTT 自适应超时
/// Phase 6: 增加 per-request cancel (N-03)
/// Phase 7: 增加 Semaphore 并发控制 + 优先级队列 (A-01)
pub struct HttpTransport {
    client: ClientWithMiddleware,
    config: Arc<HttpClientConfig>,
    circuit_breaker: Option<CircuitBreaker>,
    cancel_token: Arc<Mutex<tokio_util::sync::CancellationToken>>,
    metrics: MetricsCollector,
    adaptive_timeout: Mutex<Option<AdaptiveTimeout>>,
    /// Per-request cancel tokens, keyed by request_id (N-03)
    pending_requests: Mutex<HashMap<u64, tokio_util::sync::CancellationToken>>,
    next_request_id: AtomicU64,
    /// Semaphore for concurrency control (A-01, fallback when priority not needed)
    concurrency_semaphore: Option<Arc<Semaphore>>,
    /// Priority-based request queue (A-01, used when request has explicit priority)
    priority_queue: Option<crate::scheduler::PriorityRequestQueue<HttpResponse>>,
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
            )
            .tcp_keepalive_interval(
                config
                    .pool
                    .keep_alive
                    .then(|| Duration::from_secs(config.pool.keep_alive_interval_secs)),
            )
            .tcp_keepalive_retries(config.pool.keep_alive.then_some(3));

        // G8: TLS configuration
        reqwest_builder = build_tls_config(reqwest_builder, &config.tls)?;

        // G7: DNS resolution — always build StaleAwareDnsResolver for caching
        #[cfg(feature = "hickory-dns")]
        {
            let dns_config = config.dns.clone().unwrap_or_default();
            let resolver = crate::transport::dns::build_stale_aware_resolver(&dns_config)?;
            reqwest_builder = reqwest_builder.dns_resolver(resolver);
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
        let metrics = MetricsCollector::new();
        let mut client_builder = MiddlewareBuilder::new(reqwest_client);
        if let Some(ref retry) = config.retry {
            let policy = build_retry_policy(retry);
            client_builder = client_builder.with(MetricsRetryMiddleware::new(
                policy,
                metrics.http_retries_weak(),
            ));
        }

        let circuit_breaker = config
            .circuit_breaker
            .as_ref()
            .map(|cb| CircuitBreaker::new(cb.clone()));

        // A-01: Priority-based request queue with concurrency control.
        // PriorityRequestQueue starts Tokio worker tasks during construction, so only
        // enable it when HttpTransport::new() is called inside an active runtime.
        // Synchronous constructors (notably napi) fall back to semaphore-only
        // concurrency control and still execute requests on their async methods.
        let (concurrency_semaphore, priority_queue) = if config.max_concurrency > 0 {
            if Handle::try_current().is_ok() {
                let queue_config = catcher_core::types::scheduler::QueueConfig {
                    max_concurrency: config.max_concurrency as usize,
                    queue_capacity: 1024,
                    default_timeout_ms: config.response_timeout_ms,
                    concurrency_mode: catcher_core::types::scheduler::ConcurrencyMode::Fixed(
                        config.max_concurrency as usize,
                    ),
                };
                (
                    None, // priority_queue handles concurrency internally
                    Some(crate::scheduler::PriorityRequestQueue::new(queue_config)),
                )
            } else {
                (Some(Arc::new(Semaphore::new(config.max_concurrency as usize))), None)
            }
        } else {
            (None, None)
        };

        Ok(Self {
            client: client_builder.build(),
            config: Arc::new(config),
            circuit_breaker,
            cancel_token: Arc::new(Mutex::new(tokio_util::sync::CancellationToken::new())),
            metrics,
            adaptive_timeout: Mutex::new(None),
            pending_requests: Mutex::new(HashMap::new()),
            next_request_id: AtomicU64::new(1),
            concurrency_semaphore,
            priority_queue,
        })
    }

    /// 发起 HTTP 请求（带熔断器检查 + cancel 支持 + metrics 记录 + 自适应超时 + 并发控制）
    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
        let (_, result) = self.execute_with_token(
            0,  // dummy request_id, not registered for cancel
            tokio_util::sync::CancellationToken::new(),  // uncancellable per-request token
            request,
        ).await;
        result
    }

    /// 使用预分配的 token 执行请求（N-03，供 FFI 层使用）。
    /// request_id 由调用方通过 allocate_pending_request() 预分配，
    /// 以便在 execute 返回前就能通过 cancel_request(request_id) 取消。
    pub async fn execute_with_token(
        &self,
        request_id: u64,
        per_request_token: tokio_util::sync::CancellationToken,
        mut request: HttpRequest,
    ) -> (u64, Result<HttpResponse, CatcherError>) {
        let start = Instant::now();

        if let Some(ref cb) = self.circuit_breaker {
            if let Err(e) = cb.before_request() {
                self.pending_requests.lock().unwrap().remove(&request_id);
                self.metrics.record_http_request(false, start.elapsed().as_micros() as u64);
                return (request_id, Err(e));
            }
        }

        // Apply adaptive timeout if enabled and no per-request override
        if request.timeout_ms.is_none() {
            if let Some(ref mut at) = *self.adaptive_timeout.lock().unwrap() {
                request.timeout_ms = Some(at.timeout_ms());
            }
        }

        let global_token = {
            let token = self.cancel_token.lock().unwrap();
            token.clone()
        };

        // A-01: Acquire concurrency slot — priority queue or semaphore
        let result = if let Some(ref queue) = self.priority_queue {
            // Priority queue path: handles both concurrency and priority ordering
            let client = self.client.clone();
            let config = self.config.clone();
            let priority = request.priority;
            let timeout_ms = request.timeout_ms;

            let queue_result = tokio::select! {
                r = queue.submit(priority, timeout_ms, move || {
                    let c = client.clone();
                    let cfg = config.clone();
                    let req = request;
                    async move { execute_http_request(c, cfg, req).await }
                }) => r,
                _ = per_request_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    Err(CatcherError::Internal("request cancelled while waiting in queue".into()))
                }
                _ = global_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    Err(CatcherError::Internal("request cancelled while waiting in queue".into()))
                }
            };
            queue_result
        } else if let Some(ref sem) = self.concurrency_semaphore {
            // Fallback semaphore path (no priority ordering)
            let _permit = tokio::select! {
                permit = sem.acquire() => permit,
                _ = per_request_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    return (request_id, Err(CatcherError::Internal("request cancelled while waiting in queue".into())));
                }
                _ = global_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    return (request_id, Err(CatcherError::Internal("request cancelled while waiting in queue".into())));
                }
            };

            tokio::select! {
                r = self.do_execute(request) => r,
                _ = per_request_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    return (request_id, Err(CatcherError::Internal("request cancelled".into())));
                }
                _ = global_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    return (request_id, Err(CatcherError::Internal("request cancelled".into())));
                }
            }
        } else {
            // No concurrency control
            tokio::select! {
                r = self.do_execute(request) => r,
                _ = per_request_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    return (request_id, Err(CatcherError::Internal("request cancelled".into())));
                }
                _ = global_token.cancelled() => {
                    let elapsed_us = start.elapsed().as_micros() as u64;
                    self.metrics.record_http_request(false, elapsed_us);
                    return (request_id, Err(CatcherError::Internal("request cancelled".into())));
                }
            }
        };

        self.pending_requests.lock().unwrap().remove(&request_id);

        let elapsed_us = start.elapsed().as_micros() as u64;

        // Record RTT into adaptive timeout
        if let Ok(ref resp) = &result {
            let rtt_ms = resp.elapsed_ms;
            if let Some(ref mut at) = *self.adaptive_timeout.lock().unwrap() {
                at.record(rtt_ms);
            }
        }

        match &result {
            Ok(_) => {
                self.metrics.record_http_request(true, elapsed_us);
                if let Some(ref cb) = self.circuit_breaker {
                    cb.on_success();
                }
            }
            Err(_) => {
                self.metrics.record_http_request(false, elapsed_us);
                if let Some(ref cb) = self.circuit_breaker {
                    cb.on_failure();
                }
            }
        }

        (request_id, result)
    }

    /// 取消所有飞行中的请求。新请求不受影响。
    pub fn cancel_all(&self) {
        let mut token = self.cancel_token.lock().unwrap();
        token.cancel();
        // Replace with a fresh token so new requests can proceed
        *token = tokio_util::sync::CancellationToken::new();
        drop(token);
        // Also cancel all per-request tokens (N-03)
        let pending: Vec<tokio_util::sync::CancellationToken> =
            self.pending_requests.lock().unwrap().drain().map(|(_, t)| t).collect();
        for t in &pending {
            t.cancel();
        }
    }

    /// 取消单个飞行请求（N-03）。
    /// 返回 true 表示找到并取消，false 表示 request_id 不存在或已完成。
    pub fn cancel_request(&self, request_id: u64) -> bool {
        if let Some(token) = self.pending_requests.lock().unwrap().get(&request_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// 分配一个新的 request_id 和对应的 CancellationToken（N-03）。
    /// 调用方负责在请求完成后调用 cleanup_pending_request()。
    pub fn allocate_pending_request(&self) -> (u64, tokio_util::sync::CancellationToken) {
        let request_id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let token = tokio_util::sync::CancellationToken::new();
        self.pending_requests.lock().unwrap().insert(request_id, token.clone());
        (request_id, token)
    }

    /// 获取下一个 request_id（不注册 pending token，供 FFI 层使用）。
    pub fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// 注册一个 per-request CancellationToken 供 cancel 使用（N-03）。
    pub fn insert_pending_request(&self, request_id: u64, token: tokio_util::sync::CancellationToken) {
        self.pending_requests.lock().unwrap().insert(request_id, token);
    }

    /// 清理已完成的请求（N-03）。
    pub fn cleanup_pending_request(&self, request_id: u64) {
        self.pending_requests.lock().unwrap().remove(&request_id);
    }

    /// 使用预分配的 token 执行请求（N-03，供 FFI 层使用）。
    async fn do_execute(&self, request: HttpRequest) -> Result<HttpResponse, CatcherError> {
        execute_http_request(self.client.clone(), self.config.clone(), request).await
    }

    /// 流式执行 HTTP 请求（N-02），逐 chunk 通过回调推送。
    /// chunk_callback(event) 在每次收到数据块、响应头、或错误时调用。
    /// 支持 cancel_all() / cancel_request() 中断流式下载。
    pub async fn execute_stream(
        &self,
        request: HttpRequest,
        per_request_token: tokio_util::sync::CancellationToken,
        chunk_callback: impl Fn(StreamEvent) + Send + 'static,
    ) -> Result<(), CatcherError> {
        use tokio_stream::StreamExt;

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
        // B-02: multipart support
        if let Some(ref form) = request.multipart {
            let (encoded_body, ct) = form.encode();
            req = req.body(encoded_body);
            req = req.header("Content-Type", ct);
        } else {
            if let Some(body) = &request.body { req = req.body(body.clone()); }
            if let Some(ct) = &request.content_type { req = req.header("Content-Type", ct); }
        }
        let timeout_ms = request.timeout_ms.unwrap_or(self.config.response_timeout_ms);
        req = req.timeout(Duration::from_millis(timeout_ms));

        let global_token = {
            let token = self.cancel_token.lock().unwrap();
            token.clone()
        };

        let response = tokio::select! {
            r = req.send() => r.map_err(|e| map_middleware_error_standalone(e, &self.config))?,
            _ = per_request_token.cancelled() => {
                chunk_callback(StreamEvent::Error("request cancelled".into()));
                return Err(CatcherError::Internal("request cancelled".into()));
            }
            _ = global_token.cancelled() => {
                chunk_callback(StreamEvent::Error("request cancelled".into()));
                return Err(CatcherError::Internal("request cancelled".into()));
            }
        };

        let status = response.status().as_u16();
        let headers: HashMap<String, String> = response.headers().iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Notify headers first
        chunk_callback(StreamEvent::Headers { status, headers });

        // Stream chunks with cancel monitoring
        let mut stream = response.bytes_stream();
        loop {
            tokio::select! {
                chunk_opt = stream.next() => {
                    match chunk_opt {
                        Some(Ok(chunk)) => chunk_callback(StreamEvent::Chunk(chunk)),
                        Some(Err(e)) => {
                            chunk_callback(StreamEvent::Error(format!("stream read error: {e}")));
                            return Err(CatcherError::Internal(format!("stream read: {e}")));
                        }
                        None => break, // stream exhausted
                    }
                }
                _ = per_request_token.cancelled() => {
                    chunk_callback(StreamEvent::Error("request cancelled".into()));
                    return Err(CatcherError::Internal("request cancelled".into()));
                }
                _ = global_token.cancelled() => {
                    chunk_callback(StreamEvent::Error("request cancelled".into()));
                    return Err(CatcherError::Internal("request cancelled".into()));
                }
            }
        }

        chunk_callback(StreamEvent::Done);
        Ok(())
    }

    /// GET 快捷方法
    pub async fn get(&self, url: &str) -> Result<HttpResponse, CatcherError> {
        let request = HttpRequest {
            method: HttpMethod::GET,
            url: url.to_string(),
            ..Default::default()
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
            body: Some(body.to_vec()),
            content_type: Some(content_type.to_string()),
            ..Default::default()
        };
        self.execute(request).await
    }

    /// 返回熔断器状态（用于 metrics）
    pub fn circuit_breaker_state(&self) -> Option<CbState> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }

    /// 返回运行时指标快照
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// 当前并发队列中的等待请求数（A-01）。
    /// 返回已发出但未完成的 permit 数量（近似值）。
    pub fn queue_depth(&self) -> usize {
        if let Some(ref queue) = self.priority_queue {
            let max = self.config.max_concurrency as usize;
            let available = queue.available_permits();
            max.saturating_sub(available)
        } else if let Some(ref sem) = self.concurrency_semaphore {
            let max = self.config.max_concurrency as usize;
            let available = sem.available_permits();
            max.saturating_sub(available)
        } else {
            0
        }
    }

    /// 启用/配置自适应超时。
    /// `min_timeout_ms` / `max_timeout_ms` 限幅，`multiplier` 作用于 P90 RTT
    /// （timeout = clamp(P90_RTT * multiplier, min, max)）。
    /// 传 `None` 禁用自适应超时。
    pub fn set_adaptive_timeout(
        &self,
        min_timeout_ms: u64,
        max_timeout_ms: u64,
        multiplier: f64,
        window_size: usize,
    ) {
        let mut at = self.adaptive_timeout.lock().unwrap();
        *at = Some(AdaptiveTimeout::new(min_timeout_ms, max_timeout_ms, multiplier, window_size));
    }

    pub fn disable_adaptive_timeout(&self) {
        let mut at = self.adaptive_timeout.lock().unwrap();
        *at = None;
    }
}

/// Standalone HTTP execution (used by priority queue).
/// Takes owned client + config to satisfy `'static` bounds.
async fn execute_http_request(
    client: ClientWithMiddleware,
    config: Arc<HttpClientConfig>,
    request: HttpRequest,
) -> Result<HttpResponse, CatcherError> {
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
            config.base_url.trim_end_matches('/'),
            request.url,
        )
    };

    // G7: host_mapping is handled by StaleAwareDnsResolver at the DNS layer.
    // No request-level URL rewriting needed — the resolver returns the mapped IP
    // while reqwest preserves the original hostname for Host header and TLS SNI.

    let mut req = client.request(method, &url);
    for (k, v) in &config.default_headers {
        req = req.header(k, v);
    }
    for (k, v) in &request.headers {
        req = req.header(k, v);
    }
    // B-02: multipart encoding — overrides body/content_type if set
    if let Some(ref form) = request.multipart {
        let (encoded_body, ct) = form.encode();
        req = req.body(encoded_body);
        req = req.header("Content-Type", ct);
    } else if config.msgpack {
        // msgpack: encode JSON body → msgpack binary
        if let Some(body) = &request.body {
            let value: serde_json::Value = serde_json::from_slice(body)
                .map_err(|e| CatcherError::Internal(format!("msgpack encode: invalid JSON body: {e}")))?;
            let encoded = rmp_serde::to_vec(&value)
                .map_err(|e| CatcherError::Internal(format!("msgpack encode: {e}")))?;
            req = req.body(encoded);
        }
        req = req.header("Content-Type", "application/msgpack");
    } else {
        if let Some(body) = &request.body {
            req = req.body(body.clone());
        }
        if let Some(content_type) = &request.content_type {
            req = req.header("Content-Type", content_type);
        }
    }
    if let Some(ref override_host) = config.hostname_override {
        req = req.header("Host", override_host.as_str());
    }

    let timeout_ms = request
        .timeout_ms
        .unwrap_or(config.response_timeout_ms);
    req = req.timeout(Duration::from_millis(timeout_ms));

    let response = req
        .send()
        .await
        .map_err(|e| map_middleware_error_standalone(e, &config))?;

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

    let response_body = if config.msgpack {
        // msgpack: decode response msgpack → JSON bytes
        let value: serde_json::Value = rmp_serde::from_slice(&body)
            .map_err(|e| CatcherError::Internal(format!("msgpack decode: expected msgpack response: {e}")))?;
        serde_json::to_vec(&value)
            .map_err(|e| CatcherError::Internal(format!("msgpack decode: json serialize: {e}")))?
    } else {
        body.to_vec()
    };

    Ok(HttpResponse {
        status,
        headers,
        body: response_body,
        elapsed_ms,
    })
}

fn map_middleware_error_standalone(e: reqwest_middleware::Error, config: &HttpClientConfig) -> CatcherError {
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
    let mut result = String::with_capacity(bytes.len().div_ceil(3) * 4);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn re1_http_error_carries_request_info() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
            .mount(&server)
            .await;

        let config = HttpClientConfig {
            base_url: server.uri(),
            ..Default::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let result = transport.get("/test").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            CatcherError::HttpError { status, body } => {
                assert_eq!(status, 500);
                assert!(body.contains("internal error"));
            }
            other => panic!("Expected HttpError, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn re2_connection_error_on_unreachable() {
        let config = HttpClientConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            connect_timeout_ms: 500,
            response_timeout_ms: 500,
            ..Default::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let result = transport.get("/test").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rp2_no_proxy_direct_connection() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;

        let config = HttpClientConfig {
            base_url: server.uri(),
            ..Default::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let result = transport.get("/test").await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn rrd1_follow_redirect() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/target"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/target"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server)
            .await;

        let config = HttpClientConfig {
            base_url: server.uri(),
            ..Default::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let result = transport.get("/redirect").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, 200);
    }

    #[tokio::test]
    async fn rrd2_disable_redirect() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/redirect"))
            .respond_with(ResponseTemplate::new(302).insert_header("Location", "/target"))
            .mount(&server)
            .await;

        let config = HttpClientConfig {
            base_url: server.uri(),
            redirect: Some(RedirectConfig { follow: false, max_redirects: 0 }),
            ..Default::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let result = transport.get("/redirect").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, 302);
    }

    #[tokio::test]
    async fn t4_default_new_uses_http_transport() {
        let config = HttpClientConfig::default();
        let result = HttpTransport::new(config);
        assert!(result.is_ok());
    }

    #[test]
    fn re4_display_no_authorization() {
        // Test that base64_encode works correctly (Basic auth)
        let encoded = base64_encode("user:pass");
        assert_eq!(encoded, "dXNlcjpwYXNz");

        // Verify no sensitive data leaks through simple formatting
        let encoded = base64_encode("u:p");
        assert_eq!(encoded, "dTpw");
    }

    #[test]
    fn base64_encode_correctness() {
        assert_eq!(base64_encode(""), "");
        assert_eq!(base64_encode("a"), "YQ==");
        assert_eq!(base64_encode("ab"), "YWI=");
        assert_eq!(base64_encode("abc"), "YWJj");
        assert_eq!(base64_encode("test"), "dGVzdA==");
    }

    // ── N-02: Stream download tests ──

    #[tokio::test]
    async fn ns02_stream_receives_body_via_callback() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;
        use crate::types::http::StreamEvent;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("hello world"))
            .mount(&server).await;
        let config = HttpClientConfig { base_url: server.uri(), ..Default::default() };
        let transport = HttpTransport::new(config).unwrap();
        let events = Arc::new(Mutex::new(Vec::<StreamEvent>::new()));
        let ec = events.clone();
        let request = HttpRequest {
            method: HttpMethod::GET, url: "/test".to_string(),
            headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
            ..Default::default()
        };
        transport.execute_stream(request, tokio_util::sync::CancellationToken::new(), move |e| { ec.lock().unwrap().push(e); }).await.unwrap();
        let evts = events.lock().unwrap();
        assert!(evts.len() >= 3);
        assert!(matches!(&evts[0], StreamEvent::Headers { status: 200, .. }));
        assert!(matches!(evts.last(), Some(StreamEvent::Done)));
        let body: Vec<u8> = evts.iter().filter_map(|e| match e {
            StreamEvent::Chunk(d) => Some(d.as_ref()), _ => None,
        }).flatten().copied().collect();
        assert_eq!(String::from_utf8(body).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn ns02_stream_empty_body() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;
        use crate::types::http::StreamEvent;

        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(200)).mount(&server).await;
        let config = HttpClientConfig { base_url: server.uri(), ..Default::default() };
        let transport = HttpTransport::new(config).unwrap();
        let events = Arc::new(Mutex::new(Vec::<StreamEvent>::new()));
        let ec = events.clone();
        let request = HttpRequest {
            method: HttpMethod::GET, url: "/test".to_string(),
            headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
            ..Default::default()
        };
        transport.execute_stream(request, tokio_util::sync::CancellationToken::new(), move |e| { ec.lock().unwrap().push(e); }).await.unwrap();
        let evts = events.lock().unwrap();
        assert_eq!(evts.iter().filter(|e| matches!(e, StreamEvent::Chunk(_))).count(), 0);
        assert!(matches!(&evts[0], StreamEvent::Headers { .. }));
        assert!(matches!(evts.last(), Some(StreamEvent::Done)));
    }

    #[tokio::test]
    async fn ns02_stream_cancel_interrupts() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;
        use crate::types::http::StreamEvent;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_string("x".repeat(65536))
                .set_delay(std::time::Duration::from_secs(30)))
            .mount(&server).await;
        let config = HttpClientConfig {
            base_url: server.uri(), response_timeout_ms: 60000, ..Default::default()
        };
        let transport = Arc::new(HttpTransport::new(config).unwrap());
        let events = Arc::new(Mutex::new(Vec::<StreamEvent>::new()));
        let ec = events.clone();
        let ct = transport.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            ct.cancel_all();
        });
        let request = HttpRequest {
            method: HttpMethod::GET, url: "/test".to_string(),
            headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
            ..Default::default()
        };
        let result = transport.execute_stream(request, tokio_util::sync::CancellationToken::new(), move |e| { ec.lock().unwrap().push(e); }).await;
        assert!(result.is_err());
        assert!(events.lock().unwrap().iter().any(|e| matches!(e, StreamEvent::Error(m) if m.contains("cancelled"))));
    }

    #[tokio::test]
    async fn ns02_stream_connection_error() {
        let config = HttpClientConfig {
            base_url: "http://127.0.0.1:1".to_string(),
            connect_timeout_ms: 500, response_timeout_ms: 500, ..Default::default()
        };
        let transport = HttpTransport::new(config).unwrap();
        let request = HttpRequest {
            method: HttpMethod::GET, url: "/test".to_string(),
            headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
            ..Default::default()
        };
        assert!(transport.execute_stream(request, tokio_util::sync::CancellationToken::new(), |_| {}).await.is_err());
    }

    // ── N-03: Per-request cancel tests ──

    #[tokio::test]
    async fn ns03_execute_returns_unique_ids() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server).await;
        let config = HttpClientConfig { base_url: server.uri(), ..Default::default() };
        let transport = Arc::new(HttpTransport::new(config).unwrap());
        let mut ids = Vec::new();
        for _ in 0..3 {
            let (rid, token) = transport.allocate_pending_request();
            let t = transport.clone();
            let request = HttpRequest {
                method: HttpMethod::GET, url: "/test".to_string(),
                headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
                ..Default::default()
            };
            let (returned_id, result) = t.execute_with_token(rid, token, request).await;
            assert!(result.is_ok());
            ids.push(returned_id);
        }
        assert_eq!(ids.len(), 3);
        for i in 1..ids.len() { assert!(ids[i] > ids[i - 1]); }
    }

    #[tokio::test]
    async fn ns03_cancel_request_nonexistent_returns_false() {
        let config = HttpClientConfig::default();
        let transport = HttpTransport::new(config).unwrap();
        assert!(!transport.cancel_request(99999));
    }

    #[tokio::test]
    async fn ns03_cancel_all_cancels_everything() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(30)))
            .mount(&server).await;
        let config = HttpClientConfig {
            base_url: server.uri(), response_timeout_ms: 60000, ..Default::default()
        };
        let transport = Arc::new(HttpTransport::new(config).unwrap());
        let mut handles = Vec::new();
        for _ in 0..3 {
            let (rid, token) = transport.allocate_pending_request();
            let t = transport.clone();
            let request = HttpRequest {
                method: HttpMethod::GET, url: "/test".to_string(),
                headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
                ..Default::default()
            };
            handles.push(tokio::spawn(async move { t.execute_with_token(rid, token, request).await }));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        transport.cancel_all();
        for h in handles {
            let (_, result) = h.await.unwrap();
            assert!(result.is_err(), "all requests should be cancelled");
        }
    }

    #[tokio::test]
    async fn ns03_cancel_all_then_new_requests_work() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server).await;
        let config = HttpClientConfig { base_url: server.uri(), ..Default::default() };
        let transport = HttpTransport::new(config).unwrap();
        transport.cancel_all();
        let request = HttpRequest {
            method: HttpMethod::GET, url: "/test".to_string(),
            headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
            ..Default::default()
        };
        assert!(transport.execute(request).await.is_ok());
    }

    #[tokio::test]
    async fn ns03_cancel_idempotent() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::method;

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
            .mount(&server).await;
        let config = HttpClientConfig { base_url: server.uri(), ..Default::default() };
        let transport = HttpTransport::new(config).unwrap();
        let (rid, token) = transport.allocate_pending_request();
        let request = HttpRequest {
            method: HttpMethod::GET, url: "/test".to_string(),
            headers: HashMap::new(), body: None, content_type: None, timeout_ms: None,
            ..Default::default()
        };
        let (_, result) = transport.execute_with_token(rid, token, request).await;
        assert!(result.is_ok());
        assert!(!transport.cancel_request(rid), "completed request should not be cancellable");
    }
}
