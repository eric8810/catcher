//! UniFFI bindings for catcher.
//!
//! Uses UniFFI 0.28 proc-macro mode (no UDL file needed).
//! Generates Swift (iOS) and Kotlin (Android) bindings from this Rust API.
//!
//! # Architecture note on async
//!
//! UniFFI 0.28 does not support async methods. All async Rust operations are
//! bridged synchronously via `block_on_aux_thread()` which dispatches work to
//! a **separate std thread** with its own tokio runtime. This avoids the
//! `block_on()` re-entrance panic that would occur if a WsEventObserver
//! callback (running on a tokio thread) calls back into an HttpClient method.
//!
//! Build:
//!   cargo build --release
//!
//! Generate bindings:
//!   uniffi-bindgen generate --library ../target/release/libcatcher_uniffi.so --language swift --out-dir generated/swift
//!   uniffi-bindgen generate --library ../target/release/libcatcher_uniffi.so --language kotlin --out-dir generated/kotlin

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    sse::client::SseClient,
    sse::SseStream,
    HttpTransport,
};

use catcher_ws::{
    transport::ws_client::WsTransport,
    types::ws::WsClientConfig,
    WsEvent, WsHandle,
};

use catcher_core::types::sse::{SseClientConfig, SseMethod};
use catcher_core::CatcherError as CoreCatcherError;
use tokio_stream::StreamExt;

/// Run an async future synchronously, on a **dedicated auxiliary thread**
/// with its own tokio runtime. This avoids `block_on()` re-entrance panics
/// when called from within a tokio worker thread (e.g., WsEventObserver callbacks).
fn block_on_aux_thread<F, T>(future: F) -> std::thread::JoinHandle<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create aux tokio runtime");
        rt.block_on(future)
    })
}

/// Global tokio runtime for spawned tasks (WS event forwarding, SSE events, etc.)
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-uniffi")
    })
}

fn parse_headers_json(headers_json: Option<String>) -> HashMap<String, String> {
    match headers_json {
        Some(s) if !s.is_empty() => {
            serde_json::from_str(&s).unwrap_or_default()
        }
        _ => HashMap::new(),
    }
}

// ═══════════════════════════════════════════════════════════════
// Error type
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum CatcherError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Config error: {0}")]
    Config(String),
}

impl From<CoreCatcherError> for CatcherError {
    fn from(e: CoreCatcherError) -> Self {
        CatcherError::Network(e.to_string())
    }
}

// ═══════════════════════════════════════════════════════════════
// HTTP Client
// ═══════════════════════════════════════════════════════════════

/// HTTP response DTO (JSON-compatible)
#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpResponseDto {
    pub status: u16,
    /// Response headers as "key: value" strings (UniFFI cannot map HashMap in Records)
    pub headers: Vec<String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

/// Resilient HTTP client backed by Rust reqwest + retry + circuit breaker
#[derive(uniffi::Object)]
pub struct HttpClient {
    inner: Arc<HttpTransport>,
}

fn http_response_to_dto(resp: catcher_http::types::http::HttpResponse) -> HttpResponseDto {
    let headers: Vec<String> = resp
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect();
    HttpResponseDto {
        status: resp.status,
        headers,
        body: resp.body,
        elapsed_ms: resp.elapsed_ms,
    }
}

