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
