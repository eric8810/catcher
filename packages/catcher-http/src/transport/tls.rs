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

    // CA 证书 — inline PEM
    if let Some(ref pem) = config.ca_cert_pem {
        let cert = reqwest::Certificate::from_pem(pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse CA cert PEM: {e}")))?;
        builder = builder.add_root_certificate(cert);
    }

    // CA 证书 — file path
    if let Some(ref path) = config.ca_cert_path {
        let pem_bytes = std::fs::read(path)
            .map_err(|e| CatcherError::TlsError(format!("read CA cert file {}: {e}", path)))?;
        let cert = reqwest::Certificate::from_pem(&pem_bytes)
            .map_err(|e| CatcherError::TlsError(format!("parse CA cert file {}: {e}", path)))?;
        builder = builder.add_root_certificate(cert);
    }

    // mTLS: 客户端证书 + 私钥 (inline PEM)
    if let (Some(ref cert_pem), Some(ref key_pem)) =
        (&config.client_cert_pem, &config.client_key_pem)
    {
        let identity_pem = format!("{cert_pem}\n{key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse client identity PEM: {e}")))?;
        builder = builder.identity(identity);
    }

    // mTLS: 客户端证书 + 私钥 (file paths)
    if let (Some(ref cert_path), Some(ref key_path)) =
        (&config.client_cert_path, &config.client_key_path)
    {
        let cert_pem = std::fs::read_to_string(cert_path)
            .map_err(|e| CatcherError::TlsError(format!("read client cert {}: {e}", cert_path)))?;
        let key_pem = std::fs::read_to_string(key_path)
            .map_err(|e| CatcherError::TlsError(format!("read client key {}: {e}", key_path)))?;
        let identity_pem = format!("{cert_pem}\n{key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes())
            .map_err(|e| CatcherError::TlsError(format!("parse client identity from files: {e}")))?;
        builder = builder.identity(identity);
    }

    // mTLS: PFX/PKCS12 identity
    // NOTE: reqwest with rustls-tls only supports PEM identity.
    // PFX/PKCS12 requires native-tls feature. When using rustls,
    // convert PFX to PEM first, or use client_cert_pem + client_key_pem instead.
    #[cfg(feature = "native-tls")]
    if let Some(ref pfx) = config.client_identity_pfx {
        let password = config.client_identity_password.as_deref().unwrap_or("");
        let identity = reqwest::Identity::from_pkcs12_der(pfx, password)
            .map_err(|e| CatcherError::TlsError(format!("parse PFX identity: {e}")))?;
        builder = builder.identity(identity);
    }

    // TLS version control
    if let Some(ref min) = config.min_tls_version {
        builder = builder.min_tls_version(match min {
            crate::types::http::TlsVersion::Tls1_0 => reqwest::tls::Version::TLS_1_0,
            crate::types::http::TlsVersion::Tls1_1 => reqwest::tls::Version::TLS_1_1,
            crate::types::http::TlsVersion::Tls1_2 => reqwest::tls::Version::TLS_1_2,
            crate::types::http::TlsVersion::Tls1_3 => reqwest::tls::Version::TLS_1_3,
        });
    }
    if let Some(ref max) = config.max_tls_version {
        builder = builder.max_tls_version(match max {
            crate::types::http::TlsVersion::Tls1_0 => reqwest::tls::Version::TLS_1_0,
            crate::types::http::TlsVersion::Tls1_1 => reqwest::tls::Version::TLS_1_1,
            crate::types::http::TlsVersion::Tls1_2 => reqwest::tls::Version::TLS_1_2,
            crate::types::http::TlsVersion::Tls1_3 => reqwest::tls::Version::TLS_1_3,
        });
    }

    // TLS SNI override
    // NOTE: reqwest 0.12 tls_sni() takes a bool (enable/disable SNI), not a hostname.
    // Custom SNI hostname override is not supported by reqwest directly.
    // Use tls_sni_override in config but only toggle SNI on/off here.
    if let Some(ref _sni) = config.tls_sni_override {
        // Keep SNI enabled (default behavior) — custom hostname not supported by reqwest
        builder = builder.tls_sni(true);
    }

    // NOTE: pin_sha256 is deferred — requires custom rustls ServerCertVerifier.
    // Will be implemented in a future version.

    Ok(builder)
}
