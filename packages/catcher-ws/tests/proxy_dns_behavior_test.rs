use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

use catcher_test_support::http_proxy::{HttpProxyProbe, HttpProxyRequest};
use catcher_test_support::socks5::{Socks5Address, Socks5Probe};
use catcher_ws::{DnsConfig, DnsMode, ProxyConfig, WsClientConfig, WsTransport};
use futures_util::StreamExt;
use tokio::time::timeout;

fn proxy_config(url: String) -> ProxyConfig {
    ProxyConfig {
        url: Some(url),
        auth: None,
        no_proxy: Vec::new(),
        ..Default::default()
    }
}

fn proxy_config_with_no_proxy(url: String, no_proxy: Vec<String>) -> ProxyConfig {
    ProxyConfig {
        url: Some(url),
        auth: None,
        no_proxy,
        ..Default::default()
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
    let config = WsClientConfig {
        urls: vec!["ws://example.com/socket".to_string()],
        proxy: Some(proxy_config(proxy.socks5_url())),
        dns: Some(catcher_dns_mapping()),
        handshake_timeout_ms: 2_000,
        ..Default::default()
    };

    let connect_task = tokio::spawn(async move { WsTransport::connect(&config).await });

    let connect = proxy.wait_for_connect().await?;
    assert_eq!(
        connect.address,
        Socks5Address::Domain("example.com".to_string())
    );
    assert_eq!(connect.port, 80);

    let connect_result = connect_task.await?;
    assert!(
        connect_result.is_err(),
        "fake SOCKS5 probe intentionally rejects the WebSocket handshake"
    );

    Ok(())
}

#[tokio::test]
async fn ws_no_proxy_bypasses_configured_proxy() -> Result<(), Box<dyn Error>> {
    let mut proxy = HttpProxyProbe::start().await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _ = timeout(Duration::from_secs(2), ws.next()).await;
    });

    let config = WsClientConfig {
        urls: vec![format!("ws://127.0.0.1:{port}")],
        proxy: Some(proxy_config_with_no_proxy(
            proxy.http_url(),
            vec!["127.0.0.1".to_string()],
        )),
        handshake_timeout_ms: 2_000,
        ..Default::default()
    };

    let (handle, mut events) = WsTransport::connect(&config).await?;
    let event = timeout(Duration::from_secs(2), events.recv())
        .await?
        .expect("connected event");
    assert!(
        matches!(event, catcher_ws::WsEvent::Connected { .. }),
        "unexpected event: {event:?}"
    );
    let _ = handle.close(1000, "no proxy test");

    proxy.assert_no_request(Duration::from_millis(200)).await?;
    server.await?;

    Ok(())
}

#[tokio::test]
async fn http_proxy_connect_receives_domain_for_wss_even_when_catcher_dns_is_enabled(
) -> Result<(), Box<dyn Error>> {
    let mut proxy = HttpProxyProbe::start().await?;
    let config = WsClientConfig {
        urls: vec!["wss://example.com/socket".to_string()],
        proxy: Some(proxy_config(proxy.http_url())),
        dns: Some(catcher_dns_mapping()),
        handshake_timeout_ms: 2_000,
        ..Default::default()
    };

    let connect_task = tokio::spawn(async move { WsTransport::connect(&config).await });

    let request = proxy.wait_for_request().await?;
    assert_eq!(
        request,
        HttpProxyRequest::Connect {
            authority: "example.com:443".to_string()
        }
    );

    let connect_result = connect_task.await?;
    assert!(
        connect_result.is_err(),
        "fake HTTP proxy closes after CONNECT, so WSS should fail"
    );

    Ok(())
}
