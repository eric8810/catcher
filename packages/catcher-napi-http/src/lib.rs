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

use napi_derive::napi;
use std::collections::HashMap;
use std::sync::Arc;

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    HttpTransport,
};

use catcher_core::types::resilience::CbState;

/// JavaScript-facing HTTP response
#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: napi::bindgen_prelude::Buffer,
    pub elapsed_ms: u32,
}

/// Per-request options
#[napi(object)]
pub struct RequestOptions {
    pub headers: Option<HashMap<String, String>>,
    pub timeout_ms: Option<u32>,
    pub content_type: Option<String>,
}

/// JavaScript-facing HTTP client wrapping Rust HttpTransport
#[napi]
pub struct JsHttpClient {
    inner: Arc<HttpTransport>,
}

#[napi]
impl JsHttpClient {
    /// Create an HTTP client from a JSON config string.
    ///
    /// All fields are optional with sensible defaults.
    /// See module-level docs for the full JSON schema.
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
        body: Option<napi::bindgen_prelude::Buffer>,
        options: Option<RequestOptions>,
    ) -> napi::Result<JsHttpResponse> {
        self.do_execute(HttpMethod::POST, url, body, options).await
    }

    /// PUT request
    #[napi]
    pub async fn put(
        &self,
        url: String,
        body: Option<napi::bindgen_prelude::Buffer>,
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
        body: Option<napi::bindgen_prelude::Buffer>,
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

    // ── Internal ──

    async fn do_execute(
        &self,
        method: HttpMethod,
        url: String,
        body: Option<napi::bindgen_prelude::Buffer>,
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
        };

        let resp = self
            .inner
            .execute(request)
            .await
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        Ok(JsHttpResponse {
            status: resp.status,
            headers: resp.headers,
            body: napi::bindgen_prelude::Buffer::from(resp.body.as_slice()),
            elapsed_ms: resp.elapsed_ms as u32,
        })
    }
}
