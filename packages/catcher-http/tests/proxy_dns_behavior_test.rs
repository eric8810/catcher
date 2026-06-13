use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use catcher_http::types::http::{
    DnsConfig, DnsMode, HttpClientConfig, HttpMethod, HttpRequest, ProxyConfig,
};
use catcher_http::HttpTransport;
use catcher_test_support::http_proxy::{HttpProxyProbe, HttpProxyRequest};
use catcher_test_support::socks5::{Socks5Address, Socks5Probe};

fn proxy_config(url: String) -> ProxyConfig {
    ProxyConfig {
        url,
        auth: None,
        no_proxy: Vec::new(),
    }
}

fn proxy_config_with_no_proxy(url: String, no_proxy: Vec<String>) -> ProxyConfig {
    ProxyConfig {
        url,
        auth: None,
        no_proxy,
    }
}

fn catcher_dns_mapping() -> DnsConfig {
    DnsConfig {
        mode: DnsMode::Catcher,
        host_mapping: HashMap::from([("example.com".to_string(), "127.0.0.1".to_string())]),
        ..Default::default()
    }
}

#[tokio::test]
async fn socks5_proxy_receives_domain_even_when_catcher_dns_is_enabled(
) -> Result<(), Box<dyn Error>> {
    let mut proxy = Socks5Probe::start().await?;
    let transport = HttpTransport::new(HttpClientConfig {
        proxy: Some(proxy_config(proxy.socks5_url())),
        dns: Some(catcher_dns_mapping()),
        connect_timeout_ms: 1_000,
        response_timeout_ms: 2_000,
        max_concurrency: 0,
        ..Default::default()
    })?;

    let request_task = tokio::spawn(async move {
        transport
            .execute(HttpRequest {
                method: HttpMethod::GET,
                url: "http://example.com/proxy-check".to_string(),
                timeout_ms: Some(2_000),
                ..Default::default()
            })
            .await
    });

    let connect = proxy.wait_for_connect().await?;
    assert_eq!(
        connect.address,
        Socks5Address::Domain("example.com".to_string())
    );
    assert_eq!(connect.port, 80);

    let response = request_task.await??;
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");

    Ok(())
}

#[tokio::test]
async fn http_no_proxy_bypasses_configured_proxy() -> Result<(), Box<dyn Error>> {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let mut proxy = HttpProxyProbe::start().await?;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("direct"))
        .mount(&server)
        .await;

    let transport = HttpTransport::new(HttpClientConfig {
        proxy: Some(proxy_config_with_no_proxy(
            proxy.http_url(),
            vec!["127.0.0.1".to_string()],
        )),
        connect_timeout_ms: 1_000,
        response_timeout_ms: 2_000,
        max_concurrency: 0,
        ..Default::default()
    })?;

    let response = transport
        .execute(HttpRequest {
            method: HttpMethod::GET,
            url: server.uri(),
            timeout_ms: Some(2_000),
            ..Default::default()
        })
        .await?;

    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"direct");
    proxy.assert_no_request(Duration::from_millis(200)).await?;

    Ok(())
}

#[tokio::test]
async fn http_proxy_connect_receives_domain_even_when_catcher_dns_is_enabled(
) -> Result<(), Box<dyn Error>> {
    let mut proxy = HttpProxyProbe::start().await?;
    let transport = HttpTransport::new(HttpClientConfig {
        proxy: Some(proxy_config(proxy.http_url())),
        dns: Some(catcher_dns_mapping()),
        connect_timeout_ms: 1_000,
        response_timeout_ms: 2_000,
        max_concurrency: 0,
        ..Default::default()
    })?;

    let request_task = tokio::spawn(async move {
        transport
            .execute(HttpRequest {
                method: HttpMethod::GET,
                url: "https://example.com/connect-check".to_string(),
                timeout_ms: Some(2_000),
                ..Default::default()
            })
            .await
    });

    let request = proxy.wait_for_request().await?;
    assert_eq!(
        request,
        HttpProxyRequest::Connect {
            authority: "example.com:443".to_string()
        }
    );

    let response_result = request_task.await?;
    assert!(
        response_result.is_err(),
        "fake HTTP proxy closes after CONNECT, so TLS should fail"
    );

    Ok(())
}
