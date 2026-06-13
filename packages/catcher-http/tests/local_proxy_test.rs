use catcher_http::types::http::{HttpClientConfig, HttpMethod, HttpRequest, ProxyConfig};
use catcher_http::HttpTransport;

fn target_url() -> String {
    std::env::var("CATCHER_PROXY_TEST_HTTP_URL")
        .unwrap_or_else(|_| "https://example.com/".to_string())
}

async fn assert_http_via_proxy(proxy_url: String) {
    let transport = HttpTransport::new(HttpClientConfig {
        proxy: Some(ProxyConfig {
            url: proxy_url,
            auth: None,
            no_proxy: Vec::new(),
        }),
        connect_timeout_ms: 10_000,
        response_timeout_ms: 20_000,
        ..Default::default()
    })
    .expect("create HTTP transport");

    let response = transport
        .execute(HttpRequest {
            method: HttpMethod::GET,
            url: target_url(),
            ..Default::default()
        })
        .await
        .expect("HTTP request through proxy");

    assert!(
        (200..400).contains(&response.status),
        "unexpected status {}",
        response.status
    );
}

#[tokio::test]
#[ignore = "requires a local HTTP proxy, for example CATCHER_TEST_HTTP_PROXY=http://127.0.0.1:7890"]
async fn local_http_proxy_reaches_https_endpoint() {
    let proxy_url = std::env::var("CATCHER_TEST_HTTP_PROXY")
        .unwrap_or_else(|_| "http://127.0.0.1:7890".to_string());
    assert_http_via_proxy(proxy_url).await;
}

#[tokio::test]
#[ignore = "requires a local SOCKS proxy, for example CATCHER_TEST_SOCKS5H_PROXY=socks5h://127.0.0.1:7890"]
async fn local_socks5h_proxy_reaches_https_endpoint() {
    let proxy_url = std::env::var("CATCHER_TEST_SOCKS5H_PROXY")
        .unwrap_or_else(|_| "socks5h://127.0.0.1:7890".to_string());
    assert_http_via_proxy(proxy_url).await;
}

#[tokio::test]
#[ignore = "requires a local SOCKS proxy, for example CATCHER_TEST_SOCKS5_PROXY=socks5://127.0.0.1:7890"]
async fn local_socks5_proxy_reaches_https_endpoint() {
    let proxy_url = std::env::var("CATCHER_TEST_SOCKS5_PROXY")
        .unwrap_or_else(|_| "socks5://127.0.0.1:7890".to_string());
    assert_http_via_proxy(proxy_url).await;
}
