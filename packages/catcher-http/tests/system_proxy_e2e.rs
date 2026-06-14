//! E2E 验证：system-proxy 功能
//!
//! 运行: cd packages && cargo test -p catcher-http --test system_proxy_e2e --features system-proxy -- --nocapture
//!
//! 真实代理场景手动验证（Linux）:
//!   HTTPS_PROXY=socks5://127.0.0.1:7890 cargo test -p catcher-http --test system_proxy_e2e --features system-proxy -- e2e_env --nocapture

use catcher_core::types::network::{ProxyConfig, ProxyMode};

/// 测试 1: 无代理环境不 panic
#[test]
fn e2e_no_proxy_does_not_panic() {
    let result = catcher_dns::proxy::detect_system_proxy();
    if let Some(proxy) = &result {
        eprintln!("[INFO] OS reports proxy: {:?}", proxy.url);
    }
    // 不强断言，只验证不 panic
}

/// 测试 2: HTTPS_PROXY 环境变量检测
/// 需从 shell 传入: HTTPS_PROXY=socks5://127.0.0.1:7890 cargo test ...
#[test]
fn e2e_env() {
    let proxy_url = std::env::var("HTTPS_PROXY").ok();
    if proxy_url.is_none() {
        eprintln!("[SKIP] HTTPS_PROXY not set");
        return;
    }

    let proxy = catcher_dns::proxy::detect_system_proxy();
    assert!(proxy.is_some(), "should detect proxy from env");
    let proxy = proxy.unwrap();
    eprintln!("[INFO] Detected proxy: {:?}", proxy.url);
    assert_eq!(proxy.mode, ProxyMode::Manual, "resolved proxy should be Manual");
}

/// 测试 3: JSON 序列化往返 + 向后兼容
#[test]
fn e2e_json_roundtrip_and_backward_compat() {
    // System mode
    let config = ProxyConfig { mode: ProxyMode::System, url: None, auth: None, no_proxy: vec![] };
    let json = serde_json::to_string(&config).unwrap();
    eprintln!("[INFO] System mode JSON: {json}");
    let parsed: ProxyConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.mode, ProxyMode::System);
    assert!(parsed.url.is_none());

    // 旧格式兼容
    let old_json = r#"{"url":"http://old-proxy:8080"}"#;
    let old: ProxyConfig = serde_json::from_str(old_json).unwrap();
    assert_eq!(old.mode, ProxyMode::Manual);
    assert_eq!(old.url.as_deref(), Some("http://old-proxy:8080"));
}

/// 测试 4: System 模式 client 构建不 panic + networkChanged 不 panic
#[tokio::test]
async fn e2e_system_mode_client_builds_and_network_changed_works() {
    let config = catcher_http::types::http::HttpClientConfig {
        proxy: Some(ProxyConfig { mode: ProxyMode::System, url: None, auth: None, no_proxy: vec![] }),
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };

    let transport = catcher_http::transport::HttpTransport::new(config);
    assert!(transport.is_ok(), "should build with System proxy (url=None): {:?}", transport.err());
    let transport = transport.unwrap();

    // networkChanged 不 panic
    transport.network_changed().expect("networkChanged should work");

    // 请求可以发出（无代理时直连）
    let resp = transport.get("/get").await;
    match resp {
        Ok(r) => { eprintln!("[INFO] GET /get → {}", r.status); assert_eq!(r.status, 200); }
        Err(e) => eprintln!("[WARN] Request failed (may be no network): {e}"),
    }
}
