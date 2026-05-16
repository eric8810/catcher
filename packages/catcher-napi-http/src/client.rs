//! HTTP client napi bindings — full API
//!
//! Configuration is passed as a JSON string matching `HttpClientConfig`:
//! ```json
//! {
//!   "base_url": "https://api.example.com",
//!   "connect_timeout_ms": 5000,
//!   "response_timeout_ms": 30000,
//!   "pool": { "keep_alive": true, "max_idle_per_host": 10 },
//!   "retry": { "max_attempts": 3, "backoff": "Exponential" },
//!   "circuit_breaker": { "failure_threshold": 5, "reset_timeout_ms": 30000 }
//! }
//! ```

use napi::*;
use napi::bindgen_prelude::Buffer;
use napi::threadsafe_function::{
    ThreadSafeCallContext, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::Arc;

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    HttpTransport, MetricsSnapshot,
};

use catcher_core::types::resilience::CbState;

use crate::helpers::{parse_method, stream_event_to_json, Tsfn};

// ── JavaScript-facing types ──

/// JavaScript-facing HTTP response
#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Buffer,
    pub elapsed_ms: u32,
}

/// Per-request options
#[napi(object)]
pub struct RequestOptions {
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u32>,
    pub content_type: Option<String>,
}

/// JavaScript-facing metrics snapshot
#[napi(object)]
pub struct JsMetrics {
    pub http_requests: i64,
    pub http_success_rate: f64,
    pub http_avg_latency_us: i64,
    pub http_retries: i64,
    pub ws_connect_success_rate: f64,
    pub ws_disconnects: i64,
    pub ws_messages_sent: i64,
    pub ws_messages_received: i64,
    pub cb_open_count: i64,
    pub queue_timeouts: u32,
}

impl From<MetricsSnapshot> for JsMetrics {
    fn from(m: MetricsSnapshot) -> Self {
        Self {
            http_requests: m.http_requests as i64,
            http_success_rate: m.http_success_rate,
            http_avg_latency_us: m.http_avg_latency_us as i64,
            http_retries: m.http_retries as i64,
            ws_connect_success_rate: m.ws_connect_success_rate,
            ws_disconnects: m.ws_disconnects as i64,
            ws_messages_sent: m.ws_messages_sent as i64,
            ws_messages_received: m.ws_messages_received as i64,
            cb_open_count: m.cb_open_count as i64,
            queue_timeouts: m.queue_timeouts,
        }
    }
}

/// JavaScript-facing HTTP client wrapping Rust HttpTransport
#[napi]
pub struct JsHttpClient {
    inner: Arc<HttpTransport>,
}

#[napi]
impl JsHttpClient {
    /// Create an HTTP client from a JSON config string.
    #[napi(constructor)]
    pub fn new(config_json: String) -> napi::Result<Self> {
        let config: HttpClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;
        let transport =
            HttpTransport::new(config).map_err(|e| napi::Error::from_reason(e.to_string()))?;
        Ok(Self {
            inner: Arc::new(transport),
        })
    }

    /// GET request
    #[napi]
    pub async fn get(
        &self,
        url: String,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        self.do_execute(HttpMethod::GET, url, None, options).await
    }

    /// POST request
    #[napi]
    pub async fn post(
        &self,
        url: String,
        body: Option<Buffer>,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        self.do_execute(HttpMethod::POST, url, body, options).await
    }

    /// PUT request
    #[napi]
    pub async fn put(
        &self,
        url: String,
        body: Option<Buffer>,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        self.do_execute(HttpMethod::PUT, url, body, options).await
    }

    /// DELETE request
    #[napi]
    pub async fn delete(
        &self,
        url: String,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        self.do_execute(HttpMethod::DELETE, url, None, options).await
    }

    /// PATCH request
    #[napi]
    pub async fn patch(
        &self,
        url: String,
        body: Option<Buffer>,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        self.do_execute(HttpMethod::PATCH, url, body, options).await
    }

    /// Current circuit breaker state: "closed" | "open" | "half-open"
    #[napi]
    pub fn circuit_breaker_state(&self) -> String {
        match self.inner.circuit_breaker_state() {
            Some(CbState::Closed) | None => "closed".into(),
            Some(CbState::Open) => "open".into(),
            Some(CbState::HalfOpen) => "half-open".into(),
        }
    }

