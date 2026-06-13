#[cfg(feature = "hickory-dns")]
pub(crate) use catcher_dns::reqwest_resolver::{build_reqwest_resolver, ReqwestDnsResolver};

#[cfg(test)]
mod tests {
    use crate::types::http::DnsConfig;

    #[cfg(feature = "hickory-dns")]
    #[test]
    fn reqwest_dns_resolver_builds_with_defaults() {
        let config = DnsConfig::default();
        let result = super::build_reqwest_resolver(&config);
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
        assert!(config.use_catcher_resolver());
    }
}
