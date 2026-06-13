use serde::{Deserialize, Serialize};

use super::default_true;

/// TLS 版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TlsVersion {
    Tls1_0,
    Tls1_1,
    Tls1_2,
    Tls1_3,
}

/// TLS 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// 是否验证服务端证书。
    #[serde(alias = "rejectUnauthorized", default = "default_true")]
    pub reject_unauthorized: bool,
    /// CA 证书 PEM。
    #[serde(alias = "caCertPem", skip_serializing_if = "Option::is_none")]
    pub ca_cert_pem: Option<String>,
    /// CA 证书文件路径。
    #[serde(alias = "caCertPath", skip_serializing_if = "Option::is_none")]
    pub ca_cert_path: Option<String>,
    /// 客户端证书 PEM。
    #[serde(alias = "clientCertPem", skip_serializing_if = "Option::is_none")]
    pub client_cert_pem: Option<String>,
    /// 客户端证书文件路径。
    #[serde(alias = "clientCertPath", skip_serializing_if = "Option::is_none")]
    pub client_cert_path: Option<String>,
    /// 客户端私钥 PEM。
    #[serde(alias = "clientKeyPem", skip_serializing_if = "Option::is_none")]
    pub client_key_pem: Option<String>,
    /// 客户端私钥文件路径。
    #[serde(alias = "clientKeyPath", skip_serializing_if = "Option::is_none")]
    pub client_key_path: Option<String>,
    /// PFX/PKCS12 客户端身份。
    #[serde(alias = "clientIdentityPfx", skip_serializing_if = "Option::is_none")]
    pub client_identity_pfx: Option<Vec<u8>>,
    /// PFX 身份密码。
    #[serde(
        alias = "clientIdentityPassword",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_identity_password: Option<String>,
    /// TLS SNI 覆写。
    #[serde(alias = "tlsSniOverride", skip_serializing_if = "Option::is_none")]
    pub tls_sni_override: Option<String>,
    /// 最低 TLS 版本。
    #[serde(alias = "minTlsVersion", skip_serializing_if = "Option::is_none")]
    pub min_tls_version: Option<TlsVersion>,
    /// 最高 TLS 版本。
    #[serde(alias = "maxTlsVersion", skip_serializing_if = "Option::is_none")]
    pub max_tls_version: Option<TlsVersion>,
    /// SHA-256 公钥指纹 pinning。
    #[serde(alias = "pinSha256", skip_serializing_if = "Option::is_none")]
    pub pin_sha256: Option<Vec<String>>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            reject_unauthorized: true,
            ca_cert_pem: None,
            ca_cert_path: None,
            client_cert_pem: None,
            client_cert_path: None,
            client_key_pem: None,
            client_key_path: None,
            client_identity_pfx: None,
            client_identity_password: None,
            tls_sni_override: None,
            min_tls_version: None,
            max_tls_version: None,
            pin_sha256: None,
        }
    }
}

/// 代理认证。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyAuth {
    pub username: String,
    pub password: String,
}

/// 代理配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// 代理地址，例如 `http://host:port`、`https://host:port`、`socks5://host:port`、`socks5h://host:port`。
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,
    #[serde(alias = "noProxy", default)]
    pub no_proxy: Vec<String>,
}
