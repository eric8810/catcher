//! 系统代理自动检测。
//!
//! 从 OS 读取系统代理配置（macOS SystemConfiguration / Windows 注册表 / Linux 环境变量），
//! 转换为 catcher 的 [`ProxyConfig`]。
//!
//! 供 catcher-http 和 catcher-ws 共享使用。
//!
//! ## 已知限制
//!
//! - **macOS**: proxy-cfg 仅读取 HTTP/HTTPS/FTP 代理，不读取 SOCKS 代理。
//!   仅开启 SOCKS 的系统代理无法被检测到。
//! - **Linux**: 不读取 GNOME gsettings，仅读取环境变量和 /etc/sysconfig/proxy。
//! - **PAC/WPAD**: 不支持，需要 JS 引擎执行 PAC 脚本。

use catcher_core::types::network::ProxyConfig;
#[cfg(feature = "system-proxy")]
use catcher_core::types::network::ProxyMode;

/// 从操作系统读取系统代理配置。
///
/// 仅在 `system-proxy` feature 启用时编译。无代理或平台不支持时返回 `None`。
#[cfg(feature = "system-proxy")]
pub fn detect_system_proxy() -> Option<ProxyConfig> {
    let os = proxy_cfg::get_proxy_config().ok()??;

    // 优先级：https > http > * (all protocols)
    let url = os
        .proxies
        .get("https")
        .or_else(|| os.proxies.get("http"))
        .or_else(|| os.proxies.get("*"))?
        .clone();

    // 确保 URL 含 scheme 前缀。
    // Windows 注册表和某些 Linux 配置返回的代理地址不含 scheme。
    // 优先假定 http://（更通用），SOCKS 代理通常明确标注 socks5://。
    let url = if url.contains("://") {
        url
    } else {
        format!("http://{url}")
    };

    Some(ProxyConfig {
        mode: ProxyMode::Manual, // 内部已解析为具体 URL
        url: Some(url),
        auth: None,
        no_proxy: os.whitelist.into_iter().collect(),
    })
}

/// 无 `system-proxy` feature 时的桩实现，始终返回 `None`。
#[cfg(not(feature = "system-proxy"))]
pub fn detect_system_proxy() -> Option<ProxyConfig> {
    None
}

/// 重新检测系统代理，并在未检测到可用的固定代理时返回显式 Direct 配置。
///
/// `reqwest::ClientBuilder` 在未配置 proxy 时会自动读取环境/系统代理，因此不能用
/// `None` 表示 System 模式的直连回退。这里始终返回一个有效配置，确保调用方重建
/// client 后仍保持严格直连语义。
pub fn detect_system_proxy_or_direct(user_no_proxy: Vec<String>) -> ProxyConfig {
    system_proxy_or_direct(detect_system_proxy(), user_no_proxy)
}

fn system_proxy_or_direct(
    detected_proxy: Option<ProxyConfig>,
    user_no_proxy: Vec<String>,
) -> ProxyConfig {
    if let Some(mut proxy) = detected_proxy {
        for entry in user_no_proxy {
            if !proxy.no_proxy.contains(&entry) {
                proxy.no_proxy.push(entry);
            }
        }
        proxy
    } else {
        ProxyConfig {
            mode: catcher_core::types::network::ProxyMode::Direct,
            url: None,
            auth: None,
            no_proxy: user_no_proxy,
        }
    }
}

#[cfg(all(test, feature = "system-proxy"))]
mod tests {
    use super::*;

    #[test]
    fn detect_does_not_panic_without_proxy() {
        let _ = detect_system_proxy();
    }

    #[test]
    fn resolved_proxy_has_scheme() {
        if let Some(proxy) = detect_system_proxy() {
            let url = proxy.url.expect("resolved proxy must have url");
            assert!(
                url.contains("://"),
                "proxy URL should contain scheme prefix: {url}"
            );
        }
    }

    #[test]
    fn resolved_proxy_has_mode_manual() {
        if let Some(proxy) = detect_system_proxy() {
            assert_eq!(proxy.mode, ProxyMode::Manual);
        }
    }

    #[test]
    fn unresolved_system_proxy_becomes_explicit_direct() {
        let proxy = system_proxy_or_direct(None, vec!["localhost".into()]);

        assert_eq!(proxy.mode, ProxyMode::Direct);
        assert!(proxy.url.is_none());
        assert_eq!(proxy.no_proxy, vec!["localhost"]);
    }

    #[test]
    fn detected_system_proxy_merges_user_no_proxy() {
        let detected = ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some("http://127.0.0.1:7890".into()),
            auth: None,
            no_proxy: vec!["localhost".into()],
        };

        let proxy = system_proxy_or_direct(
            Some(detected),
            vec!["localhost".into(), "internal.example".into()],
        );

        assert_eq!(proxy.mode, ProxyMode::Manual);
        assert_eq!(
            proxy.no_proxy,
            vec!["localhost".to_string(), "internal.example".to_string()]
        );
    }
}
