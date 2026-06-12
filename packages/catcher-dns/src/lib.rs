//! catcher-dns — catcher 的共享 DNS 解析能力。
//!
//! 这个 crate 放置 HTTP 和 WebSocket 都会用到的 DNS 配置、缓存、
//! host mapping 和旧缓存兜底逻辑，避免业务协议包互相依赖。
//!
//! 启用默认 `hickory-dns` feature 时，解析器使用 hickory-resolver，
//! 并提供缓存、过期缓存兜底和后台刷新。关闭该 feature 时，解析器只
//! 使用系统 DNS 和 `host_mapping`，不提供缓存和过期缓存兜底。

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use catcher_core::types::default_true;
use catcher_core::CatcherError;
use serde::{Deserialize, Serialize};

/// DNS 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsConfig {
    /// DNS 模式。
    #[serde(default)]
    pub mode: DnsMode,
    /// 缓存条目数上限。
    #[serde(alias = "cacheSize", default = "default_dns_cache_size")]
    pub cache_size: u64,
    /// DNS 缓存 TTL，单位秒。
    #[serde(alias = "cacheTtlSecs", default = "default_dns_cache_ttl")]
    pub cache_ttl_secs: u32,
    /// 否定缓存 TTL，单位秒。
    #[serde(alias = "negativeTtlSecs", default = "default_dns_negative_ttl")]
    pub negative_ttl_secs: u32,
    /// 过期后仍可使用旧缓存的时间，单位秒。
    #[serde(alias = "staleTtlSecs", default = "default_dns_stale_ttl")]
    pub stale_ttl_secs: u32,
    /// DNS 失败时是否使用旧缓存兜底。
    #[serde(alias = "staleOnError", default = "default_true")]
    pub stale_on_error: bool,
    /// 自定义 DNS 服务器地址列表，如 `["8.8.8.8:53"]`。
    #[serde(default)]
    pub nameservers: Vec<String>,
    /// 主机名到 IP 的固定映射。
    #[serde(alias = "hostMapping", default)]
    pub host_mapping: HashMap<String, String>,
    /// 读取系统 DNS 失败时是否退回 Hickory 默认 DNS。
    #[serde(alias = "fallbackToDefaultNameservers", default)]
    pub fallback_to_default_nameservers: bool,
}

/// DNS 模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DnsMode {
    /// 使用 Catcher resolver、缓存、host mapping 和旧缓存兜底。
    #[default]
    Catcher,
    /// 不注入 Catcher resolver，让协议库使用自身默认解析流程。
    Native,
}

impl DnsConfig {
    /// 是否应接入 Catcher resolver。
    pub fn use_catcher_resolver(&self) -> bool {
        self.mode == DnsMode::Catcher
    }
}

fn default_dns_cache_size() -> u64 {
    512
}

fn default_dns_cache_ttl() -> u32 {
    300
}

fn default_dns_negative_ttl() -> u32 {
    60
}

fn default_dns_stale_ttl() -> u32 {
    3600
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            mode: DnsMode::default(),
            cache_size: default_dns_cache_size(),
            cache_ttl_secs: default_dns_cache_ttl(),
            negative_ttl_secs: default_dns_negative_ttl(),
            stale_ttl_secs: default_dns_stale_ttl(),
            stale_on_error: default_true(),
            nameservers: Vec::new(),
            host_mapping: HashMap::new(),
            fallback_to_default_nameservers: false,
        }
    }
}

/// 共享 DNS 解析器。
///
/// 默认 feature 会启用缓存和旧缓存兜底；关闭 `hickory-dns` 后仅保留
/// 系统 DNS 查询和 `host_mapping`。
#[derive(Clone, Debug)]
pub struct DnsResolver {
    #[cfg(feature = "hickory-dns")]
    inner: DnsBackend,
    #[cfg(not(feature = "hickory-dns"))]
    host_mapping: HashMap<String, Vec<SocketAddr>>,
}

#[cfg(feature = "hickory-dns")]
#[derive(Clone, Debug)]
enum DnsBackend {
    Hickory(Box<hickory_backend::HickoryDnsResolver>),
    Native {
        host_mapping: HashMap<String, Vec<SocketAddr>>,
    },
}

