//! HTTP client napi bindings

use napi_derive::napi;
use std::collections::HashMap;
use std::sync::Arc;

use catcher_http::{
    types::http::{HttpClientConfig, HttpMethod, HttpRequest},
    HttpTransport,
};

/// JavaScript-facing HTTP response
#[napi(object)]
pub struct JsHttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: napi::bindgen_prelude::Buffer,
    pub elapsed_ms: u32,
}

/// JavaScript-facing HTTP client wrapping Rust HttpTransport
#[napi]
pub struct JsHttpClient {
    inner: Arc<HttpTransport>,
}

#[napi]
impl JsHttpClient {
    /// Create an HTTP client from a JSON config string
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

    /// Perform a GET request
    #[napi]
    pub async fn get(&self, url: String) -> napi::Result<JsHttpResponse> {
        let request = HttpRequest {
            method: HttpMethod::GET,
            url,
            headers: Default::default(),
            body: None,
            content_type: None,
            timeout_ms: None,
        };
        self.do_execute(request).await
    }

    /// Perform a POST request
    #[napi]
    pub async fn post(
        &self,
        url: String,
        body: napi::bindgen_prelude::Buffer,
        content_type: Option<String>,
    ) -> napi::Result<JsHttpResponse> {
        let request = HttpRequest {
            method: HttpMethod::POST,
            url,
            headers: Default::default(),
            body: Some(body.to_vec()),
            content_type,
            timeout_ms: None,
        };
        self.do_execute(request).await
    }

    async fn do_execute(&self, request: HttpRequest) -> napi::Result<JsHttpResponse> {
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
