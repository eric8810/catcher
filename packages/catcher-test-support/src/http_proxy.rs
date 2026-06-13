use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

/// HTTP 代理收到的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpProxyRequest {
    Connect { authority: String },
    Forward { method: String, uri: String },
}

/// 只接受一个连接的 HTTP 代理探针。
///
/// 它用于测试 HTTPS / WSS 是否通过 CONNECT 传递原始域名，以及
/// `no_proxy` 命中时是否完全没有连接代理。
pub struct HttpProxyProbe {
    addr: SocketAddr,
    request_rx: mpsc::Receiver<io::Result<HttpProxyRequest>>,
    task: JoinHandle<()>,
}

impl HttpProxyProbe {
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (request_tx, request_rx) = mpsc::channel(1);

        let task = tokio::spawn(async move {
            accept_one(listener, request_tx).await;
        });

        Ok(Self {
            addr,
            request_rx,
            task,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn http_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub async fn wait_for_request(&mut self) -> io::Result<HttpProxyRequest> {
        match tokio::time::timeout(Duration::from_secs(5), self.request_rx.recv()).await {
            Ok(Some(result)) => result,
            Ok(None) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "HTTP proxy probe task stopped before sending result",
            )),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for HTTP proxy request",
            )),
        }
    }

    pub async fn assert_no_request(&mut self, duration: Duration) -> io::Result<()> {
        match tokio::time::timeout(duration, self.request_rx.recv()).await {
            Ok(Some(Ok(request))) => Err(io::Error::other(format!(
                "unexpected HTTP proxy request: {request:?}"
            ))),
            Ok(Some(Err(err))) => Err(err),
            Ok(None) | Err(_) => Ok(()),
        }
    }
}

impl Drop for HttpProxyProbe {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn accept_one(listener: TcpListener, request_tx: mpsc::Sender<io::Result<HttpProxyRequest>>) {
    let result = async {
        let (mut stream, _) = listener.accept().await?;
        let request = read_http_proxy_request(&mut stream).await?;
        respond_to_proxy_request(&mut stream, &request).await?;
        Ok((stream, request))
    }
    .await;

    match result {
        Ok((mut stream, request)) => {
            let _ = request_tx.send(Ok(request)).await;
            let _ = tokio::time::timeout(Duration::from_millis(200), stream.read_u8()).await;
        }
        Err(err) => {
            let _ = request_tx.send(Err(err)).await;
        }
    }
}

async fn read_http_proxy_request(stream: &mut TcpStream) -> io::Result<HttpProxyRequest> {
    let mut bytes = Vec::with_capacity(1024);
    let mut buf = [0; 512];

    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "HTTP proxy connection closed before headers",
            ));
        }
        bytes.extend_from_slice(&buf[..n]);
        if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if bytes.len() > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "HTTP proxy request header too large",
            ));
        }
    }

    let text = String::from_utf8(bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid HTTP proxy request bytes: {e}"),
        )
    })?;
    let first_line = text
        .lines()
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing request line"))?;
    let mut parts = first_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing method"))?;
    let target = parts
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing target"))?;

    if method.eq_ignore_ascii_case("CONNECT") {
        Ok(HttpProxyRequest::Connect {
            authority: target.to_string(),
        })
    } else {
        Ok(HttpProxyRequest::Forward {
            method: method.to_string(),
            uri: target.to_string(),
        })
    }
}

async fn respond_to_proxy_request(
    stream: &mut TcpStream,
    request: &HttpProxyRequest,
) -> io::Result<()> {
    let response = match request {
        HttpProxyRequest::Connect { .. } => {
            b"HTTP/1.1 200 Connection Established\r\nProxy-Agent: catcher-test\r\n\r\n".as_slice()
        }
        HttpProxyRequest::Forward { .. } => {
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".as_slice()
        }
    };
    stream.write_all(response).await
}
