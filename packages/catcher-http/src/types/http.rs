use catcher_core::types::default_true;
pub use catcher_core::types::network::{ProxyAuth, ProxyConfig, TlsConfig, TlsVersion};
use catcher_core::types::resilience::{CircuitBreakerConfig, RetryConfig};
pub use catcher_dns::{DnsConfig, DnsMode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP 方法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HttpMethod {
    #[default]
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

/// HTTP 请求（内部使用）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpRequest {
    #[serde(default)]
    pub method: HttpMethod,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 请求优先级 (A-01)，用于优先级队列调度
    #[serde(default = "default_priority")]
    pub priority: catcher_core::types::observability::Priority,
    /// Multipart form data (B-02). When set, overrides `body` and `content_type`.
    #[serde(skip)]
    pub multipart: Option<crate::transport::multipart::MultipartForm>,
}

fn default_priority() -> catcher_core::types::observability::Priority {
    catcher_core::types::observability::Priority::Normal
}

/// HTTP 响应（内部使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub elapsed_ms: u64,
}

/// 流式 HTTP 响应事件（N-02）
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Headers {
        status: u16,
        headers: HashMap<String, String>,
    },
    Chunk(bytes::Bytes),
    Done,
    Error(String),
}

/// 连接池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// 每个 host 最大空闲连接数
    #[serde(alias = "maxIdlePerHost", default = "default_max_idle_per_host")]
    pub max_idle_per_host: usize,
    /// 空闲连接超时（秒）— 连接空闲超过此时间将被淘汰
    /// 降低此值可减少 retry 时复用已死连接的风险 (G-01/G-02)
    #[serde(alias = "idleTimeoutSecs", default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// 是否启用 TCP keepalive
    #[serde(alias = "keepAlive", default = "default_true")]
    pub keep_alive: bool,
    /// keepalive 间隔（秒）— 更短的间隔能更快检测死连接 (G-02)
    #[serde(
        alias = "keepAliveIntervalSecs",
        default = "default_keep_alive_interval"
    )]
    pub keep_alive_interval_secs: u64,
}

fn default_max_idle_per_host() -> usize {
    10
}
fn default_idle_timeout_secs() -> u64 {
    30
}
fn default_keep_alive_interval() -> u64 {
    20
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_idle_per_host: default_max_idle_per_host(),
            idle_timeout_secs: default_idle_timeout_secs(),
            keep_alive: default_true(),
            keep_alive_interval_secs: default_keep_alive_interval(),
        }
    }
}

/// 重定向配置 (G6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedirectConfig {
    /// 是否跟随重定向. Default: true
    #[serde(default = "default_true")]
    pub follow: bool,
    /// 最大重定向次数. Default: 5
    #[serde(alias = "maxRedirects", default = "default_max_redirects")]
    pub max_redirects: u32,
}

fn default_max_redirects() -> u32 {
    5
}

impl Default for RedirectConfig {
    fn default() -> Self {
        Self {
            follow: true,
            max_redirects: default_max_redirects(),
        }
    }
}

/// HTTP 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpClientConfig {
    /// 基础 URL
    #[serde(alias = "baseUrl", default)]
    pub base_url: String,

    /// 连接超时（毫秒）
    #[serde(alias = "connectTimeoutMs", default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,

    /// 响应超时（毫秒）
    #[serde(alias = "responseTimeoutMs", default = "default_response_timeout")]
    pub response_timeout_ms: u64,

    /// 连接池配置
    #[serde(default)]
    pub pool: PoolConfig,

    /// TLS 配置
    #[serde(default)]
    pub tls: TlsConfig,

    /// DNS 配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsConfig>,

    /// 重试配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,

    /// 熔断器配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circuit_breaker: Option<CircuitBreakerConfig>,

    /// 最大并发请求数 (NOTE: not yet enforced at the transport layer — queuing is handled by TS/UniFFI wrappers)
    #[serde(alias = "maxConcurrency", default = "default_max_concurrency")]
    pub max_concurrency: u32,

    /// 默认请求头（每次请求自动携带，per-request headers 优先级更高）
    #[serde(alias = "defaultHeaders", default)]
    pub default_headers: HashMap<String, String>,

    /// Hostname 覆写（HTTP DNS 场景：连接 IP 但 Host header 用域名）
    /// NOTE: not yet wired into reqwest; configure via default_headers "Host" field as workaround.
    #[serde(alias = "hostnameOverride", skip_serializing_if = "Option::is_none")]
    pub hostname_override: Option<String>,

    // --- G4: Proxy ---
    /// 代理配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy: Option<ProxyConfig>,

    // --- G6: Redirect ---
    /// 重定向配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect: Option<RedirectConfig>,

    // --- G12: Auth ---
    /// Basic 认证
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,

    /// Bearer token
    #[serde(alias = "bearerToken", skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,

    /// 启用 msgpack 编解码 — body 自动 JSON↔msgpack 转码
    #[serde(default)]
    pub msgpack: bool,

    /// 网络路径版本。外部平台在 VPN / 代理 / DNS 变化时应传入新的值并重建 client。
    #[serde(alias = "networkPathId", skip_serializing_if = "Option::is_none")]
    pub network_path_id: Option<String>,
}

fn default_connect_timeout() -> u64 {
    10_000
}
fn default_response_timeout() -> u64 {
    30_000
}
fn default_max_concurrency() -> u32 {
    50
}

impl Default for HttpClientConfig {
    fn default() -> Self {
        Self {
            base_url: String::new(),
            connect_timeout_ms: default_connect_timeout(),
            response_timeout_ms: default_response_timeout(),
            pool: PoolConfig::default(),
            tls: TlsConfig::default(),
            dns: None,
            retry: None,
            circuit_breaker: None,
            max_concurrency: default_max_concurrency(),
            default_headers: HashMap::new(),
            hostname_override: None,
            proxy: None,
            redirect: None,
            auth: None,
            bearer_token: None,
            msgpack: false,
            network_path_id: None,
        }
    }
}
