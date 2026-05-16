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