impl DnsResolver {
    /// 根据配置创建 DNS 解析器。
    pub fn new(config: &DnsConfig) -> Result<Self, CatcherError> {
        #[cfg(feature = "hickory-dns")]
        {
            if config.mode == DnsMode::Native {
                return Ok(Self {
                    inner: DnsBackend::Native {
                        host_mapping: parse_host_mapping(&config.host_mapping)?,
                    },
                });
            }
            Ok(Self {
                inner: DnsBackend::Hickory(Box::new(hickory_backend::HickoryDnsResolver::new(
                    config,
                )?)),
            })
        }

        #[cfg(not(feature = "hickory-dns"))]
        {
            Ok(Self {
                host_mapping: parse_host_mapping(&config.host_mapping)?,
            })
        }
    }

    /// 解析主机名并填入调用方需要连接的端口。
    pub async fn resolve_socket_addrs(
        &self,
        hostname: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, CatcherError> {
        #[cfg(feature = "hickory-dns")]
        {
            match &self.inner {
                DnsBackend::Hickory(inner) => inner.resolve_socket_addrs(hostname, port).await,
                DnsBackend::Native { host_mapping } => {
                    resolve_native_socket_addrs(host_mapping, hostname, port).await
                }
            }
        }

        #[cfg(not(feature = "hickory-dns"))]
        {
            resolve_native_socket_addrs(&self.host_mapping, hostname, port).await
        }
    }

    #[cfg(test)]
    fn has_host_mapping(&self, hostname: &str) -> bool {
        #[cfg(feature = "hickory-dns")]
        {
            match &self.inner {
                DnsBackend::Hickory(inner) => inner.has_host_mapping(hostname),
                DnsBackend::Native { host_mapping } => host_mapping.contains_key(hostname),
            }
        }

        #[cfg(not(feature = "hickory-dns"))]
        {
            self.host_mapping.contains_key(hostname)
        }
    }
}

async fn resolve_native_socket_addrs(
    host_mapping: &HashMap<String, Vec<SocketAddr>>,
    hostname: &str,
    port: u16,
) -> Result<Vec<SocketAddr>, CatcherError> {
    if let Some(addrs) = host_mapping.get(hostname) {
        return Ok(with_port(addrs.clone(), port));
    }

    tokio::net::lookup_host((hostname, port))
        .await
        .map(|iter| iter.collect())
        .map_err(|e| CatcherError::DnsError {
            host: hostname.to_string(),
            reason: e.to_string(),
        })
}

/// 创建共享 DNS 解析器。
pub fn build_stale_aware_resolver(config: &DnsConfig) -> Result<Arc<DnsResolver>, CatcherError> {
    Ok(Arc::new(DnsResolver::new(config)?))
}

fn parse_host_mapping(
    mapping: &HashMap<String, String>,
) -> Result<HashMap<String, Vec<SocketAddr>>, CatcherError> {
    let mut parsed = HashMap::new();
    for (host, ip_str) in mapping {
        let ip: IpAddr = ip_str.parse().map_err(|_| {
            CatcherError::InvalidConfig(format!(
                "dns.host_mapping: invalid IP '{ip_str}' for host '{host}'"
            ))
        })?;
        parsed.insert(host.clone(), vec![SocketAddr::new(ip, 0)]);
    }
    Ok(parsed)
}

fn with_port(addrs: Vec<SocketAddr>, port: u16) -> Vec<SocketAddr> {
    addrs
        .into_iter()
        .map(|addr| SocketAddr::new(addr.ip(), port))
        .collect()
}

#[cfg(feature = "hickory-dns")]
mod hickory_backend {
    use std::collections::HashSet;
    use std::net::SocketAddr;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use catcher_core::CatcherError;
    use hickory_proto::xfer::Protocol;
    use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
    use hickory_resolver::name_server::TokioConnectionProvider;
    use hickory_resolver::TokioResolver;
    use moka::future::Cache;
    use parking_lot::Mutex;

    use crate::{parse_host_mapping, with_port, DnsConfig};

    #[derive(Clone, Debug)]
    struct DnsCacheEntry {
        addrs: Vec<SocketAddr>,
        inserted: Instant,
    }

    #[derive(Clone, Debug)]
    pub(super) struct HickoryDnsResolver {
        resolver: TokioResolver,
        cache: Cache<String, DnsCacheEntry>,
        host_mapping: std::collections::HashMap<String, Vec<SocketAddr>>,
        cache_ttl: Duration,
        stale_ttl: Duration,
        stale_on_error: bool,
        refreshing: Arc<Mutex<HashSet<String>>>,
    }

