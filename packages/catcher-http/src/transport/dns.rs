// ── StaleAwareDnsResolver (hickory-dns feature) ──────────────

#[cfg(feature = "hickory-dns")]
mod stale_resolver {
    use std::collections::{HashMap, HashSet};
    use std::net::{IpAddr, SocketAddr};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
    use hickory_resolver::name_server::TokioConnectionProvider;
    use hickory_resolver::TokioResolver;
    use hickory_proto::xfer::Protocol;
    use moka::future::Cache;
    use parking_lot::Mutex;

    use catcher_core::CatcherError;
    use crate::types::http::DnsConfig;

    #[derive(Clone, Debug)]
    struct DnsCacheEntry {
        addrs: Vec<SocketAddr>,
        inserted: Instant,
    }

    #[derive(Clone, Debug)]
    pub struct StaleAwareDnsResolver {
        resolver: TokioResolver,
        cache: Cache<String, DnsCacheEntry>,
        host_mapping: HashMap<String, Vec<SocketAddr>>,
        cache_ttl: Duration,
        stale_ttl: Duration,
        stale_on_error: bool,
        refreshing: Arc<Mutex<HashSet<String>>>,
    }

    impl StaleAwareDnsResolver {
        pub fn new(config: &DnsConfig) -> Result<Self, CatcherError> {
            let resolver = build_inner_resolver(config)?;

            let total_ttl = Duration::from_secs(
                config.cache_ttl_secs as u64 + config.stale_ttl_secs as u64,
            );
            let cache = Cache::builder()
                .max_capacity(config.cache_size)
                .time_to_live(total_ttl)
                .build();

            let mut host_mapping: HashMap<String, Vec<SocketAddr>> = HashMap::new();
            for (host, ip_str) in &config.host_mapping {
                let ip: IpAddr = ip_str.parse().map_err(|_| {
                    CatcherError::InvalidConfig(format!(
                        "dns.host_mapping: invalid IP '{ip_str}' for host '{host}'"
                    ))
                })?;
                host_mapping.insert(host.clone(), vec![SocketAddr::new(ip, 0)]);
            }

            Ok(Self {
                resolver,
                cache,
                host_mapping,
                cache_ttl: Duration::from_secs(config.cache_ttl_secs as u64),
                stale_ttl: Duration::from_secs(config.stale_ttl_secs as u64),
                stale_on_error: config.stale_on_error,
                refreshing: Arc::new(Mutex::new(HashSet::new())),
            })
        }

        #[cfg(test)]
        pub fn has_host_mapping(&self, hostname: &str) -> bool {
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
            let addrs: Vec<SocketAddr> =
                lookup.into_iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            if addrs.is_empty() {
                return Err("DNS resolved to empty address list".into());
            }
            Ok(addrs)
        }
    }

    impl reqwest::dns::Resolve for StaleAwareDnsResolver {
        fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
            let this = self.clone();
            Box::pin(async move {
                let hostname = name.as_str().to_string();

                // host_mapping has highest priority
                if let Some(addrs) = this.host_mapping.get(&hostname) {
                    return Ok(Box::new(addrs.clone().into_iter())
                        as Box<dyn Iterator<Item = SocketAddr> + Send>);
                }

                // Check cache
                if let Some(entry) = this.cache.get(&hostname).await {
                    let age = entry.inserted.elapsed();

                    if age < this.cache_ttl {
                        return Ok(Box::new(entry.addrs.clone().into_iter())
                            as Box<dyn Iterator<Item = SocketAddr> + Send>);
                    }

                    if age < this.cache_ttl + this.stale_ttl {
                        // Stale hit — return old result, spawn background refresh
                        let refresh = this.clone();
                        let key = hostname.clone();
                        if this.refreshing.lock().insert(key.clone()) {
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
                        return Ok(Box::new(entry.addrs.clone().into_iter())
                            as Box<dyn Iterator<Item = SocketAddr> + Send>);
                    }
                    // Past stale window — fall through to synchronous resolve
                }

                // Cache miss — coalesced resolve via try_get_with.
                // Concurrent lookups for the same hostname share one DNS query.
                let resolver = this.clone();
                let key = hostname.clone();
                let result = this
                    .cache
                    .try_get_with::<_, String>(hostname.clone(), async move {
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
                    Ok(entry) => Ok(Box::new(entry.addrs.clone().into_iter())
                        as Box<dyn Iterator<Item = SocketAddr> + Send>),
                    Err(e) => {
                        // stale-on-error fallback
                        if this.stale_on_error {
                            if let Some(entry) = this.cache.get(&hostname).await {
                                return Ok(Box::new(entry.addrs.clone().into_iter())
                                    as Box<dyn Iterator<Item = SocketAddr> + Send>);
                            }
                        }
                        let msg: String = match Arc::try_unwrap(e) {
                            Ok(s) => s,
                            Err(arc) => (*arc).clone(),
                        };
                        Err(msg.into())
                    }
                }
            })
        }
    }

