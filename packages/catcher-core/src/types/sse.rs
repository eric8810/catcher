use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::resilience::CircuitBreakerConfig;

/// SSE HTTP method
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub enum SseMethod {
    #[default]
    GET,
    POST,
}

/// SSE client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseClientConfig {
    pub url: String,
    #[serde(default)]
    pub method: SseMethod,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Request body (JSON string, caller serializes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect: Option<SseReconnectConfig>,
    #[serde(alias = "timeoutMs", default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(alias = "circuitBreaker", skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreakerConfig>,
}

fn default_timeout() -> u64 {
    30_000
}

impl Default for SseClientConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: SseMethod::default(),
            headers: HashMap::new(),
            body: None,
            reconnect: None,
            timeout_ms: default_timeout(),
            circuit_breaker: None,
        }
    }
}

/// SSE reconnect configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseReconnectConfig {
    #[serde(alias = "maxRetries", default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(alias = "initialDelayMs", default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    #[serde(alias = "maxDelayMs", default = "default_max_delay")]
    pub max_delay_ms: u64,
    #[serde(alias = "backoffMultiplier", default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_max_retries() -> u32 {
    10
}
fn default_initial_delay() -> u64 {
    1000
}
fn default_max_delay() -> u64 {
    30_000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}

impl Default for SseReconnectConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            backoff_multiplier: default_backoff_multiplier(),
        }
    }
}
