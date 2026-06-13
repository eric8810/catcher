use catcher_ws::{ProxyConfig, WsClientConfig, WsEvent, WsTransport};
use tokio::time::{timeout, Duration};

fn target_url() -> String {
    std::env::var("CATCHER_PROXY_TEST_WS_URL")
        .unwrap_or_else(|_| "wss://ws.postman-echo.com/raw".to_string())
}

async fn assert_ws_via_proxy(proxy_url: String) {
    let config = WsClientConfig {
        urls: vec![target_url()],
        proxy: Some(ProxyConfig {
            url: proxy_url,
            auth: None,
            no_proxy: Vec::new(),
        }),
        handshake_timeout_ms: 20_000,
        ..Default::default()
    };

    let (handle, mut events) = WsTransport::connect(&config)
        .await
        .expect("connect WebSocket through proxy");
    let event = timeout(Duration::from_secs(20), events.recv())
        .await
        .expect("wait for connected event")
        .expect("connected event");

    assert!(
        matches!(event, WsEvent::Connected { .. }),
        "unexpected event: {event:?}"
    );

    let _ = handle.close(1000, "local proxy test");
}

#[tokio::test]
#[ignore = "requires a local HTTP proxy, for example CATCHER_TEST_HTTP_PROXY=http://127.0.0.1:7890"]
async fn local_http_proxy_connects_websocket() {
    let proxy_url = std::env::var("CATCHER_TEST_HTTP_PROXY")
        .unwrap_or_else(|_| "http://127.0.0.1:7890".to_string());
    assert_ws_via_proxy(proxy_url).await;
}

#[tokio::test]
#[ignore = "requires a local SOCKS proxy, for example CATCHER_TEST_SOCKS5H_PROXY=socks5h://127.0.0.1:7890"]
async fn local_socks5h_proxy_connects_websocket() {
    let proxy_url = std::env::var("CATCHER_TEST_SOCKS5H_PROXY")
        .unwrap_or_else(|_| "socks5h://127.0.0.1:7890".to_string());
    assert_ws_via_proxy(proxy_url).await;
}

#[tokio::test]
#[ignore = "requires a local SOCKS proxy, for example CATCHER_TEST_SOCKS5_PROXY=socks5://127.0.0.1:7890"]
async fn local_socks5_proxy_connects_websocket() {
    let proxy_url = std::env::var("CATCHER_TEST_SOCKS5_PROXY")
        .unwrap_or_else(|_| "socks5://127.0.0.1:7890".to_string());
    assert_ws_via_proxy(proxy_url).await;
}