    impl HickoryDnsResolver {
        pub(super) fn new(config: &DnsConfig) -> Result<Self, CatcherError> {
            let resolver = build_inner_resolver(config)?;
            let total_ttl =
                Duration::from_secs(config.cache_ttl_secs as u64 + config.stale_ttl_secs as u64);
            let cache = Cache::builder()
                .max_capacity(config.cache_size)
                .time_to_live(total_ttl)
                .build();

            Ok(Self {
                resolver,
                cache,
                host_mapping: parse_host_mapping(&config.host_mapping)?,
                cache_ttl: Duration::from_secs(config.cache_ttl_secs as u64),
                stale_ttl: Duration::from_secs(config.stale_ttl_secs as u64),
                stale_on_error: config.stale_on_error,
                refreshing: Arc::new(Mutex::new(HashSet::new())),
            })
        }

        #[cfg(test)]
        pub(super) fn has_host_mapping(&self, hostname: &str) -> bool {
            self.host_mapping.contains_key(hostname)
        }

        async fn do_resolve(
            &self,
            hostname: &str,
        ) -> Result<Vec<SocketAddr>, Box<dyn std::error::Error + Send + Sync>> {
            let lookup = self
                .resolver
                .lookup_ip(hostname)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            let addrs: Vec<SocketAddr> = lookup
                .into_iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect();
            if addrs.is_empty() {
                return Err("DNS resolved to empty address list".into());
            }
            Ok(addrs)
        }

        async fn resolve_zero_port(
            &self,
            hostname: &str,
        ) -> Result<Vec<SocketAddr>, Box<dyn std::error::Error + Send + Sync>> {
            if let Some(addrs) = self.host_mapping.get(hostname) {
                return Ok(addrs.clone());
            }

            if let Some(entry) = self.cache.get(hostname).await {
                let age = entry.inserted.elapsed();

                if age < self.cache_ttl {
                    return Ok(entry.addrs.clone());
                }

                if age < self.cache_ttl + self.stale_ttl {
                    let refresh = self.clone();
                    let key = hostname.to_string();
                    if self.refreshing.lock().insert(key.clone()) {
                        tokio::spawn(async move {
                            if let Ok(addrs) = refresh.do_resolve(&key).await {
                                refresh
                                    .cache
                                    .insert(
                                        key.clone(),
                                        DnsCacheEntry {
                                            addrs,
                                            inserted: Instant::now(),
                                        },
                                    )
                                    .await;
                            }
                            refresh.refreshing.lock().remove(&key);
                        });
                    }
                    return Ok(entry.addrs.clone());
                }
            }

            let resolver = self.clone();
            let key = hostname.to_string();
            let result = self
                .cache
                .try_get_with::<_, String>(hostname.to_string(), async move {
                    match resolver.do_resolve(&key).await {
                        Ok(addrs) => Ok(DnsCacheEntry {
                            addrs,
                            inserted: Instant::now(),
                        }),
                        Err(e) => Err(e.to_string()),
                    }
                })
                .await;

            match result {
                Ok(entry) => Ok(entry.addrs.clone()),
                Err(e) => {
                    if self.stale_on_error {
                        if let Some(entry) = self.cache.get(hostname).await {
                            return Ok(entry.addrs.clone());
                        }
                    }
                    let msg = match Arc::try_unwrap(e) {
                        Ok(s) => s,
                        Err(arc) => (*arc).clone(),
                    };
                    Err(msg.into())
                }
            }
        }

        pub(super) async fn resolve_socket_addrs(
            &self,
            hostname: &str,
            port: u16,
        ) -> Result<Vec<SocketAddr>, CatcherError> {
            let addrs =
                self.resolve_zero_port(hostname)
                    .await
                    .map_err(|e| CatcherError::DnsError {
                        host: hostname.to_string(),
                        reason: e.to_string(),
                    })?;
            Ok(with_port(addrs, port))
        }
    }

