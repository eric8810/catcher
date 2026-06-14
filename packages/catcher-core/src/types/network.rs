use serde::{Deserialize, Serialize};
use std::borrow::Cow;

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
    /// TLS SNI 主机名覆写。
    ///
    /// 仅纯 TS 路径（catcher-http-ts 的 Node Agent，通过 `servername`）支持。
    /// 原生 transport（catcher-http / catcher-ws）基于 reqwest，无法覆写 SNI 主机名，
    /// 设置此字段会在构建客户端时返回 `InvalidConfig` 错误（而非静默忽略）。详见 issue #028。
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

/// 代理模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProxyMode {
    /// 手动指定代理（现有行为）。
    #[default]
    Manual,
    /// 自动从 OS 系统代理检测。
    System,
}

/// 代理配置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProxyConfig {
    /// 代理模式。默认 Manual 以保持向后兼容。
    #[serde(default)]
    pub mode: ProxyMode,
    /// 代理地址，例如 `http://host:port`、`https://host:port`、`socks5://host:port`、`socks5h://host:port`。
    /// Manual 模式必填，System 模式忽略。
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<ProxyAuth>,
    #[serde(alias = "noProxy", default)]
    pub no_proxy: Vec<String>,
}

impl ProxyConfig {
    /// 返回协议库实际使用的代理地址。
    ///
    /// `socks5://` 会让 reqwest 在本地先解析目标域名。Clash fake-ip、VPN
    /// 分流和远端 DNS 场景下，这会把域名提前变成 IP，代理无法再按域名处理。
    /// 因此 Catcher 将 `socks5://` 统一按 `socks5h://` 处理，让代理解析目标域名。
    ///
    /// # Panics
    ///
    /// 当 `url` 为 `None` 时 panic。调用方应在调用前确保 proxy 已解析
    /// （System 模式在 `detect_system_proxy()` 后 url 一定为 Some）。
    pub fn transport_url(&self) -> Cow<'_, str> {
        let url = self
            .url
            .as_deref()
            .expect("transport_url: url is None; System proxy must be resolved via detect_system_proxy() before building the client");
        let Some((scheme, rest)) = url.split_once("://") else {
            return Cow::Borrowed(url);
        };
        if scheme.eq_ignore_ascii_case("socks5") {
            Cow::Owned(format!("socks5h://{rest}"))
        } else {
            Cow::Borrowed(url)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProxyConfig, ProxyMode};

    fn proxy_url(url: &str) -> String {
        ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some(url.to_string()),
            auth: None,
            no_proxy: Vec::new(),
        }
        .transport_url()
        .into_owned()
    }

    #[test]
    fn socks5_proxy_uses_remote_dns() {
        assert_eq!(
            proxy_url("socks5://127.0.0.1:7890"),
            "socks5h://127.0.0.1:7890"
        );
        assert_eq!(
            proxy_url("SOCKS5://127.0.0.1:7890"),
            "socks5h://127.0.0.1:7890"
        );
    }

    #[test]
    fn other_proxy_schemes_are_unchanged() {
        assert_eq!(
            proxy_url("socks5h://127.0.0.1:7890"),
            "socks5h://127.0.0.1:7890"
        );
        assert_eq!(proxy_url("http://127.0.0.1:7890"), "http://127.0.0.1:7890");
    }

    #[test]
    fn system_mode_defaults_to_manual() {
        let config: ProxyConfig = serde_json::from_str(r#"{"url":"http://proxy:8080"}"#).unwrap();
        assert_eq!(config.mode, ProxyMode::Manual);
        assert_eq!(config.url.as_deref(), Some("http://proxy:8080"));
    }

    #[test]
    fn system_mode_json_roundtrip() {
        let config = ProxyConfig {
            mode: ProxyMode::System,
            url: None,
            auth: None,
            no_proxy: vec!["localhost".into()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProxyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mode, ProxyMode::System);
        assert!(parsed.url.is_none());
    }
}
