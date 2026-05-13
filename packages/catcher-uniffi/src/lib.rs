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

use std::sync::{Arc, OnceLock};

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    HttpTransport,
};

use catcher_ws::{
    transport::ws_client::WsTransport,
    types::ws::WsClientConfig,
    WsEvent, WsHandle,
};

/// Run an async future synchronously, on a **dedicated auxiliary thread**
/// with its own tokio runtime. This avoids `block_on()` re-entrance panics
/// when called from within a tokio worker thread (e.g., WsEventObserver callbacks).
fn block_on_aux_thread<F, T>(future: F) -> std::thread::JoinHandle<T>
where
    F: std::future::Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    // Each call spawns a dedicated thread with its own single-threaded runtime.
    // Creating a new runtime per thread avoids the OnceLock race where multiple
    // threads share a current_thread runtime (which is bound to its creating thread).
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create aux tokio runtime");
        rt.block_on(future)
    })
}

/// Global tokio runtime for spawned tasks (WS event forwarding, etc.)
fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime for catcher-uniffi")
    })
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

// ═══════════════════════════════════════════════════════════════
// HTTP Client
// ═══════════════════════════════════════════════════════════════

/// HTTP response DTO (JSON-compatible)
#[derive(Debug, Clone, uniffi::Record)]
pub struct HttpResponseDto {
    pub status: u16,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

/// Resilient HTTP client backed by Rust reqwest + retry + circuit breaker
#[derive(uniffi::Object)]
pub struct HttpClient {
    inner: Arc<HttpTransport>,
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
    pub fn get(&self, url: String) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::GET,
                url,
                headers: Default::default(),
                body: None,
                content_type: None,
                timeout_ms: None,
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// POST request
    #[uniffi::method]
    pub fn post(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::POST,
                url,
                headers: Default::default(),
                body: Some(body),
                content_type,
                timeout_ms: None,
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// PUT request
    #[uniffi::method]
    pub fn put(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::PUT,
                url,
                headers: Default::default(),
                body: Some(body),
                content_type,
                timeout_ms: None,
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// DELETE request
    #[uniffi::method]
    pub fn delete(&self, url: String) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::DELETE,
                url,
                headers: Default::default(),
                body: None,
                content_type: None,
                timeout_ms: None,
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// PATCH request
    #[uniffi::method]
    pub fn patch(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let inner = self.inner.clone();
        let handle = block_on_aux_thread(async move {
            inner.execute(HttpRequest {
                method: HttpMethod::PATCH,
                url,
                headers: Default::default(),
                body: Some(body),
                content_type,
                timeout_ms: None,
            }).await
        });
        let resp = handle.join()
            .map_err(|_| CatcherError::Network("thread panicked".into()))?
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
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
        let first_url = urls
            .first()
            .cloned()
            .ok_or_else(|| CatcherError::Config("urls cannot be empty".into()))?;

        // Use aux thread to avoid block_on re-entrance
        let handle = block_on_aux_thread(async move {
            WsTransport::connect(&first_url, &config).await
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

// UniFFI proc-macro mode scaffolding (no UDL file needed)
uniffi::setup_scaffolding!();
