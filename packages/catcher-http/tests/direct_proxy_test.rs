use std::error::Error;

use catcher_core::types::network::{ProxyConfig, ProxyMode};
use catcher_http::types::http::{HttpClientConfig, HttpMethod, HttpRequest};
use catcher_http::HttpTransport;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn install_unreachable_proxy_env() {
    // SAFETY: 此集成测试单独运行在自己的测试进程中，不会与其他测试线程竞争环境变量。
    unsafe {
        for name in [
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
        ] {
            std::env::set_var(name, "http://127.0.0.1:9");
        }
        std::env::remove_var("no_proxy");
        std::env::remove_var("NO_PROXY");
    }
}

#[tokio::test]
async fn direct_mode_bypasses_proxy_environment_after_network_change() -> Result<(), Box<dyn Error>>
{
    install_unreachable_proxy_env();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .mount(&server)
        .await;

    let transport = HttpTransport::new(HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::Direct,
            url: None,
            auth: None,
            no_proxy: Vec::new(),
        }),
        connect_timeout_ms: 1_000,
        response_timeout_ms: 2_000,
        max_concurrency: 0,
        ..Default::default()
    })?;

    let response = transport
        .execute(HttpRequest {
            method: HttpMethod::GET,
            url: format!("{}/before-network-change", server.uri()),
            timeout_ms: Some(2_000),
            ..Default::default()
        })
        .await?;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"direct");

    transport.network_changed()?;
    let response = transport
        .execute(HttpRequest {
            method: HttpMethod::GET,
            url: format!("{}/after-network-change", server.uri()),
            timeout_ms: Some(2_000),
            ..Default::default()
        })
        .await?;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"direct");
    Ok(())
}

/// 未启用 system-proxy feature 时，System 检测必然无结果。networkChanged() 仍须
/// 显式回退 Direct，不能让 reqwest 重新读取不可达的代理环境变量。
#[cfg(not(feature = "system-proxy"))]
#[tokio::test]
async fn unresolved_system_mode_stays_direct_after_network_change() -> Result<(), Box<dyn Error>> {
    install_unreachable_proxy_env();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("system-fallback-direct"))
        .mount(&server)
        .await;

    let transport = HttpTransport::new(HttpClientConfig {
        proxy: Some(ProxyConfig {
            mode: ProxyMode::System,
            url: None,
            auth: None,
            no_proxy: Vec::new(),
        }),
        connect_timeout_ms: 1_000,
        response_timeout_ms: 2_000,
        max_concurrency: 0,
        ..Default::default()
    })?;

    transport.network_changed()?;
    let response = transport
        .execute(HttpRequest {
            method: HttpMethod::GET,
            url: format!("{}/after-network-change", server.uri()),
            timeout_ms: Some(2_000),
            ..Default::default()
        })
        .await?;

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"system-fallback-direct");
    Ok(())
}
