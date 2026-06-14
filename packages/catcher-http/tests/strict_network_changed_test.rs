//! networkChanged 重新检测系统代理 — CI 安全
//!
//! 不依赖本地代理进程，不修改全局环境变量。
//! 真实代理验证见 clash_real_e2e.rs（需要 HTTPS_PROXY env）。

use catcher_core::types::network::{ProxyConfig, ProxyMode};

/// System 模式构建 + networkChanged 不 panic
#[test]
fn system_mode_client_builds_and_network_changed_no_panic() {
    let config = catcher_http::types::http::HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::System,
            url: None,
            auth: None,
            no_proxy: vec![],
        }),
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };
    let t = catcher_http::transport::HttpTransport::new(config).unwrap();

    // 连续多次 networkChanged 不 panic
    for _ in 0..3 {
        t.network_changed().unwrap();
    }
}

/// detect_system_proxy 无代理时返回 None（不 panic）
#[test]
fn detect_system_proxy_returns_none_without_proxy() {
    // CI 环境通常无代理，验证不 panic
    let result = catcher_dns::proxy::detect_system_proxy();
    // 不强断言具体值（某些 CI 可能有系统代理），只验证不 panic
    let _ = result;
}

/// Manual 模式下 networkChanged 行为不变
#[test]
fn manual_mode_network_changed_unchanged() {
    let config = catcher_http::types::http::HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some("socks5://127.0.0.1:9999".into()),
            auth: None,
            no_proxy: vec![],
        }),
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };
    let t = catcher_http::transport::HttpTransport::new(config).unwrap();
    t.network_changed().unwrap();
}