    fn build_inner_resolver(config: &DnsConfig) -> Result<TokioResolver, CatcherError> {
        let mut opts = ResolverOpts::default();
        opts.negative_max_ttl = Some(Duration::from_secs(config.negative_ttl_secs as u64));

        if config.nameservers.is_empty() {
            let (sys_config, sys_opts) = match hickory_resolver::system_conf::read_system_conf() {
                Ok(config) => config,
                Err(_) if config.fallback_to_default_nameservers => {
                    (ResolverConfig::default(), ResolverOpts::default())
                }
                Err(e) => {
                    return Err(CatcherError::DnsError {
                        host: "system".to_string(),
                        reason: format!("read system DNS config: {e}"),
                    });
                }
            };
            opts.negative_max_ttl = opts.negative_max_ttl.or(sys_opts.negative_max_ttl);
            Ok(
                TokioResolver::builder_with_config(sys_config, TokioConnectionProvider::default())
                    .with_options(opts)
                    .build(),
            )
        } else {
            let mut resolver_config = ResolverConfig::new();
            for ns in &config.nameservers {
                let addr: SocketAddr = ns.parse().map_err(|e| {
                    CatcherError::InvalidConfig(format!("invalid nameserver '{ns}': {e}"))
                })?;
                resolver_config.add_name_server(NameServerConfig {
                    socket_addr: addr,
                    protocol: Protocol::Udp,
                    tls_dns_name: None,
                    trust_negative_responses: false,
                    bind_addr: None,
                    http_endpoint: None,
                });
            }
            Ok(TokioResolver::builder_with_config(
                resolver_config,
                TokioConnectionProvider::default(),
            )
            .with_options(opts)
            .build())
        }
    }
}

#[cfg(feature = "reqwest-resolver")]
pub mod reqwest_resolver {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use catcher_core::CatcherError;

    use crate::{build_stale_aware_resolver, DnsConfig, DnsResolver};

    /// reqwest DNS 适配器。
    #[derive(Clone, Debug)]
    pub struct ReqwestDnsResolver {
        inner: Arc<DnsResolver>,
    }

    impl reqwest::dns::Resolve for ReqwestDnsResolver {
        fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
            let inner = self.inner.clone();
            Box::pin(async move {
                let hostname = name.as_str().to_string();
                let addrs = inner
                    .resolve_socket_addrs(&hostname, 0)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
                Ok(Box::new(addrs.into_iter()) as Box<dyn Iterator<Item = SocketAddr> + Send>)
            })
        }
    }

