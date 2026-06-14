//! Clash 真实验证测试
//!
//! 前提: Clash Verge 运行在 127.0.0.1:7897 (mixed HTTP+SOCKS5)
//! 运行: cd packages && HTTPS_PROXY=socks5://127.0.0.1:7897 cargo test -p catcher-http --test clash_real_e2e --features system-proxy -- --nocapture

use catcher_core::types::network::{ProxyConfig, ProxyMode};

/// 检测 Clash 代理
#[test]
fn detect_clash_proxy_via_env() {
    let url = std::env::var("HTTPS_PROXY").unwrap_or_default();
    if url.is_empty() {
        eprintln!("[SKIP] HTTPS_PROXY not set");
        return;
    }
    eprintln!("[ENV] HTTPS_PROXY={url}");

    let proxy = catcher_dns::proxy::detect_system_proxy();
    assert!(proxy.is_some(), "should detect proxy from HTTPS_PROXY");
    let proxy = proxy.unwrap();
    eprintln!("[DETECT] url={:?} no_proxy={:?}", proxy.url, proxy.no_proxy);
    assert!(proxy.url.unwrap().contains("127.0.0.1"));
}

/// 通过 Clash 代理发 HTTP 请求
#[tokio::test]
async fn http_request_through_clash_proxy() {
    let proxy_url = std::env::var("HTTPS_PROXY").unwrap_or_default();
    if proxy_url.is_empty() {
        eprintln!("[SKIP] HTTPS_PROXY not set");
        return;
    }

    // 方式 A: 手动指定代理
    let config_manual = catcher_http::types::http::HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some(proxy_url.clone()),
            auth: None,
            no_proxy: vec![],
        }),
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };

    let transport = catcher_http::transport::HttpTransport::new(config_manual)
        .expect("build client with manual proxy");

    let resp = transport.get("/ip").await.expect("GET /ip via proxy");
    eprintln!("[MANUAL] status={} body={}", resp.status, String::from_utf8_lossy(&resp.body));
    assert_eq!(resp.status, 200);

    // 方式 B: System 模式
    let config_system = catcher_http::types::http::HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::System,
            url: None,
            auth: None,
            no_proxy: vec![],
        }),
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };

    let transport = catcher_http::transport::HttpTransport::new(config_system)
        .expect("build client with system proxy");

    // System 模式首次构建时 url=None → 内部跳过代理
    // networkChanged() 后重新检测到代理
    transport.network_changed().expect("networkChanged");

    let resp = transport.get("/ip").await;
    match resp {
        Ok(r) => {
            eprintln!("[SYSTEM] status={} body={}", r.status, String::from_utf8_lossy(&r.body));
            assert_eq!(r.status, 200);
        }
        Err(e) => {
            // System 模式首次请求可能直连（url=None 跳过代理构建），
            // networkChanged 后重建的 client 才带代理
            eprintln!("[SYSTEM-first] error={e} (expected if first build had no proxy)");

            // 再调一次 networkChanged + 请求
            transport.network_changed().expect("networkChanged again");
            let resp2 = transport.get("/ip").await.expect("GET /ip after 2nd networkChanged");
            eprintln!("[SYSTEM-retry] status={} body={}", resp2.status, String::from_utf8_lossy(&resp2.body));
            assert_eq!(resp2.status, 200);
        }
    }
}

/// 对比直连 vs 代理 IP
#[tokio::test]
async fn compare_direct_vs_proxy_ip() {
    let proxy_url = std::env::var("HTTPS_PROXY").unwrap_or_default();
    if proxy_url.is_empty() {
        eprintln!("[SKIP] HTTPS_PROXY not set");
        return;
    }

    // 直连
    let config_direct = catcher_http::types::http::HttpClientConfig {
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };
    let t_direct = catcher_http::transport::HttpTransport::new(config_direct).unwrap();
    let ip_direct = String::from_utf8_lossy(&t_direct.get("/ip").await.unwrap().body).to_string();

    // 代理
    let config_proxy = catcher_http::types::http::HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some(proxy_url),
            auth: None,
            no_proxy: vec![],
        }),
        base_url: "https://httpbin.org".into(),
        ..Default::default()
    };
    let t_proxy = catcher_http::transport::HttpTransport::new(config_proxy).unwrap();
    let ip_proxy = String::from_utf8_lossy(&t_proxy.get("/ip").await.unwrap().body).to_string();

    eprintln!("[DIRECT] {ip_direct}");
    eprintln!("[PROXY]  {ip_proxy}");

    // 代理 IP 应该不同于直连 IP（除非代理节点和本地同出口）
    // 不强断言，只打印对比
}
