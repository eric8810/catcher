use catcher_core::CatcherError;
use crate::types::http::TlsConfig;
use reqwest::ClientBuilder;

/// 将 TlsConfig 应用到 reqwest ClientBuilder
///
/// reqwest 的 ClientBuilder 方法取 ownership (builder pattern)，
/// 所以每次配置后需要重新赋值。
pub fn build_tls_config(
    mut builder: ClientBuilder,
    config: &TlsConfig,
) -> Result<ClientBuilder, CatcherError> {
    if !config.reject_unauthorized {
        // ⚠️ 仅测试/开发环境使用
        builder = builder.danger_accept_invalid_certs(true);
    }

    // CA 证书 (PEM 直接传入)
    if let Some(ref pem) = config.ca_cert_pem {
        let cert = reqwest::Certificate::from_pem(pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse CA cert: {e}")))?;
        builder = builder.add_root_certificate(cert);
    }

    // 客户端证书 + 私钥 (PEM 直接传入)
    if let (Some(ref cert_pem), Some(ref key_pem)) =
        (&config.client_cert_pem, &config.client_key_pem)
    {
        let identity_pem = format!("{cert_pem}\n{key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse client identity: {e}")))?;
        builder = builder.identity(identity);
    }

    Ok(builder)
}
