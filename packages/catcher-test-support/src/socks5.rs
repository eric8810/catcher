use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

/// SOCKS5 代理收到的目标地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Socks5Address {
    Domain(String),
    Ip(IpAddr),
}

/// SOCKS5 CONNECT 请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Socks5Connect {
    pub address: Socks5Address,
    pub port: u16,
}

/// 只接受一个连接的 SOCKS5 探针。
///
/// 它用于测试客户端是否把目标域名交给代理，而不是在本地提前解析成 IP。
pub struct Socks5Probe {
    addr: SocketAddr,
    connect_rx: Option<oneshot::Receiver<io::Result<Socks5Connect>>>,
    task: JoinHandle<()>,
}

impl Socks5Probe {
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let (connect_tx, connect_rx) = oneshot::channel();

        let task = tokio::spawn(async move {
            accept_one(listener, connect_tx).await;
        });

        Ok(Self {
            addr,
            connect_rx: Some(connect_rx),
            task,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn socks5_url(&self) -> String {
        format!("socks5://{}", self.addr)
    }

    pub async fn wait_for_connect(&mut self) -> io::Result<Socks5Connect> {
        let Some(connect_rx) = self.connect_rx.take() else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "SOCKS5 connect result already consumed",
            ));
        };

        match tokio::time::timeout(Duration::from_secs(5), connect_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "SOCKS5 probe task stopped before sending result",
            )),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for SOCKS5 connect request",
            )),
        }
    }
}

impl Drop for Socks5Probe {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn accept_one(listener: TcpListener, connect_tx: oneshot::Sender<io::Result<Socks5Connect>>) {
    let result = async {
        let (mut stream, _) = listener.accept().await?;
        let connect = read_socks5_connect(&mut stream).await?;
        send_socks5_success(&mut stream).await?;
        Ok((stream, connect))
    }
    .await;

    match result {
        Ok((mut stream, connect)) => {
            let _ = connect_tx.send(Ok(connect));
            let _ = serve_one_http_like_response(&mut stream).await;
        }
        Err(err) => {
            let _ = connect_tx.send(Err(err));
        }
    }
}

async fn read_socks5_connect(stream: &mut TcpStream) -> io::Result<Socks5Connect> {
    let version = stream.read_u8().await?;
    if version != 0x05 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SOCKS version is not 5",
        ));
    }

    let methods_len = stream.read_u8().await? as usize;
    let mut methods = vec![0; methods_len];
    stream.read_exact(&mut methods).await?;
    if !methods.contains(&0x00) {
        stream.write_all(&[0x05, 0xff]).await?;
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "SOCKS5 client did not offer no-auth method",
        ));
    }
    stream.write_all(&[0x05, 0x00]).await?;

    let version = stream.read_u8().await?;
    let command = stream.read_u8().await?;
    let _reserved = stream.read_u8().await?;
    let address_type = stream.read_u8().await?;

    if version != 0x05 || command != 0x01 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "SOCKS5 request is not CONNECT",
        ));
    }

    let address = match address_type {
        0x01 => {
            let mut octets = [0; 4];
            stream.read_exact(&mut octets).await?;
            Socks5Address::Ip(IpAddr::V4(Ipv4Addr::from(octets)))
        }
        0x03 => {
            let len = stream.read_u8().await? as usize;
            let mut bytes = vec![0; len];
            stream.read_exact(&mut bytes).await?;
            let domain = String::from_utf8(bytes).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid SOCKS5 domain bytes: {e}"),
                )
            })?;
            Socks5Address::Domain(domain)
        }
        0x04 => {
            let mut octets = [0; 16];
            stream.read_exact(&mut octets).await?;
            Socks5Address::Ip(IpAddr::V6(Ipv6Addr::from(octets)))
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported SOCKS5 address type",
            ))
        }
    };

    let mut port_bytes = [0; 2];
    stream.read_exact(&mut port_bytes).await?;
    let port = u16::from_be_bytes(port_bytes);

    Ok(Socks5Connect { address, port })
}

async fn send_socks5_success(stream: &mut TcpStream) -> io::Result<()> {
    stream
        .write_all(&[0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0, 0])
        .await
}

async fn serve_one_http_like_response(stream: &mut TcpStream) -> io::Result<()> {
    let mut buf = [0; 2048];
    let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buf)).await;
    let n = match read {
        Ok(Ok(n)) => n,
        Ok(Err(err)) => return Err(err),
        Err(_) => return Ok(()),
    };

    if n == 0 {
        return Ok(());
    }

    let request = String::from_utf8_lossy(&buf[..n]);
    let response = if request.to_ascii_lowercase().contains("upgrade: websocket") {
        b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".as_slice()
    } else {
        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok".as_slice()
    };

    stream.write_all(response).await
}