#[uniffi::export]
impl HttpClient {
    /// Create from JSON config string.
    #[uniffi::constructor]
    pub fn new(config_json: String) -> Result<Self, CatcherError> {
        let config: HttpClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| CatcherError::Config(e.to_string()))?;
        let inner = Arc::new(HttpTransport::new(config)
            .map_err(|e| CatcherError::Network(e.to_string()))?);
        Ok(Self { inner })
    }

    /// GET request
    #[uniffi::method]
    pub fn get(
        &self,
        url: String,
        headers_json: Option<String>,
        timeout_ms: Option<u32>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let headers = parse_headers_json(headers_json);
        let timeout = timeout_ms.map(|t| t as u64);
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::GET,
                url,
                headers,
                body: None,
                content_type: None,
                timeout_ms: timeout,
                ..Default::default()
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;
        Ok(http_response_to_dto(resp))
    }

    /// POST request
    #[uniffi::method]
    pub fn post(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
        headers_json: Option<String>,
        timeout_ms: Option<u32>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let headers = parse_headers_json(headers_json);
        let timeout = timeout_ms.map(|t| t as u64);
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::POST,
                url,
                headers,
                body: Some(body),
                content_type,
                timeout_ms: timeout,
                ..Default::default()
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;
        Ok(http_response_to_dto(resp))
    }

    /// PUT request
    #[uniffi::method]
    pub fn put(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
        headers_json: Option<String>,
        timeout_ms: Option<u32>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let headers = parse_headers_json(headers_json);
        let timeout = timeout_ms.map(|t| t as u64);
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::PUT,
                url,
                headers,
                body: Some(body),
                content_type,
                timeout_ms: timeout,
                ..Default::default()
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;
        Ok(http_response_to_dto(resp))
    }

    /// DELETE request
    #[uniffi::method]
    pub fn delete(
        &self,
        url: String,
        headers_json: Option<String>,
        timeout_ms: Option<u32>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let headers = parse_headers_json(headers_json);
        let timeout = timeout_ms.map(|t| t as u64);
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::DELETE,
                url,
                headers,
                body: None,
                content_type: None,
                timeout_ms: timeout,
                ..Default::default()
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;
        Ok(http_response_to_dto(resp))
    }

    /// PATCH request
    #[uniffi::method]
    pub fn patch(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
        headers_json: Option<String>,
        timeout_ms: Option<u32>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let headers = parse_headers_json(headers_json);
        let timeout = timeout_ms.map(|t| t as u64);
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::PATCH,
                url,
                headers,
                body: Some(body),
                content_type,
                timeout_ms: timeout,
                ..Default::default()
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;
        Ok(http_response_to_dto(resp))
    }

    /// POST SSE stream (one-shot, for OpenAI/Anthropic streaming APIs).
    /// Collects all SSE content lines and returns them as a list of event JSON strings.
    #[uniffi::method]
    pub fn sse_stream(
        &self,
        method: String,
        url: String,
        body: Option<Vec<u8>>,
        headers_json: Option<String>,
    ) -> Result<Vec<String>, CatcherError> {
        let headers = parse_headers_json(headers_json);
        let sse_method = if method.to_uppercase() == "GET" {
            SseMethod::GET
        } else {
            SseMethod::POST
        };

        let config = SseClientConfig {
            url,
            method: sse_method,
            headers,
            body: body.map(|b| String::from_utf8_lossy(&b).to_string()),
            reconnect: None,
            timeout_ms: 30_000,
            circuit_breaker: None,
        };

        let handle = block_on_aux_thread(async move {
            let mut stream = SseStream::connect(config).await?;
            let mut events = Vec::new();
            while let Some(line_result) = stream.next().await {
                match line_result {
                    Ok(line) => {
                        events.push(serde_json::json!({"type":"data","data":line}).to_string());
                    }
                    Err(e) => {
                        events.push(serde_json::json!({"type":"error","data":e.to_string()}).to_string());
                    }
                }
            }
            Ok::<_, CatcherError>(events)
        });

        handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))
    }

    /// Query circuit breaker state as JSON string
    #[uniffi::method]
    pub fn circuit_breaker_state(&self) -> String {
        match self.inner.circuit_breaker_state() {
            Some(s) => serde_json::to_string(&s).unwrap_or_default(),
            None => r#"{"state":"disabled"}"#.to_string(),
        }
    }

    /// Query runtime metrics as JSON string
    #[uniffi::method]
    pub fn metrics(&self) -> String {
        serde_json::to_string(&self.inner.metrics()).unwrap_or_default()
    }
}

// ═══════════════════════════════════════════════════════════════
// WebSocket Client
// ═══════════════════════════════════════════════════════════════

