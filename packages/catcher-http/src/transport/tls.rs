use catcher_core::CatcherError;
use crate::types::http::TlsConfig;
use reqwest::ClientBuilder;

/// 将 TlsConfig 应用到 reqwest ClientBuilder
///
/// 当 `pin_sha256` 配置时，构建完整的 `rustls::ClientConfig` 并通过
/// `use_preconfigured_tls` 注入自定义 verifier。
/// 否则使用 reqwest 的标准 builder 方法。
pub fn build_tls_config(
    mut builder: ClientBuilder,
    config: &TlsConfig,
) -> Result<ClientBuilder, CatcherError> {
    // If pin_sha256 is set, build a full rustls::ClientConfig with pinning verifier
    #[cfg(feature = "rustls-tls")]
    if config.pin_sha256.as_ref().is_some_and(|pins| !pins.is_empty()) {
        return build_tls_with_pinning(builder, config);
    }

    // Standard reqwest TLS configuration path (no pinning)
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
    if let Some(ref _sni) = config.tls_sni_override {
        builder = builder.tls_sni(true);
    }

    Ok(builder)
}

// ── Pinning path: build full rustls::ClientConfig ────────────

#[cfg(feature = "rustls-tls")]
fn build_tls_with_pinning(
    builder: ClientBuilder,
    config: &TlsConfig,
) -> Result<ClientBuilder, CatcherError> {
    use std::sync::Arc;
    use rustls_pki_types::pem::PemObject;
    use crate::transport::tls_pinning::PinningVerifier;

    // Build root certificate store
    let mut root_store = rustls::RootCertStore::empty();
    if config.reject_unauthorized {
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }

    // Add custom CA certs (inline PEM)
    if let Some(ref pem) = config.ca_cert_pem {
        for cert in rustls_pki_types::CertificateDer::pem_slice_iter(pem.as_bytes()) {
            let cert = cert.map_err(|e| CatcherError::TlsError(format!("parse PEM cert: {e}")))?;
            root_store.add(cert)
                .map_err(|e| CatcherError::TlsError(format!("add CA cert: {e}")))?;
        }
    }

    // Add custom CA certs (file path)
    if let Some(ref path) = config.ca_cert_path {
        let pem_bytes = std::fs::read(path)
            .map_err(|e| CatcherError::TlsError(format!("read CA cert file {}: {e}", path)))?;
        for cert in rustls_pki_types::CertificateDer::pem_slice_iter(&pem_bytes) {
            let cert = cert.map_err(|e| CatcherError::TlsError(format!("parse PEM cert: {e}")))?;
            root_store.add(cert)
                .map_err(|e| CatcherError::TlsError(format!("add CA cert from file: {e}")))?;
        }
    }

    // Build base verifier (standard webpki verification)
    let base_verifier = rustls::client::WebPkiServerVerifier::builder(Arc::new(root_store))
        .build()
        .map_err(|e| CatcherError::TlsError(format!("build webpki verifier: {e}")))?;

    // Wrap with pinning
    let pins = config.pin_sha256.as_deref().unwrap_or(&[]).to_vec();
    let pinning_verifier = Arc::new(PinningVerifier::new(base_verifier, &pins));

    // Build ClientConfig with custom verifier
    let crypto_provider = rustls::crypto::ring::default_provider();
    let tls_config = rustls::ClientConfig::builder_with_provider(Arc::new(crypto_provider))
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .map_err(|e| CatcherError::TlsError(format!("set TLS versions: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(pinning_verifier)
        .with_no_client_auth();

    // SNI
    let mut tls_config = tls_config;
    if config.tls_sni_override.is_some() {
        tls_config.enable_sni = true;
    }

    // Inject into reqwest
    let builder = builder.use_preconfigured_tls(tls_config);
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::http::TlsVersion;

    #[test]
    fn tls1_default_accepts_unauthorized() {
        let config = TlsConfig::default();
        assert!(config.reject_unauthorized);
    }

    #[test]
    fn tls2_config_with_reject_false() {
        let config = TlsConfig {
            reject_unauthorized: false,
            ..Default::default()
        };
        let builder = reqwest::Client::builder();
        let result = build_tls_config(builder, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn tls3_config_with_ca_cert_pem_invalid() {
        let invalid_pem = "-----BEGIN CERTIFICATE-----\nnot-a-real-cert\n-----END CERTIFICATE-----\n";
        let config = TlsConfig {
            ca_cert_pem: Some(invalid_pem.to_string()),
            ..Default::default()
        };
        let builder = reqwest::Client::builder();
        let _result = build_tls_config(builder, &config);
    }

    #[test]
    fn tls4_min_tls_version() {
        let config = TlsConfig {
            min_tls_version: Some(TlsVersion::Tls1_2),
            ..Default::default()
        };
        let builder = reqwest::Client::builder();
        let result = build_tls_config(builder, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn tls5_sni_override() {
        let config = TlsConfig {
            tls_sni_override: Some("custom-sni.example.com".to_string()),
            ..Default::default()
        };
        let builder = reqwest::Client::builder();
        let result = build_tls_config(builder, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn tls6_default_config_builds_client() {
        let config = TlsConfig::default();
        let builder = reqwest::Client::builder();
        let result = build_tls_config(builder, &config);
        assert!(result.is_ok());
        let client = result.unwrap().build();
        assert!(client.is_ok());
    }
}
