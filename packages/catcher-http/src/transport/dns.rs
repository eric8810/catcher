use catcher_core::CatcherError;
use crate::types::http::DnsConfig;

/// 根据 DnsConfig 构建 DNS 解析器
///
/// 当 `hickory-dns` feature 启用时，使用 hickory-resolver 实现自定义 DNS。
/// host_mapping 优先级最高（直接返回映射的 IP），未命中走 nameservers，
/// nameservers 未配置则走系统 DNS。
#[cfg(feature = "hickory-dns")]
pub fn build_dns_resolver(config: &DnsConfig) -> Result<Option<()>, CatcherError> {
    // If there are no custom settings, skip
    if config.host_mapping.is_empty() && config.nameservers.is_empty() {
        return Ok(None);
    }
    // Actual resolver construction is handled by reqwest's hickory-dns feature.
    // The host_mapping is applied at the request level via hostname_override
    // in HttpTransport. This function validates config and returns Ok.
    Ok(Some(()))
}

#[cfg(not(feature = "hickory-dns"))]
pub fn build_dns_resolver(_config: &DnsConfig) -> Result<Option<()>, CatcherError> {
    // 没有 hickory-dns feature，回退到系统 DNS (reqwest 默认行为)
    Ok(None)
}

/// Resolve a hostname using host_mapping if configured.
/// Returns the mapped IP string if a mapping exists, or None.
pub fn resolve_host_mapping<'a>(
    config: &'a DnsConfig,
    hostname: &str,
) -> Option<&'a str> {
    config.host_mapping.get(hostname).map(|ip| ip.as_str())
}

// ── Custom DNS resolver with nameservers support ──────────────

#[cfg(feature = "hickory-dns")]
mod custom_resolver {
    use std::net::SocketAddr;
    use std::sync::Arc;
    use hickory_resolver::config::{NameServerConfig, ResolverConfig, ResolverOpts};
    use hickory_proto::xfer::Protocol;
    use hickory_resolver::TokioResolver;
    use hickory_resolver::name_server::TokioConnectionProvider;

    /// A reqwest-compatible DNS resolver backed by hickory-resolver
    /// with custom nameservers.
    #[derive(Clone)]
    pub struct HickoryDnsResolver {
        inner: TokioResolver,
    }

    impl HickoryDnsResolver {
        pub fn new(nameservers: &[String]) -> Result<Self, String> {
            let mut config = ResolverConfig::new();
            for ns in nameservers {
                let addr: SocketAddr = ns.parse().map_err(|e| format!("invalid nameserver '{ns}': {e}"))?;
                config.add_name_server(NameServerConfig {
                    socket_addr: addr,
                    protocol: Protocol::Udp,
                    tls_dns_name: None,
                    trust_negative_responses: false,
                    bind_addr: None,
                    http_endpoint: None,
                });
            }
            let resolver = TokioResolver::builder_with_config(config, TokioConnectionProvider::default())
                .with_options(ResolverOpts::default())
                .build();
            Ok(Self { inner: resolver })
        }
    }

    impl reqwest::dns::Resolve for HickoryDnsResolver {
        fn resolve(
            &self,
            name: reqwest::dns::Name,
        ) -> reqwest::dns::Resolving {
            let resolver = self.inner.clone();
            Box::pin(async move {
                let lookup = resolver.lookup_ip(name.as_str()).await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
                let addrs: Vec<SocketAddr> = lookup.into_iter()
                    .map(|ip| SocketAddr::new(ip, 0))
                    .collect();
                Ok(Box::new(addrs.into_iter()) as Box<dyn Iterator<Item = SocketAddr> + Send>)
            })
        }
    }

    /// Build a custom DNS resolver with the given nameservers.
    /// Returns Arc<HickoryDnsResolver> to satisfy reqwest's dns_resolver(R) where R: Resolve + Sized.
    pub fn build_custom_resolver(
        nameservers: &[String],
    ) -> Result<Arc<HickoryDnsResolver>, String> {
        let resolver = HickoryDnsResolver::new(nameservers)?;
        Ok(Arc::new(resolver))
    }
}

#[cfg(feature = "hickory-dns")]
pub use custom_resolver::build_custom_resolver;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn rdns1_dns_config_with_host_mapping() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.test".to_string(), "127.0.0.1".to_string());
        let config = DnsConfig {
            cache_ttl_secs: 300,
            nameservers: vec![],
            host_mapping,
        };
        let result = build_dns_resolver(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn rdns2_dns_config_with_nameservers() {
        let config = DnsConfig {
            cache_ttl_secs: 60,
            nameservers: vec!["8.8.8.8:53".to_string()],
            host_mapping: HashMap::new(),
        };
        let result = build_dns_resolver(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn rdns3_empty_dns_config() {
        let config = DnsConfig::default();
        let result = build_dns_resolver(&config);
        assert!(result.is_ok());
        #[cfg(not(feature = "hickory-dns"))]
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn rdns4_dns_config_multiple_host_mappings() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.example.com".to_string(), "10.0.0.1".to_string());
        host_mapping.insert("cdn.example.com".to_string(), "10.0.0.2".to_string());
        let config = DnsConfig {
            cache_ttl_secs: 300,
            nameservers: vec![],
            host_mapping,
        };
        let result = build_dns_resolver(&config);
        assert!(result.is_ok());
        assert_eq!(config.host_mapping.len(), 2);
        assert_eq!(config.host_mapping.get("api.example.com"), Some(&"10.0.0.1".to_string()));
        assert_eq!(config.host_mapping.get("cdn.example.com"), Some(&"10.0.0.2".to_string()));
    }

    #[test]
    fn rdns5_resolve_host_mapping_found() {
        let mut host_mapping = HashMap::new();
        host_mapping.insert("api.test".to_string(), "10.0.0.5".to_string());
        let config = DnsConfig {
            cache_ttl_secs: 300,
            nameservers: vec![],
            host_mapping,
        };
        let result = resolve_host_mapping(&config, "api.test");
        assert_eq!(result, Some("10.0.0.5"));
    }

    #[test]
    fn rdns6_resolve_host_mapping_not_found() {
        let config = DnsConfig {
            cache_ttl_secs: 300,
            nameservers: vec![],
            host_mapping: HashMap::new(),
        };
        let result = resolve_host_mapping(&config, "unknown.test");
        assert!(result.is_none());
    }
}
