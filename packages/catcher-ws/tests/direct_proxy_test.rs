use std::error::Error;
use std::time::Duration;

use catcher_core::types::network::ProxyMode;
use catcher_ws::{ProxyConfig, WsClientConfig, WsEvent, WsTransport};
use futures_util::StreamExt;
use tokio::time::timeout;

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
async fn direct_mode_bypasses_automatic_proxy_environment() -> Result<(), Box<dyn Error>> {
    install_unreachable_proxy_env();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _ = timeout(Duration::from_secs(2), ws.next()).await;
    });

    let config = WsClientConfig {
        urls: vec![format!("ws://127.0.0.1:{port}")],
        proxy: Some(ProxyConfig {
            mode: ProxyMode::Direct,
            url: None,
            auth: None,
            no_proxy: Vec::new(),
        }),
        handshake_timeout_ms: 2_000,
        ..Default::default()
    };

    let (handle, mut events) = WsTransport::connect(&config).await?;
    let event = timeout(Duration::from_secs(2), events.recv())
        .await?
        .ok_or("WebSocket event channel closed")?;
    assert!(matches!(event, WsEvent::Connected { .. }));

    let _ = handle.close(1000, "direct mode test");
    let _ = server.await;
    Ok(())
}

/// 未启用 system-proxy feature 时，System 检测必然无结果。networkChanged() 重连
/// 仍须显式回退 Direct，不能让 reqwest 重新读取不可达的代理环境变量。
#[cfg(not(feature = "system-proxy"))]
#[tokio::test]
async fn unresolved_system_mode_stays_direct_after_network_change() -> Result<(), Box<dyn Error>> {
    install_unreachable_proxy_env();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let mut connections = Vec::new();
        for _ in 0..2 {
            let (stream, _) = listener.accept().await.unwrap();
            connections.push(tokio::spawn(async move {
                let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
                let _ = timeout(Duration::from_secs(2), ws.next()).await;
            }));
        }
        for connection in connections {
            let _ = connection.await;
        }
    });

    let config = WsClientConfig {
        urls: vec![format!("ws://127.0.0.1:{port}")],
        proxy: Some(ProxyConfig {
            mode: ProxyMode::System,
            url: None,
            auth: None,
            no_proxy: Vec::new(),
        }),
        handshake_timeout_ms: 2_000,
        ..Default::default()
    };

    let (handle, mut events) = WsTransport::connect(&config).await?;
    let event = timeout(Duration::from_secs(2), events.recv())
        .await?
        .ok_or("WebSocket event channel closed")?;
    assert!(matches!(event, WsEvent::Connected { .. }));

    handle.network_changed()?;
    timeout(Duration::from_secs(3), async {
        loop {
            match events.recv().await {
                Some(WsEvent::Connected { .. }) => break,
                Some(_) => continue,
                None => panic!("WebSocket event channel closed before reconnect"),
            }
        }
    })
    .await?;

    let _ = handle.close(1000, "system fallback direct test");
    let _ = server.await;
    Ok(())
}