/// WebSocket event types
#[derive(Debug, Clone, uniffi::Enum)]
pub enum WsEventDto {
    Connected { url: String, latency_ms: u64 },
    Disconnected { code: u16, reason: String },
    Reconnecting { attempt: u32, delay_ms: u64 },
    Message { data: Vec<u8>, is_binary: bool },
    Error { message: String },
    HeartbeatRtt { rtt_ms: u64 },
}

/// Convert internal WsEvent to the UniFFI-safe DTO.
impl From<WsEvent> for WsEventDto {
    fn from(event: WsEvent) -> Self {
        match event {
            WsEvent::Connected { url, latency_ms } => WsEventDto::Connected { url, latency_ms },
            WsEvent::Disconnected { code, reason } => WsEventDto::Disconnected { code, reason },
            WsEvent::Reconnecting { attempt, delay_ms } => WsEventDto::Reconnecting { attempt, delay_ms },
            WsEvent::Message { data, is_binary } => WsEventDto::Message { data, is_binary },
            WsEvent::Error { message } => WsEventDto::Error { message },
            WsEvent::HeartbeatRtt { rtt_ms } => WsEventDto::HeartbeatRtt { rtt_ms },
        }
    }
}

/// Observer interface for receiving WebSocket events.
///
/// Swift/Kotlin implementations register via the constructor.
#[uniffi::export(callback_interface)]
pub trait WsEventObserver: Send + Sync {
    fn on_event(&self, event: WsEventDto);
}

/// Resilient WebSocket client backed by Rust tokio-tungstenite
#[derive(uniffi::Object)]
pub struct WsClient {
    handle: Arc<WsHandle>,
    _event_task: tokio::task::JoinHandle<()>,
}

#[uniffi::export]
impl WsClient {
    /// Create a WebSocket client and connect.
    /// Tries all configured URLs with race-to-first semantics.
    ///
    /// Note: Sync because UniFFI 0.28 does not support async constructors.
    #[uniffi::constructor]
    pub fn new(
        config_json: String,
        observer: Box<dyn WsEventObserver>,
    ) -> Result<Self, CatcherError> {
        let config: WsClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| CatcherError::Config(e.to_string()))?;

        let urls = config.urls.clone();
        if urls.is_empty() {
            return Err(CatcherError::Config("urls cannot be empty".into()));
        }

        // Multi-endpoint racing, reconnect, heartbeat handled by WsTransport::connect
        let handle = block_on_aux_thread(async move {
            WsTransport::connect(&config).await
        });
        let (ws_handle, mut rx) = handle.join()
            .map_err(|_| CatcherError::Network("connect thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        let ws_handle = Arc::new(ws_handle);

        // Spawn event forwarding on the main multi-threaded runtime
        // (not inside block_on, so observer callbacks can safely call HTTP methods)
        let event_task = runtime().spawn(async move {
            while let Some(event) = rx.recv().await {
                observer.on_event(event.into());
            }
        });

        Ok(Self {
            handle: ws_handle,
            _event_task: event_task,
        })
    }

    /// Send a text message
    #[uniffi::method]
    pub fn send_text(&self, text: String) -> Result<(), CatcherError> {
        self.handle
            .send_text(&text)
            .map_err(|e| CatcherError::Network(e.to_string()))
    }

    /// Send a binary message
    #[uniffi::method]
    pub fn send_binary(&self, data: Vec<u8>) -> Result<(), CatcherError> {
        self.handle
            .send_binary(&data)
            .map_err(|e| CatcherError::Network(e.to_string()))
    }

    /// Close the connection
    #[uniffi::method]
    pub fn close(&self, code: u16, reason: String) -> Result<(), CatcherError> {
        self.handle
            .close(code, &reason)
            .map_err(|e| CatcherError::Network(e.to_string()))
    }
}

// When WsClient is dropped, abort the event-forwarding task to prevent
// callbacks to a GC'd observer.
impl Drop for WsClient {
    fn drop(&mut self) {
        self._event_task.abort();
    }
}

// ═══════════════════════════════════════════════════════════════
// SSE Client (persistent with auto-reconnect)
// ═══════════════════════════════════════════════════════════════

/// SSE event DTO
#[derive(Debug, Clone, uniffi::Enum)]
pub enum SseEventDto {
    Open,
    Data { data: String },
    Error { message: String },
    Close,
}

/// Observer for persistent SSE client events
#[uniffi::export(callback_interface)]
pub trait SseEventObserver: Send + Sync {
    fn on_event(&self, event: SseEventDto);
}

/// Persistent SSE client with auto-reconnect
#[derive(uniffi::Object)]
pub struct SseClientHandle {
    _event_task: tokio::task::JoinHandle<()>,
}

#[uniffi::export]
impl SseClientHandle {
    #[uniffi::constructor]
    pub fn connect(
        config_json: String,
        observer: Box<dyn SseEventObserver>,
    ) -> Result<Self, CatcherError> {
        let config: SseClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| CatcherError::Config(e.to_string()))?;

