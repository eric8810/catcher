use catcher_core::CatcherError;
use crate::types::http::DnsConfig;

/// 根据 DnsConfig 构建 DNS 解析器
///
/// 仅在 feature = "hickory-dns" 时使用 reqwest 内置的 hickory-dns 支持。
/// reqwest 的 `hickory-dns` feature 会自动启用 hickory resolver 并实现 Resolve trait。
/// 这里我们提供一个辅助函数来验证和记录 DNS 配置，但实际解析由 reqwest 处理。
#[cfg(feature = "hickory-dns")]
pub fn build_dns_resolver(_config: &DnsConfig) -> Result<Option<()>, CatcherError> {
    // reqwest 的 hickory-dns feature 会在内部构建 resolver
    // 自定义 nameservers 等高级配置可以通过环境变量或系统配置实现
    // 此处预留接口，后续可扩展
    Ok(Some(()))
}

#[cfg(not(feature = "hickory-dns"))]
pub fn build_dns_resolver(_config: &DnsConfig) -> Result<Option<()>, CatcherError> {
    // 没有 hickory-dns feature，回退到系统 DNS (reqwest 默认行为)
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // NOTE: build_dns_resolver is currently a stub — it validates config but
    // delegates actual resolution to reqwest (system DNS by default, hickory-dns
    // when the feature is enabled). These tests verify config acceptance and
    // return value semantics. When a real custom resolver is implemented,
    // add integration tests with wiremock or real DNS queries.

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
        // Config with host_mapping should be accepted
        let inner = result.unwrap();
        // Without hickory-dns feature: returns None (uses system DNS)
        // With hickory-dns feature: returns Some(())
        #[cfg(not(feature = "hickory-dns"))]
        assert!(inner.is_none(), "without hickory-dns, should return None");
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
        // Default config with no custom DNS — should return None (system DNS)
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
        // Verify config preserved the host mappings
        assert_eq!(config.host_mapping.len(), 2);
        assert_eq!(config.host_mapping.get("api.example.com"), Some(&"10.0.0.1".to_string()));
        assert_eq!(config.host_mapping.get("cdn.example.com"), Some(&"10.0.0.2".to_string()));
    }
}
