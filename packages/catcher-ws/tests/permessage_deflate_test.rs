use base64::Engine;
use catcher_ws::{WsClientConfig, WsEvent, WsTransport};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

fn header_value(request: &str, name: &str) -> Option<String> {
    request.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.eq_ignore_ascii_case(name) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn sec_websocket_accept(key: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(key.as_bytes());
    hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, std::io::Error> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..n]);
        if request.windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&request).to_string())
}

#[tokio::test]
async fn permessage_deflate_negotiates_and_sets_rsv1_on_data_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (server_tx, server_rx) = oneshot::channel::<Result<(String, u8), String>>();

    tokio::spawn(async move {
        let result = async {
            let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let request = read_http_request(&mut stream)
                .await
                .map_err(|e| e.to_string())?;

            let extension_offer = header_value(&request, "Sec-WebSocket-Extensions")
                .ok_or_else(|| "missing Sec-WebSocket-Extensions".to_string())?;
            let key = header_value(&request, "Sec-WebSocket-Key")
                .ok_or_else(|| "missing Sec-WebSocket-Key".to_string())?;
            let accept = sec_websocket_accept(&key);

            let response = format!(
                "HTTP/1.1 101 Switching Protocols\r\n\
Upgrade: websocket\r\n\
Connection: Upgrade\r\n\
Sec-WebSocket-Accept: {accept}\r\n\
Sec-WebSocket-Extensions: permessage-deflate\r\n\
\r\n"
            );
            stream
                .write_all(response.as_bytes())
                .await
                .map_err(|e| e.to_string())?;

            let mut frame_header = [0_u8; 2];
            stream
                .read_exact(&mut frame_header)
                .await
                .map_err(|e| e.to_string())?;

            Ok((extension_offer, frame_header[0]))
        }
        .await;

        let _ = server_tx.send(result);
    });

    let config = WsClientConfig {
        urls: vec![format!("ws://{addr}/ws")],
        per_message_deflate: true,
        ..Default::default()
    };

    let (handle, mut events) = WsTransport::connect(&config).await.unwrap();
    let event = timeout(Duration::from_secs(3), events.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, WsEvent::Connected { .. }));

    handle
        .send_text(&"permessage-deflate alignment ".repeat(128))
        .unwrap();

    let (extension_offer, first_frame_byte) = timeout(Duration::from_secs(3), server_rx)
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert!(extension_offer.contains("permessage-deflate"));
    assert_eq!(first_frame_byte & 0x0f, 0x01);
    assert_eq!(first_frame_byte & 0x40, 0x40);

    let _ = handle.close(1000, "done");
}
