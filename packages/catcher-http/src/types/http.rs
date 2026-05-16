use catcher_core::types::resilience::{CircuitBreakerConfig, RetryConfig};
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
    Headers { status: u16, headers: HashMap<String, String> },
    Chunk(Vec<u8>),
    Done,
    Error(String),
}

/// 连接池配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// 每个 host 最大空闲连接数
    #[serde(default = "default_max_idle_per_host")]
    pub max_idle_per_host: usize,
    /// 空闲连接超时（秒）— 连接空闲超过此时间将被淘汰
    /// 降低此值可减少 retry 时复用已死连接的风险 (G-01/G-02)
    #[serde(default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// 是否启用 TCP keepalive
    #[serde(default = "default_true")]
    pub keep_alive: bool,
    /// keepalive 间隔（秒）— 更短的间隔能更快检测死连接 (G-02)
    #[serde(default = "default_keep_alive_interval")]
    pub keep_alive_interval_secs: u64,
}

fn default_max_idle_per_host() -> usize {
    10
}
fn default_idle_timeout_secs() -> u64 {
    30
}
fn default_true() -> bool {
    true
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

/// TLS 版本
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsVersion {
    Tls1_0,
    Tls1_1,
    Tls1_2,
    Tls1_3,
}

/// TLS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// 是否验证服务端证书
    #[serde(default = "default_true")]
    pub reject_unauthorized: bool,
    /// CA 证书 PEM (inline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_cert_pem: Option<String>,
    /// CA 证书文件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ca_cert_path: Option<String>,
    /// 客户端证书 PEM (inline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_cert_pem: Option<String>,
    /// 客户端证书文件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_cert_path: Option<String>,
    /// 客户端私钥 PEM (inline)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key_pem: Option<String>,
    /// 客户端私钥文件路径
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_key_path: Option<String>,
    /// PFX/PKCS12 客户端身份 (binary)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_identity_pfx: Option<Vec<u8>>,
    /// PFX 身份密码
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_identity_password: Option<String>,
    /// TLS SNI 覆写
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_sni_override: Option<String>,
    /// 最低 TLS 版本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_tls_version: Option<TlsVersion>,
    /// 最高 TLS 版本
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tls_version: Option<TlsVersion>,
    /// SHA-256 公钥指纹 pinning (deferred — requires custom ServerCertVerifier)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_sha256: Option<Vec<String>>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            reject_unauthorized: true,
            ca_cert_pem: None,
            ca_cert_path: None,
            client_cert_pem: None,
            client_cert_path: None,
            client_key_pem: None,
            client_key_path: None,
            client_identity_pfx: None,
            client_identity_password: None,
            tls_sni_override: None,
            min_tls_version: None,
            max_tls_version: None,
            pin_sha256: None,
        }
    }
}

/// DNS 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    /// DNS 缓存 TTL（秒）
    #[serde(default = "default_dns_cache_ttl")]
    pub cache_ttl_secs: u32,
    /// 自定义 DNS 服务器地址列表（如 ["8.8.8.8:53"]）
    #[serde(default)]
    pub nameservers: Vec<String>,
    /// Hostname → IP 映射 (G7: custom DNS host mapping)
    #[serde(default)]
    pub host_mapping: HashMap<String, String>,
}

fn default_dns_cache_ttl() -> u32 {
    300
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            cache_ttl_secs: default_dns_cache_ttl(),
            nameservers: Vec::new(),
            host_mapping: HashMap::new(),
        }
    }
}

/// 代理认证
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

/// 代理配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// "http://host:port" | "https://host:port" | "socks5://host:port"
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,
    #[serde(default)]
    pub no_proxy: Vec<String>,
}

/// 重定向配置 (G6)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedirectConfig {
    /// 是否跟随重定向. Default: true
    #[serde(default = "default_true")]
    pub follow: bool,
    /// 最大重定向次数. Default: 5
    #[serde(default = "default_max_redirects")]
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
    #[serde(default)]
    pub base_url: String,

    /// 连接超时（毫秒）
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,

    /// 响应超时（毫秒）
    #[serde(default = "default_response_timeout")]
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
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u32,

    /// 默认请求头（每次请求自动携带，per-request headers 优先级更高）
    #[serde(default)]
    pub default_headers: HashMap<String, String>,

    /// Hostname 覆写（HTTP DNS 场景：连接 IP 但 Host header 用域名）
    /// NOTE: not yet wired into reqwest; configure via default_headers "Host" field as workaround.
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bearer_token: Option<String>,
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
        }
    }
}
