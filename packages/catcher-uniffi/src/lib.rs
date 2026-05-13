//! UniFFI bindings for catcher.
//!
//! Auto-generates Swift (iOS) and Kotlin (Android) bindings from this Rust API.
//! Run `uniffi-bindgen generate src/catcher.udl` to regenerate bindings after changes.

use std::sync::Arc;

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    HttpTransport,
};

use catcher_ws::{
    types::ws::WsClientConfig,
    WsHandle, WsTransport,
};

// ═══════════════════════════════════════════════════════════════
// HTTP Client
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum CatcherError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Config error: {0}")]
    Config(String),
}

/// Dart/JSON-compatible HTTP response
#[derive(Debug, Clone)]
pub struct HttpResponseDto {
    pub status: u16,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

/// Resilient HTTP client
pub struct HttpClient {
    inner: HttpTransport,
}

#[uniffi::export]
impl HttpClient {
    /// Create from JSON config string.
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
}

// ═══════════════════════════════════════════════════════════════
// WebSocket Client
// ═══════════════════════════════════════════════════════════════

/// WebSocket event (JSON-compatible)
#[derive(Debug, Clone)]
pub enum WsEventDto {
    Connected { url: String, latency_ms: u32 },
    Disconnected { code: u16, reason: String },
    Message { data: String, is_binary: bool },
    Error { message: String },
}

/// Resilient WebSocket client
pub struct WsClient {
    handle: Arc<WsHandle>,
}

/// Create a WebSocket client.
#[uniffi::export]
pub fn create_ws_client(config_json: String) -> Result<Arc<WsClient>, CatcherError> {
    let config: WsClientConfig = serde_json::from_str(&config_json)
        .map_err(|e| CatcherError::Config(e.to_string()))?;

    // UniFFI limitation: async needs a callback mechanism
    // For production, use UniFFI's async support in 0.28+
    Err(CatcherError::Config("WebSocket async binding requires UniFFI async support (v0.28+)".into()))
}

uniffi::include_scaffolding!("catcher");
