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
    ErrorStrategy, ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::Arc;

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest, StreamEvent},
    HttpTransport, MetricsSnapshot, SseClient, SseStream,
};

use catcher_core::types::resilience::CbState;
use catcher_core::types::sse::SseClientConfig;

type Tsfn = ThreadsafeFunction<String, ErrorStrategy::CalleeHandled>;

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

    // ── SSE ──

    /// Create a one-shot SSE stream (no auto-reconnect).
    /// The `onEvent` callback receives JSON strings.
    /// Returns a `JsSseStream` handle. Call `.close()` to stop.
    #[napi]
    pub fn sse_stream(
        config_json: String,
        #[napi(ts_arg_type = "(eventJson: string) => void")] on_event: JsFunction,
    ) -> napi::Result<JsSseStream> {
        let config: SseClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let tsfn: Tsfn = on_event
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                Ok(vec![ctx.value])
            })?;

        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

        tokio::spawn(async move {
            match SseStream::connect(config).await {
                Ok(mut stream) => {
                    use tokio_stream::StreamExt;
                    loop {
                        tokio::select! {
                            result = stream.next() => {
                                match result {
                                    Some(Ok(line)) => {
                                        let _ = tsfn.call(
                                            Ok(sse_line_json(&line)),
                                            ThreadsafeFunctionCallMode::NonBlocking,
                                        );
                                    }
                                    Some(Err(e)) => {
                                        let _ = tsfn.call(
                                            Ok(sse_error_json(&e.to_string())),
                                            ThreadsafeFunctionCallMode::NonBlocking,
                                        );
                                    }
                                    None => break,
                                }
                            }
                            _ = cancel_rx.recv() => break,
                        }
                    }
                    let _ = tsfn.call(
                        Ok(sse_end_json()),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
                Err(e) => {
                    let _ = tsfn.call(
                        Ok(sse_error_json(&e.to_string())),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
            }
        });

        Ok(JsSseStream {
            cancel_tx: Some(cancel_tx),
        })
    }

    /// Create a long-lived SSE client with auto-reconnect.
    #[napi]
    pub fn sse_client(
        config_json: String,
        #[napi(ts_arg_type = "(eventJson: string) => void")] on_event: JsFunction,
    ) -> napi::Result<JsSseClient> {
        let config: SseClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| napi::Error::from_reason(e.to_string()))?;

        let tsfn: Tsfn = on_event
            .create_threadsafe_function(0, |ctx: ThreadSafeCallContext<String>| {
                Ok(vec![ctx.value])
            })?;

        let (cancel_tx, mut cancel_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

        tokio::spawn(async move {
            match SseClient::connect(config).await {
                Ok(mut client) => {
                    loop {
                        tokio::select! {
                            result = client.next_line() => {
                                match result {
                                    Some(Ok(line)) => {
                                        let _ = tsfn.call(
                                            Ok(sse_line_json(&line)),
                                            ThreadsafeFunctionCallMode::NonBlocking,
                                        );
                                    }
                                    Some(Err(e)) => {
                                        let _ = tsfn.call(
                                            Ok(sse_error_json(&e.to_string())),
                                            ThreadsafeFunctionCallMode::NonBlocking,
                                        );
                                    }
                                    None => break,
                                }
                            }
                            _ = cancel_rx.recv() => {
                                client.close();
                                break;
                            }
                        }
                    }
                    let _ = tsfn.call(
                        Ok(sse_end_json()),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
                Err(e) => {
                    let _ = tsfn.call(
                        Ok(sse_error_json(&e.to_string())),
                        ThreadsafeFunctionCallMode::NonBlocking,
                    );
                }
            }
        });

        Ok(JsSseClient {
            cancel_tx: Some(cancel_tx),
        })
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

/// JavaScript-facing SSE stream handle (one-shot, no auto-reconnect).
/// Call `close()` to abort the stream.
#[napi]
pub struct JsSseStream {
    cancel_tx: Option<tokio::sync::mpsc::UnboundedSender<()>>,
}

#[napi]
impl JsSseStream {
    /// Close the SSE stream — aborts the background read loop.
    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        if let Some(ref tx) = self.cancel_tx {
            let _ = tx.send(());
        }
        Ok(())
    }
}

/// JavaScript-facing SSE client handle (auto-reconnect).
/// Call `close()` to stop the client and auto-reconnection.
#[napi]
pub struct JsSseClient {
    cancel_tx: Option<tokio::sync::mpsc::UnboundedSender<()>>,
}

#[napi]
impl JsSseClient {
    /// Close the SSE client — stops auto-reconnect.
    #[napi]
    pub fn close(&self) -> napi::Result<()> {
        if let Some(ref tx) = self.cancel_tx {
            let _ = tx.send(());
        }
        Ok(())
    }
}

// ── Helpers ──

fn parse_method(s: &str) -> napi::Result<HttpMethod> {
    match s.to_uppercase().as_str() {
        "GET" => Ok(HttpMethod::GET),
        "POST" => Ok(HttpMethod::POST),
        "PUT" => Ok(HttpMethod::PUT),
        "DELETE" => Ok(HttpMethod::DELETE),
        "PATCH" => Ok(HttpMethod::PATCH),
        other => Err(napi::Error::from_reason(format!(
            "Unknown HTTP method: {other}"
        ))),
    }
}

fn stream_event_to_json(event: &StreamEvent) -> String {
    match event {
        StreamEvent::Headers { status, headers } => serde_json::json!({
            "type": "Headers",
            "status": status,
            "headers": headers,
        })
        .to_string(),
        StreamEvent::Chunk(data) => {
            use base64::Engine;
            serde_json::json!({
                "type": "Chunk",
                "data": base64::engine::general_purpose::STANDARD.encode(data),
            })
            .to_string()
        }
        StreamEvent::Done => serde_json::json!({"type": "Done"}).to_string(),
        StreamEvent::Error(msg) => serde_json::json!({
            "type": "Error",
            "message": msg,
        })
        .to_string(),
    }
}

fn sse_line_json(line: &str) -> String {
    serde_json::json!({"type": "Line", "data": line}).to_string()
}

fn sse_error_json(msg: &str) -> String {
    serde_json::json!({"type": "Error", "message": msg}).to_string()
}

fn sse_end_json() -> String {
    serde_json::json!({"type": "End"}).to_string()
}

// ── Rust unit tests ──

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify parse_method works for all supported methods
    #[test]
    fn parse_method_all() {
        assert!(matches!(parse_method("GET").unwrap(), HttpMethod::GET));
        assert!(matches!(parse_method("get").unwrap(), HttpMethod::GET));
        assert!(matches!(parse_method("POST").unwrap(), HttpMethod::POST));
        assert!(matches!(parse_method("PUT").unwrap(), HttpMethod::PUT));
        assert!(matches!(parse_method("DELETE").unwrap(), HttpMethod::DELETE));
        assert!(matches!(parse_method("PATCH").unwrap(), HttpMethod::PATCH));
        assert!(parse_method("INVALID").is_err());
    }

    /// Verify JsMetrics conversion from MetricsSnapshot
    #[test]
    fn metrics_snapshot_to_js() {
        let snap = MetricsSnapshot {
            http_requests: 100,
            http_success_rate: 0.95,
            http_avg_latency_us: 1500,
            http_retries: 5,
            ws_connect_success_rate: 1.0,
            ws_disconnects: 2,
            ws_messages_sent: 50,
            ws_messages_received: 48,
            cb_open_count: 1,
            queue_timeouts: 0,
        };
        let js: JsMetrics = snap.into();
        assert_eq!(js.http_requests, 100);
        assert_eq!(js.http_success_rate, 0.95);
        assert_eq!(js.http_avg_latency_us, 1500);
        assert_eq!(js.http_retries, 5);
        assert_eq!(js.cb_open_count, 1);
    }

    /// Verify HttpClientConfig parsing with minimal config
    #[test]
    fn http_config_minimal() {
        let json = r#"{}"#;
        let config: HttpClientConfig = serde_json::from_str(json).unwrap();
        assert!(config.base_url.is_empty());
    }

    /// Verify HttpClientConfig parsing with full config
    #[test]
    fn http_config_full() {
        let json = r#"{
            "base_url": "https://api.example.com",
            "connect_timeout_ms": 5000,
            "response_timeout_ms": 30000,
            "retry": { "max_attempts": 3, "backoff": "Exponential" },
            "circuit_breaker": { "failure_threshold": 5, "reset_timeout_ms": 30000 }
        }"#;
        let config: HttpClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.base_url, "https://api.example.com");
        assert_eq!(config.connect_timeout_ms, 5000);
        assert_eq!(config.response_timeout_ms, 30000);
    }

    /// Verify SseClientConfig parsing
    #[test]
    fn sse_config_parse() {
        let json = r#"{
            "url": "https://example.com/events",
            "method": "GET",
            "headers": {"Authorization": "Bearer test"},
            "timeout_ms": 10000,
            "reconnect": {
                "max_retries": 5,
                "initial_delay_ms": 500,
                "max_delay_ms": 30000,
                "backoff_multiplier": 2.0
            }
        }"#;
        let config: SseClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.url, "https://example.com/events");
        assert_eq!(config.timeout_ms, 10000);
        let rc = config.reconnect.unwrap();
        assert_eq!(rc.max_retries, 5);
    }

    /// Verify StreamEvent variant matching
    #[test]
    fn stream_event_variants() {
        let evt = StreamEvent::Headers {
            status: 200,
            headers: HashMap::from([("content-type".into(), "text/plain".into())]),
        };
        match &evt {
            StreamEvent::Headers { status, .. } => assert_eq!(*status, 200),
            _ => panic!("Expected Headers"),
        }

        let evt = StreamEvent::Chunk(vec![1, 2, 3]);
        match &evt {
            StreamEvent::Chunk(data) => assert_eq!(data, &[1, 2, 3]),
            _ => panic!("Expected Chunk"),
        }

        assert!(matches!(StreamEvent::Done, StreamEvent::Done));

        let evt = StreamEvent::Error("test error".into());
        match &evt {
            StreamEvent::Error(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected Error"),
        }
    }

    /// Verify HTTP transport can be created and metrics is accessible
    #[test]
    fn http_transport_metrics_initial() {
        let config = HttpClientConfig::default();
        let transport = HttpTransport::new(config).unwrap();
        let metrics = transport.metrics();
        assert_eq!(metrics.http_requests, 0);
        assert_eq!(metrics.http_success_rate, 0.0);
        assert_eq!(metrics.http_retries, 0);
    }

    /// Verify cancel_all and cancel_request don't panic on fresh client
    #[test]
    fn http_cancel_on_fresh_client() {
        let config = HttpClientConfig::default();
        let transport = HttpTransport::new(config).unwrap();
        transport.cancel_all();
        assert!(!transport.cancel_request(999));
    }

    /// Verify adaptive timeout set/disable
    #[test]
    fn http_adaptive_timeout_set_disable() {
        let config = HttpClientConfig::default();
        let transport = HttpTransport::new(config).unwrap();
        transport.set_adaptive_timeout(1000, 30000, 2.0, 50);
        transport.disable_adaptive_timeout();
    }

    /// Verify next_request_id returns monotonically increasing values
    #[test]
    fn http_request_id_monotonic() {
        let config = HttpClientConfig::default();
        let transport = HttpTransport::new(config).unwrap();
        let id1 = transport.next_request_id();
        let id2 = transport.next_request_id();
        assert!(id2 > id1, "request IDs should be monotonically increasing");
    }

    /// Verify base64 encoding of StreamEvent::Chunk
    #[test]
    fn stream_chunk_base64_encoding() {
        use base64::Engine;
        let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
        assert_eq!(b64, "3q2+7w==");
    }

    /// Verify stream_event_to_json produces valid JSON for each variant
    #[test]
    fn stream_event_json_output() {
        let json = stream_event_to_json(&StreamEvent::Done);
        assert_eq!(json, "{\"type\":\"Done\"}");

        let json = stream_event_to_json(&StreamEvent::Error("oops".into()));
        assert!(json.contains("\"type\":\"Error\""));
        assert!(json.contains("oops"));

        let json = stream_event_to_json(&StreamEvent::Headers {
            status: 200,
            headers: HashMap::new(),
        });
        assert!(json.contains("\"status\":200"));
    }

    /// Verify SSE cancel channel works
    #[test]
    fn sse_cancel_channel() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        assert!(tx.send(()).is_ok());
        assert!(rx.try_recv().is_ok());
    }

    /// Verify SSE JSON helpers
    #[test]
    fn sse_json_helpers() {
        let line = sse_line_json("data: hello");
        assert!(line.contains("\"type\":\"Line\""));
        assert!(line.contains("data: hello"));

        let err = sse_error_json("timeout");
        assert!(err.contains("\"type\":\"Error\""));
        assert!(err.contains("timeout"));

        let end = sse_end_json();
        assert_eq!(end, "{\"type\":\"End\"}");
    }
}