    fn build_inner_resolver(config: &DnsConfig) -> Result<TokioResolver, CatcherError> {
        let mut opts = ResolverOpts::default();
        opts.negative_max_ttl = Some(Duration::from_secs(config.negative_ttl_secs as u64));

        if config.nameservers.is_empty() {
            // Use system DNS configuration
            let (sys_config, sys_opts) = hickory_resolver::system_conf::read_system_conf()
                .unwrap_or_else(|_| (ResolverConfig::default(), ResolverOpts::default()));
            opts.negative_max_ttl =
                opts.negative_max_ttl.or(sys_opts.negative_max_ttl);
            let resolver =
                TokioResolver::builder_with_config(sys_config, TokioConnectionProvider::default())
                    .with_options(opts)
                    .build();
            Ok(resolver)
        } else {
            let mut resolver_config = ResolverConfig::new();
            for ns in &config.nameservers {
                let addr: std::net::SocketAddr = ns
                    .parse()
                    .map_err(|e| CatcherError::InvalidConfig(format!("invalid nameserver '{ns}': {e}")))?;
                resolver_config.add_name_server(NameServerConfig {
                    socket_addr: addr,
                    protocol: Protocol::Udp,
                    tls_dns_name: None,
                    trust_negative_responses: false,
                    bind_addr: None,
                    http_endpoint: None,
                });
            }
            let resolver = TokioResolver::builder_with_config(
                resolver_config,
                TokioConnectionProvider::default(),
            )
            .with_options(opts)
            .build();
            Ok(resolver)
        }
    }

    pub fn build_stale_aware_resolver(
        config: &DnsConfig,
    ) -> Result<Arc<StaleAwareDnsResolver>, CatcherError> {
        let resolver = StaleAwareDnsResolver::new(config)?;
        Ok(Arc::new(resolver))
    }
}

#[cfg(feature = "hickory-dns")]
pub use stale_resolver::build_stale_aware_resolver;

// ── Fallback when hickory-dns is not enabled ──────────────

#[cfg(not(feature = "hickory-dns"))]
pub fn build_dns_resolver(
    _config: &crate::types::http::DnsConfig,
) -> Result<Option<()>, catcher_core::CatcherError> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use crate::types::http::DnsConfig;
    use std::collections::HashMap;

    #[test]
    fn rdns1_dns_config_with_host_mapping() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.test".to_string(), "127.0.0.1".to_string());
        let config = DnsConfig {
            host_mapping,
            ..Default::default()
        };
        assert_eq!(config.host_mapping.get("api.test"), Some(&"127.0.0.1".to_string()));
    }

    #[test]
    fn rdns3_empty_dns_config() {
        let config = DnsConfig::default();
        assert_eq!(config.cache_size, 512);
        assert_eq!(config.cache_ttl_secs, 300);
        assert_eq!(config.negative_ttl_secs, 60);
        assert_eq!(config.stale_ttl_secs, 3600);
        assert!(config.stale_on_error);
        assert!(config.nameservers.is_empty());
        assert!(config.host_mapping.is_empty());
    }

    #[test]
    fn rdns4_dns_config_multiple_host_mappings() {
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
    fn rdns5_host_mapping_lookup() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.test".to_string(), "10.0.0.5".to_string());
        let config = DnsConfig {
            host_mapping,
            ..Default::default()
        };
        assert_eq!(config.host_mapping.get("api.test"), Some(&"10.0.0.5".to_string()));
        assert_eq!(config.host_mapping.get("unknown.test"), None);
    }

    #[test]
    fn rdns7_dns_config_serde_new_fields() {
        let json = r#"{
            "cache_size": 1024,
            "cache_ttl_secs": 600,
            "negative_ttl_secs": 30,
            "stale_ttl_secs": 7200,
            "stale_on_error": false,
            "nameservers": ["8.8.8.8:53"],
            "host_mapping": {"api.test": "127.0.0.1"}
        }"#;
        let config: DnsConfig = serde_json::from_str(json).unwrap();
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
    fn rdns8_dns_config_serde_camel_case_aliases() {
        let json = r#"{
            "cacheSize": 256,
            "cacheTtlSecs": 120,
            "negativeTtlSecs": 10,
            "staleTtlSecs": 1800,
            "staleOnError": true,
            "hostMapping": {"cdn.test": "10.0.0.1"}
        }"#;
        let config: DnsConfig = serde_json::from_str(json).unwrap();
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
    fn rdns9_dns_config_defaults_on_empty_json() {
        let config: DnsConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.cache_size, 512);
        assert_eq!(config.cache_ttl_secs, 300);
        assert_eq!(config.negative_ttl_secs, 60);
        assert_eq!(config.stale_ttl_secs, 3600);
        assert!(config.stale_on_error);
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn rdns10_stale_aware_resolver_builds_with_defaults() {
        let config = DnsConfig::default();
        let result = super::stale_resolver::build_stale_aware_resolver(&config);
        assert!(result.is_ok());
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn rdns11_stale_aware_resolver_builds_with_nameservers() {
        let config = DnsConfig {
            nameservers: vec!["8.8.8.8:53".to_string()],
            ..Default::default()
        };
        let result = super::stale_resolver::build_stale_aware_resolver(&config);
        assert!(result.is_ok());
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn rdns12_stale_aware_resolver_invalid_nameserver() {
        let config = DnsConfig {
            nameservers: vec!["not-an-address".to_string()],
            ..Default::default()
        };
        let result = super::stale_resolver::build_stale_aware_resolver(&config);
        assert!(result.is_err());
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn rdns13_stale_aware_resolver_host_mapping_valid() {
        let config = DnsConfig {
            host_mapping: vec![("api.test".to_string(), "10.0.0.42".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let resolver =
            super::stale_resolver::StaleAwareDnsResolver::new(&config).unwrap();
        assert!(resolver.has_host_mapping("api.test"));
    }

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn rdns14_stale_aware_resolver_rejects_invalid_ip() {
        let config = DnsConfig {
            host_mapping: vec![("bad.test".to_string(), "not-an-ip".to_string())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let result = super::stale_resolver::StaleAwareDnsResolver::new(&config);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("not-an-ip"), "error should mention the bad IP: {msg}");
    }
}