        let handle = block_on_aux_thread(async move {
            let mut client = SseClient::connect(config).await?;

            // Send open event
            observer.on_event(SseEventDto::Open);

            // Forward SSE lines
            while let Some(line_result) = client.next_line().await {
                match line_result {
                    Ok(line) => observer.on_event(SseEventDto::Data { data: line }),
                    Err(e) => {
                        observer.on_event(SseEventDto::Error {
                            message: e.to_string(),
                        });
                    }
                }
            }

            observer.on_event(SseEventDto::Close);
            Ok::<_, CatcherError>(())
        });

        handle.join()
            .map_err(|_| CatcherError::Network("SSE connect thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        // Spawn an empty task to keep the struct alive.
        // The real work is done synchronously above in block_on_aux_thread.
        let event_task = runtime().spawn(async {});
        Ok(Self { _event_task: event_task })
    }
}

// ═══════════════════════════════════════════════════════════════
// Codec (msgpack pack / unpack)
// ═══════════════════════════════════════════════════════════════

/// Pack a JSON string to msgpack binary
#[uniffi::export]
pub fn catcher_pack(json_input: String) -> Result<Vec<u8>, CatcherError> {
    let value: serde_json::Value = serde_json::from_str(&json_input)
        .map_err(|e| CatcherError::Config(format!("invalid JSON: {e}")))?;
    catcher_ws::codec::pack(&value)
        .map_err(|e| CatcherError::Config(e.to_string()))
}

/// Unpack msgpack binary to a JSON string
#[uniffi::export]
pub fn catcher_unpack(data: Vec<u8>) -> Result<String, CatcherError> {
    let value: serde_json::Value = catcher_ws::codec::unpack_value(&data)
        .map_err(|e| CatcherError::Config(e.to_string()))?;
    serde_json::to_string(&value)
        .map_err(|e| CatcherError::Config(e.to_string()))
}

// ═══════════════════════════════════════════════════════════════
// Network Quality
// ═══════════════════════════════════════════════════════════════

/// Evaluate network quality to the given host (single HTTP HEAD measurement).
/// Returns a JSON string with level, avg_rtt_ms, jitter_ms, etc.
#[uniffi::export]
pub fn evaluate_quality(host: String) -> Result<String, CatcherError> {
    use catcher_http::observability::network_quality::NetworkQualityEvaluator;

    let handle = block_on_aux_thread(async move {
        let mut evaluator = NetworkQualityEvaluator::new(20);
        match evaluator.measure_http_rtt(&host, "/").await {
            Ok(_rtt) => {
                let result = evaluator.evaluate();
                Ok(serde_json::to_string(&result).unwrap_or_default())
            }
            Err(e) => {
                let result = evaluator.evaluate();
                let mut map = serde_json::to_value(&result).unwrap_or_default();
                if let Some(obj) = map.as_object_mut() {
                    obj.insert("error".into(), e.to_string().into());
                }
                Ok(serde_json::to_string(&map).unwrap_or_default())
            }
        }
    });

    handle.join()
        .map_err(|_| CatcherError::Network("quality thread panicked".into()))?
}

// UniFFI proc-macro mode scaffolding (no UDL file needed)
uniffi::setup_scaffolding!();
