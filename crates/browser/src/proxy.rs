//! The browser's only way out: an HTTP proxy on a loopback port that connects only where
//! [`Reach`] allows. It resolves each name itself and connects to an address it checked, so a
//! page, a frame, a worker or a name that resolves differently the second time cannot reach
//! this computer or its network. HTTPS (and WebSockets) pass through as `CONNECT` tunnels;
//! plain HTTP requests are sent on with `Connection: close`, one per connection.

use std::net::SocketAddr;

use aoide_fetch::{public, Reach};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use url::Url;

/// The longest request head accepted.
const MAX_HEAD: usize = 32 * 1024;

/// A running proxy; it stops when dropped.
pub struct Proxy {
    pub address: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Proxy {
    /// Starts a proxy on a loopback port the system picks.
    pub async fn start(reach: Reach) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let task = tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                tokio::spawn(async move {
                    if let Err(error) = serve(client, reach).await {
                        tracing::debug!(event = "browser.proxy", %error, "A proxied connection ended");
                    }
                });
            }
        });
        Ok(Self { address, task })
    }
}

async fn serve(mut client: TcpStream, reach: Reach) -> std::io::Result<()> {
    let (head, rest) = read_head(&mut client).await?;
    let mut lines = head.split("\r\n");
    let request = lines.next().unwrap_or_default();
    let mut parts = request.split(' ');
    let (method, target, version) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or("HTTP/1.1"),
    );
    if method.eq_ignore_ascii_case("CONNECT") {
        let Some((host, port)) = split_authority(target) else {
            return refuse(&mut client, 400).await;
        };
        let Some(mut server) = connect(&host, port, reach).await else {
            return refuse(&mut client, 403).await;
        };
        client
            .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            .await?;
        server.write_all(&rest).await?;
        tokio::io::copy_bidirectional(&mut client, &mut server).await?;
        return Ok(());
    }
    // A plain request names the whole address: `GET http://host/path HTTP/1.1`.
    let Some(url) = Url::parse(target).ok().filter(|u| u.scheme() == "http") else {
        return refuse(&mut client, 400).await;
    };
    let (Some(host), Some(port)) = (url.host_str(), url.port_or_known_default()) else {
        return refuse(&mut client, 400).await;
    };
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let Some(mut server) = connect(&host, port, reach).await else {
        return refuse(&mut client, 403).await;
    };
    let path = &url[url::Position::BeforePath..url::Position::AfterQuery];
    let mut out = format!("{method} {path} {version}\r\n");
    for line in lines.filter(|l| !l.is_empty()) {
        let name = line.split(':').next().unwrap_or_default().trim();
        let dropped = [
            "connection",
            "proxy-connection",
            "proxy-authorization",
            "keep-alive",
        ];
        if !dropped.iter().any(|d| name.eq_ignore_ascii_case(d)) {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    out.push_str("Connection: close\r\n\r\n");
    server.write_all(out.as_bytes()).await?;
    server.write_all(&rest).await?;
    tokio::io::copy_bidirectional(&mut client, &mut server).await?;
    Ok(())
}

/// The request head as text, and whatever was read past it.
async fn read_head(client: &mut TcpStream) -> std::io::Result<(String, Vec<u8>)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            let rest = buffer.split_off(end + 4);
            buffer.truncate(end);
            return Ok((String::from_utf8_lossy(&buffer).into_owned(), rest));
        }
        if buffer.len() > MAX_HEAD {
            return Err(std::io::Error::other("request head too large"));
        }
        let read = client.read(&mut chunk).await?;
        if read == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

/// `host:port` (with `[v6]` brackets removed).
fn split_authority(authority: &str) -> Option<(String, u16)> {
    let (host, port) = authority.rsplit_once(':')?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    Some((host.to_owned(), port.parse().ok()?)).filter(|(h, _)| !h.is_empty())
}

/// A connection to `host`'s first allowed address; `None` when it has none.
async fn connect(host: &str, port: u16, reach: Reach) -> Option<TcpStream> {
    let name = host.trim_end_matches('.').to_ascii_lowercase();
    if reach == Reach::Public && (name == "localhost" || name.ends_with(".localhost")) {
        return None;
    }
    let addresses = tokio::net::lookup_host((host, port)).await.ok()?;
    for address in addresses.filter(|a| reach == Reach::AnyAddress || public(a.ip())) {
        if let Ok(stream) = TcpStream::connect(address).await {
            return Some(stream);
        }
    }
    None
}

async fn refuse(client: &mut TcpStream, status: u16) -> std::io::Result<()> {
    let reason = if status == 403 {
        "Forbidden"
    } else {
        "Bad Request"
    };
    let answer =
        format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    client.write_all(answer.as_bytes()).await
}