    // ── Metrics ──

    /// Get a snapshot of runtime metrics.
    #[napi]
    pub fn metrics(&self) -> JsMetrics {
        self.inner.metrics().into()
    }

    // ── Adaptive timeout ──

    /// Enable adaptive timeout.
    /// Timeout = clamp(P90_RTT * multiplier, min_timeout_ms, max_timeout_ms).
    #[napi]
    pub fn set_adaptive_timeout(
        &self,
        min_timeout_ms: i64,
        max_timeout_ms: i64,
        multiplier: f64,
        window_size: i64,
    ) {
        self.inner.set_adaptive_timeout(
            min_timeout_ms as u64,
            max_timeout_ms as u64,
            multiplier,
            window_size as usize,
        );
    }

    /// Disable adaptive timeout (revert to static timeout from config).
    #[napi]
    pub fn disable_adaptive_timeout(&self) {
        self.inner.disable_adaptive_timeout();
    }

    // ── Cancel ──

    /// Cancel all in-flight requests.
    #[napi]
    pub fn cancel_all(&self) {
        self.inner.cancel_all();
    }

    /// Cancel a specific in-flight request by ID.
    /// Returns true if the request was found and cancelled.
    #[napi]
    pub fn cancel_request(&self, request_id: i64) -> bool {
        self.inner.cancel_request(request_id as u64)
    }

    /// Get the next request ID for tracking a pending request.
    #[napi]
    pub fn next_request_id(&self) -> i64 {
        self.inner.next_request_id() as i64
    }

    // ── Streaming download ──

    /// Execute a streaming HTTP request.
    /// The `onChunk` callback receives JSON strings for each event:
    ///   {"type":"Headers","status":200,"headers":{...}}
    ///   {"type":"Chunk","data":"<base64>"}
    ///   {"type":"Done"}
    ///   {"type":"Error","message":"..."}
    #[napi]
    pub fn execute_stream(
        &self,
        method: String,
        url: String,
        body: Option<Buffer>,
        options: Option<RequestOptions>,
        #[napi(ts_arg_type = "(eventJson: string) => void")] on_chunk: JsFunction,
    ) -> napi::Result<()> {
        let http_method = parse_method(&method)?;
        let tsfn: Tsfn = on_chunk
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                Ok(vec![ctx.value])
            })?;

        let (headers, timeout_ms, content_type) = if let Some(opts) = options {
            (
                opts.headers.unwrap_or_default(),
                opts.timeout_ms.map(|t| t as u64),
                opts.content_type,
            )
        } else {
            (HashMap::new(), None, None)
        };

        let request = HttpRequest {
            method: http_method,
            url,
            headers,
            body: body.map(|b| b.to_vec()),
            content_type,
            timeout_ms,
            priority: catcher_core::types::observability::Priority::Normal,
            multipart: None,
        };

        let inner = self.inner.clone();
        tokio::spawn(async move {
            let _ = inner
                .execute_stream(request, move |event| {
                    let json = stream_event_to_json(&event);
                    let _ = tsfn.call(
                        Ok(json),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                })
                .await;
        });

        Ok(())
    }

    // ── Internal ──

    async fn do_execute(
        &self,
        method: HttpMethod,
        url: String,
        body: Option<Buffer>,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        let (headers, timeout_ms, content_type) = if let Some(opts) = options {
            (
                opts.headers.unwrap_or_default(),
                opts.timeout_ms.map(|t| t as u64),
                opts.content_type,
            )
        } else {
            (HashMap::new(), None, None)
        };

        let request = HttpRequest {
            method,
            url,
            headers,
            body: body.map(|b| b.to_vec()),
            content_type,
            timeout_ms,
            priority: catcher_core::types::observability::Priority::Normal,
            multipart: None,
        };

        let resp = self
            .inner
            .execute(request)
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(JsHttpResponse {
            status: resp.status,
            headers: resp.headers,
            body: Buffer::from(resp.body.as_slice()),
            elapsed_ms: resp.elapsed_ms as u32,
        })
    }
}
