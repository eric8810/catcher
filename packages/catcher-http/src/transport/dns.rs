// ── reqwest DNS 适配器 ──

#[cfg(feature = "hickory-dns")]
mod reqwest_resolver {
    use std::net::SocketAddr;
    use std::sync::Arc;

    use catcher_core::CatcherError;
    use catcher_dns::{
        build_stale_aware_resolver as build_shared_resolver, DnsConfig, DnsResolver,
    };

    #[derive(Clone, Debug)]
    pub(crate) struct ReqwestDnsResolver {
        inner: Arc<DnsResolver>,
    }

    impl ReqwestDnsResolver {
        /// 清空 DNS 缓存（网络环境变化后调用）。
        pub(crate) fn clear_cache(&self) {
            self.inner.clear_cache();
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

    pub(crate) fn build_stale_aware_resolver(
        config: &DnsConfig,
    ) -> Result<Arc<ReqwestDnsResolver>, CatcherError> {
        Ok(Arc::new(ReqwestDnsResolver {
            inner: build_shared_resolver(config)?,
        }))
    }
}

#[cfg(feature = "hickory-dns")]
pub(crate) use reqwest_resolver::{build_stale_aware_resolver, ReqwestDnsResolver};

#[cfg(test)]
mod tests {
    use crate::types::http::DnsConfig;

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn reqwest_dns_resolver_builds_with_defaults() {
        let config = DnsConfig::default();
        let result = super::build_stale_aware_resolver(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn http_dns_config_reexport_keeps_defaults() {
        let config: DnsConfig = serde_json::from_str("{}").expect("valid empty JSON");
        assert_eq!(config.cache_size, 512);
        assert_eq!(config.cache_ttl_secs, 300);
        assert_eq!(config.negative_ttl_secs, 60);
        assert_eq!(config.stale_ttl_secs, 3600);
        assert!(config.stale_on_error);
    }
}