    /// 构建 reqwest 可用的共享 DNS resolver。
    pub fn build_reqwest_resolver(
        config: &DnsConfig,
    ) -> Result<Arc<ReqwestDnsResolver>, CatcherError> {
        Ok(Arc::new(ReqwestDnsResolver {
            inner: build_stale_aware_resolver(config)?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{build_stale_aware_resolver, DnsConfig, DnsMode, DnsResolver};
    use std::collections::HashMap;

    #[test]
    fn dns_config_with_host_mapping_keeps_mapping() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.test".to_string(), "127.0.0.1".to_string());
        let config = DnsConfig {
            host_mapping,
            ..Default::default()
        };
        assert_eq!(
            config.host_mapping.get("api.test"),
            Some(&"127.0.0.1".to_string())
        );
    }

    #[test]
    fn dns_config_uses_default_values() {
        let config = DnsConfig::default();
        assert_eq!(config.mode, DnsMode::Catcher);
        assert_eq!(config.cache_size, 512);
        assert_eq!(config.cache_ttl_secs, 300);
        assert_eq!(config.negative_ttl_secs, 60);
        assert_eq!(config.stale_ttl_secs, 3600);
        assert!(config.stale_on_error);
        assert!(config.nameservers.is_empty());
        assert!(config.host_mapping.is_empty());
        assert!(!config.fallback_to_default_nameservers);
        assert!(config.use_catcher_resolver());
    }

    #[test]
    fn dns_config_can_use_native_mode() {
        let config: DnsConfig = serde_json::from_str(r#"{"mode":"native"}"#).unwrap();
        assert_eq!(config.mode, DnsMode::Native);
        assert!(!config.use_catcher_resolver());
    }

    #[test]
    fn dns_config_supports_multiple_host_mappings() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.example.com".to_string(), "10.0.0.1".to_string());
        host_mapping.insert("cdn.example.com".to_string(), "10.0.0.2".to_string());
        let config = DnsConfig {
            host_mapping,
            ..Default::default()
        };
        assert_eq!(config.host_mapping.len(), 2);
        assert_eq!(
            config.host_mapping.get("api.example.com"),
            Some(&"10.0.0.1".to_string())
        );
        assert_eq!(
            config.host_mapping.get("cdn.example.com"),
            Some(&"10.0.0.2".to_string())
        );
    }

    #[test]
    fn dns_config_deserializes_snake_case_fields() {
        let json = r#"{
            "cache_size": 1024,
            "cache_ttl_secs": 600,
            "negative_ttl_secs": 30,
            "stale_ttl_secs": 7200,
            "stale_on_error": false,
            "nameservers": ["8.8.8.8:53"],
            "host_mapping": {"api.test": "127.0.0.1"}
        }"#;
        let config: DnsConfig = serde_json::from_str(json).expect("valid DNS JSON");
        assert_eq!(config.cache_size, 1024);
        assert_eq!(config.cache_ttl_secs, 600);
        assert_eq!(config.negative_ttl_secs, 30);
        assert_eq!(config.stale_ttl_secs, 7200);
        assert!(!config.stale_on_error);
        assert_eq!(config.nameservers, vec!["8.8.8.8:53"]);
        assert_eq!(
            config.host_mapping.get("api.test"),
            Some(&"127.0.0.1".to_string())
        );
    }

    #[test]
    fn dns_config_deserializes_camel_case_aliases() {
        let json = r#"{
            "cacheSize": 256,
            "cacheTtlSecs": 120,
            "negativeTtlSecs": 10,
            "staleTtlSecs": 1800,
            "staleOnError": true,
            "hostMapping": {"cdn.test": "10.0.0.1"}
        }"#;
        let config: DnsConfig = serde_json::from_str(json).expect("valid DNS JSON");
        assert_eq!(config.cache_size, 256);
        assert_eq!(config.cache_ttl_secs, 120);
        assert_eq!(config.negative_ttl_secs, 10);
        assert_eq!(config.stale_ttl_secs, 1800);
        assert!(config.stale_on_error);
        assert_eq!(
            config.host_mapping.get("cdn.test"),
            Some(&"10.0.0.1".to_string())
        );
    }

    #[test]
    fn dns_config_defaults_on_empty_json() {
        let config: DnsConfig = serde_json::from_str("{}").expect("valid empty JSON");
        assert_eq!(config.cache_size, 512);
        assert_eq!(config.cache_ttl_secs, 300);
        assert_eq!(config.negative_ttl_secs, 60);
        assert_eq!(config.stale_ttl_secs, 3600);
        assert!(config.stale_on_error);
    }

    #[test]
    fn resolver_builds_with_defaults() {
        let config = DnsConfig::default();
        let result = build_stale_aware_resolver(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn resolver_accepts_valid_host_mapping() {
        let config = DnsConfig {
            host_mapping: vec![("api.test".to_string(), "10.0.0.42".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let resolver = DnsResolver::new(&config).expect("valid resolver");
        assert!(resolver.has_host_mapping("api.test"));
    }

    #[test]
    fn resolver_rejects_invalid_host_mapping_ip() {
        let config = DnsConfig {
            host_mapping: vec![("bad.test".to_string(), "not-an-ip".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let result = DnsResolver::new(&config);
        assert!(result.is_err());
        let msg = result.expect_err("invalid IP must fail").to_string();
        assert!(
            msg.contains("not-an-ip"),
            "error should mention the bad IP: {msg}"
        );
    }

    #[tokio::test]
    async fn resolver_applies_host_mapping_port() {
        let config = DnsConfig {
            host_mapping: vec![("api.test".to_string(), "127.0.0.1".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let resolver = build_stale_aware_resolver(&config).expect("valid resolver");
        let addrs = resolver
            .resolve_socket_addrs("api.test", 8080)
            .await
            .expect("host mapping resolves");
        assert_eq!(addrs, vec!["127.0.0.1:8080".parse().expect("valid addr")]);
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn resolver_builds_with_nameservers() {
        let config = DnsConfig {
            nameservers: vec!["8.8.8.8:53".to_string()],
            ..Default::default()
        };
        let result = build_stale_aware_resolver(&config);
        assert!(result.is_ok());
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn resolver_rejects_invalid_nameserver() {
        let config = DnsConfig {
            nameservers: vec!["not-an-address".to_string()],
            ..Default::default()
        };
        let result = build_stale_aware_resolver(&config);
        assert!(result.is_err());
    }
}
