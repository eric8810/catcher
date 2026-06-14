//! catcher-dns — catcher 的共享 DNS 解析能力。
//!
//! 这个 crate 放置 HTTP 和 WebSocket 都会用到的 DNS 配置、缓存、
//! host mapping 和旧缓存兜底逻辑，以及系统代理检测，避免业务协议包互相依赖。

pub mod proxy;

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

    /// 清空 DNS 缓存。
    ///
    /// 网络环境变化（WiFi 切换、VPN 换节点等）后旧解析结果可能指向不可达
    /// 地址，调用此方法立即失效缓存，下次解析走全新查询。`host_mapping`
    /// 是静态配置，不受影响。关闭 `hickory-dns` feature 时无缓存，为 no-op。
    pub fn clear_cache(&self) {
        #[cfg(feature = "hickory-dns")]
        {
            if let DnsBackend::Hickory(inner) = &self.inner {
                inner.clear_cache();
            }
        }
    }

    /// 网络环境变化时的完整恢复：清空缓存 + 重建底层解析器。
    ///
    /// 仅 `clear_cache()` 不够：底层解析器的 nameserver 列表在创建时确定
    /// （系统配置只读一次），UDP socket 也绑定在旧网络接口上。新网络往往
    /// 推送不同的 DNS 服务器（VPN/蜂窝切换尤其明显），重建解析器会重读
    /// 系统 DNS 配置并重新建立到 nameserver 的连接。
    /// 关闭 `hickory-dns` feature 时每次解析都走系统调用，为 no-op。
    pub fn network_changed(&self) -> Result<(), CatcherError> {
        #[cfg(feature = "hickory-dns")]
        {
            if let DnsBackend::Hickory(inner) = &self.inner {
                inner.network_changed()?;
            }
        }
        Ok(())
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
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use catcher_core::CatcherError;
    use hickory_proto::xfer::Protocol;
    use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
    use hickory_resolver::name_server::TokioConnectionProvider;
    use hickory_resolver::TokioResolver;
    use moka::future::Cache;
    use parking_lot::{Mutex, RwLock};

    use crate::{parse_host_mapping, with_port, DnsConfig};

    #[derive(Clone, Debug)]
    struct DnsCacheEntry {
        addrs: Vec<SocketAddr>,
        inserted: Instant,
    }

    #[derive(Clone, Debug)]
    pub(super) struct HickoryDnsResolver {
        /// 底层解析器 — RwLock 包装以支持 network_changed() 热重建
        /// （重读系统 DNS 配置、重建绑定在新网络接口上的 socket）
        resolver: Arc<RwLock<TokioResolver>>,
        /// 重建解析器所需的配置
        config: DnsConfig,
        cache: Cache<String, DnsCacheEntry>,
        host_mapping: std::collections::HashMap<String, Vec<SocketAddr>>,
        cache_ttl: Duration,
        stale_ttl: Duration,
        stale_on_error: bool,
        refreshing: Arc<Mutex<HashSet<String>>>,
        /// 缓存代际 — clear_cache() 时递增。在旧代际发起的解析结果
        /// 不允许写入新代际的缓存（防止网络切换后旧网络的解析结果回灌）。
        generation: Arc<AtomicU64>,
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
                resolver: Arc::new(RwLock::new(resolver)),
                config: config.clone(),
                cache,
                host_mapping: parse_host_mapping(&config.host_mapping)?,
                cache_ttl: Duration::from_secs(config.cache_ttl_secs as u64),
                stale_ttl: Duration::from_secs(config.stale_ttl_secs as u64),
                stale_on_error: config.stale_on_error,
                refreshing: Arc::new(Mutex::new(HashSet::new())),
                generation: Arc::new(AtomicU64::new(0)),
            })
        }

        #[cfg(test)]
        pub(super) fn has_host_mapping(&self, hostname: &str) -> bool {
            self.host_mapping.contains_key(hostname)
        }

        /// 立即失效全部缓存条目（含 hickory 内部缓存）。
        ///
        /// 递增代际计数：清空时刻之前发起、之后才完成的解析（后台 stale
        /// 刷新或并发查询）属于旧网络，其结果不会留在新代际的缓存中。
        pub(super) fn clear_cache(&self) {
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.cache.invalidate_all();
            self.resolver.read().clear_cache();
            // 放行被旧代际刷新任务占位的 hostname，允许立即发起新解析
            self.refreshing.lock().clear();
        }

        /// 网络变化恢复：清空缓存 + 重建底层解析器。
        ///
        /// 重建会重读系统 DNS 配置（未显式配置 nameservers 时）并重新
        /// 建立到 nameserver 的 socket — 旧 socket 可能仍绑定在已失效
        /// 的网络接口上。重建失败时保留旧解析器（缓存已清空），返回错误。
        pub(super) fn network_changed(&self) -> Result<(), CatcherError> {
            self.clear_cache();
            let new_resolver = build_inner_resolver(&self.config)?;
            *self.resolver.write() = new_resolver;
            Ok(())
        }

        async fn do_resolve(
            &self,
            hostname: &str,
        ) -> Result<Vec<SocketAddr>, Box<dyn std::error::Error + Send + Sync>> {
            // 克隆出当前解析器（内部 Arc，克隆廉价），不跨 await 持锁
            let resolver = self.resolver.read().clone();
            let lookup = resolver
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
                    let gen_at_start = self.generation.load(Ordering::SeqCst);
                    if self.refreshing.lock().insert(key.clone()) {
                        tokio::spawn(async move {
                            if let Ok(addrs) = refresh.do_resolve(&key).await {
                                // 解析期间发生 clear_cache（网络已切换）：
                                // 结果来自旧网络，丢弃
                                if refresh.generation.load(Ordering::SeqCst) == gen_at_start {
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
                            }
                            refresh.refreshing.lock().remove(&key);
                        });
                    }
                    return Ok(entry.addrs.clone());
                }
            }

            let resolver = self.clone();
            let key = hostname.to_string();
            let gen_at_start = self.generation.load(Ordering::SeqCst);
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

            // 解析期间发生 clear_cache（网络已切换）：本次结果可能来自旧网络。
            // 仍返回给当前调用方（连接失败会自行重试），但从缓存中移除，
            // 下一次解析强制走新网络的全新查询。
            if self.generation.load(Ordering::SeqCst) != gen_at_start {
                self.cache.invalidate(hostname).await;
            }

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

    impl ReqwestDnsResolver {
        /// 网络变化时清空缓存并重建底层 resolver。
        pub fn network_changed(&self) -> Result<(), CatcherError> {
            self.inner.network_changed()
        }
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
    fn dns_config_can_use_catcher_mode() {
        let config: DnsConfig = serde_json::from_str(r#"{"mode":"catcher"}"#).unwrap();
        assert_eq!(config.mode, DnsMode::Catcher);
        assert!(config.use_catcher_resolver());
    }

    #[test]
    fn dns_config_can_use_native_mode() {
        let config: DnsConfig = serde_json::from_str(r#"{"mode":"native"}"#).unwrap();
        assert_eq!(config.mode, DnsMode::Native);
        assert!(!config.use_catcher_resolver());
    }

    #[test]
    fn partial_dns_config_keeps_catcher_mode() {
        let config: DnsConfig = serde_json::from_str(r#"{"cache_ttl_secs":300}"#).unwrap();
        assert_eq!(config.mode, DnsMode::Catcher);
        assert!(config.use_catcher_resolver());
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
    async fn clear_cache_keeps_host_mapping_resolvable() {
        let config = DnsConfig {
            host_mapping: vec![("api.test".to_string(), "127.0.0.1".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let resolver = build_stale_aware_resolver(&config).expect("valid resolver");
        resolver.clear_cache();
        let addrs = resolver
            .resolve_socket_addrs("api.test", 8080)
            .await
            .expect("host mapping survives clear_cache");
        assert_eq!(addrs, vec!["127.0.0.1:8080".parse().expect("valid addr")]);
        // 再次清空应幂等
        resolver.clear_cache();
    }

    #[tokio::test]
    async fn network_changed_rebuilds_resolver_and_keeps_resolving() {
        let config = DnsConfig {
            host_mapping: vec![("api.test".to_string(), "127.0.0.1".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let resolver = build_stale_aware_resolver(&config).expect("valid resolver");
        resolver
            .network_changed()
            .expect("rebuild with same config succeeds");
        // 重建后仍可解析（host_mapping 与新解析器都可用）
        let addrs = resolver
            .resolve_socket_addrs("api.test", 9090)
            .await
            .expect("resolves after rebuild");
        assert_eq!(addrs, vec!["127.0.0.1:9090".parse().expect("valid addr")]);
        // 幂等
        resolver.network_changed().expect("idempotent");
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
            mode: DnsMode::Catcher,
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
            mode: DnsMode::Catcher,
            nameservers: vec!["not-an-address".to_string()],
            ..Default::default()
        };
        let result = build_stale_aware_resolver(&config);
        assert!(result.is_err());
    }
}
