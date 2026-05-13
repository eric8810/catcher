//! UniFFI bindings for catcher.
//!
//! Uses UniFFI 0.28 proc-macro mode (no UDL file needed).
//! Generates Swift (iOS) and Kotlin (Android) bindings from this Rust API.
//!
//! Build:
//!   cargo build --release
//!
//! Generate bindings:
//!   uniffi-bindgen generate --library ../target/release/libcatcher_uniffi.so --language swift --out-dir generated/swift
//!   uniffi-bindgen generate --library ../target/release/libcatcher_uniffi.so --language kotlin --out-dir generated/kotlin

use std::sync::Arc;

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    HttpTransport,
};

use catcher_ws::{
    transport::ws_client::WsTransport,
    types::ws::WsClientConfig,
    WsHandle,
};

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
    inner: HttpTransport,
}

#[uniffi::export]
impl HttpClient {
    /// Create from JSON config string.
    ///
    /// Config format matches `HttpClientConfig` in Rust:
    /// ```json
    /// {"base_url": "https://api.example.com", "connect_timeout_ms": 10000, ...}
    /// ```
    #[uniffi::constructor]
    pub fn new(config_json: String) -> Result<Self, CatcherError> {
        let config: HttpClientConfig = serde_json::from_str(&config_json)
            .map_err(|e| CatcherError::Config(e.to_string()))?;
        let inner = HttpTransport::new(config)
            .map_err(|e| CatcherError::Network(e.to_string()))?;
        Ok(Self { inner })
    }

    /// GET request
    #[uniffi::method]
    pub async fn get(&self, url: String) -> Result<HttpResponseDto, CatcherError> {
        let resp = self.inner.execute(HttpRequest {
            method: HttpMethod::GET,
            url,
            headers: Default::default(),
            body: None,
            content_type: None,
            timeout_ms: None,
        }).await.map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// POST request
    #[uniffi::method]
    pub async fn post(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let resp = self.inner.execute(HttpRequest {
            method: HttpMethod::POST,
            url,
            headers: Default::default(),
            body: Some(body),
            content_type,
            timeout_ms: None,
        }).await.map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// PUT request
    #[uniffi::method]
    pub async fn put(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let resp = self.inner.execute(HttpRequest {
            method: HttpMethod::PUT,
            url,
            headers: Default::default(),
            body: Some(body),
            content_type,
            timeout_ms: None,
        }).await.map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// DELETE request
    #[uniffi::method]
    pub async fn delete(&self, url: String) -> Result<HttpResponseDto, CatcherError> {
        let resp = self.inner.execute(HttpRequest {
            method: HttpMethod::DELETE,
            url,
            headers: Default::default(),
            body: None,
            content_type: None,
            timeout_ms: None,
        }).await.map_err(|e| CatcherError::Network(e.to_string()))?;

        Ok(HttpResponseDto {
            status: resp.status,
            body: resp.body,
            elapsed_ms: resp.elapsed_ms,
        })
    }

    /// PATCH request
    #[uniffi::method]
    pub async fn patch(
        &self,
        url: String,
        body: Vec<u8>,
        content_type: Option<String>,
    ) -> Result<HttpResponseDto, CatcherError> {
        let resp = self.inner.execute(HttpRequest {
            method: HttpMethod::PATCH,
            url,
            headers: Default::default(),
            body: Some(body),
            content_type,
            timeout_ms: None,
        }).await.map_err(|e| CatcherError::Network(e.to_string()))?;

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
}

#[uniffi::export]
impl WsClient {
    /// Create a WebSocket client and connect.
    ///
    /// The `config_json` format matches `WsClientConfig`:
    /// ```json
    /// {"urls": ["wss://echo.example.com"], "reconnect": {"initial_delay_ms": 1000}}
    /// ```
    #[uniffi::constructor]
    pub async fn new(
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

        let (handle, mut rx) = WsTransport::connect(&first_url, &config)
            .await
            .map_err(|e| CatcherError::Network(e.to_string()))?;

        // Spawn a task to forward events to the observer
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let dto = match event {
                    catcher_ws::WsEvent::Connected { url, latency_ms } => {
                        WsEventDto::Connected { url, latency_ms }
                    }
                    catcher_ws::WsEvent::Disconnected { code, reason } => {
                        WsEventDto::Disconnected { code, reason }
                    }
                    catcher_ws::WsEvent::Reconnecting { attempt, delay_ms } => {
                        WsEventDto::Reconnecting { attempt, delay_ms }
                    }
                    catcher_ws::WsEvent::Message { data, is_binary } => {
                        WsEventDto::Message { data, is_binary }
                    }
                    catcher_ws::WsEvent::Error { message } => {
                        WsEventDto::Error { message }
                    }
                    catcher_ws::WsEvent::HeartbeatRtt { rtt_ms } => {
                        WsEventDto::HeartbeatRtt { rtt_ms }
                    }
                };
                observer.on_event(dto);
            }
        });

        Ok(Self {
            handle: Arc::new(handle),
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

// UniFFI proc-macro mode setup (no UDL file needed)
uniffi::setup!();
