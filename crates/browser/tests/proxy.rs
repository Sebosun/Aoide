//! The browser's proxy, over loopback: what it lets through and what it refuses.

use aoide_browser::proxy::Proxy;
use aoide_fetch::Reach;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A server that answers every connection with its request line, then closes.
async fn echo_server() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 4096];
                let mut got = Vec::new();
                while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = stream.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    got.extend_from_slice(&buffer[..n]);
                }
                let head = String::from_utf8_lossy(&got).into_owned();
                let body = head.replace("\r\n", "|");
                let answer = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(answer.as_bytes()).await.unwrap();
            });
        }
    });
    port
}

async fn ask(proxy: &Proxy, request: &str) -> String {
    let mut stream = TcpStream::connect(proxy.address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).await.unwrap();
    String::from_utf8_lossy(&answer).into_owned()
}

#[tokio::test]
async fn plain_requests_are_sent_on_to_allowed_addresses_one_per_connection() {
    let port = echo_server().await;
    let proxy = Proxy::start(Reach::AnyAddress).await.unwrap();
    let answer = ask(
        &proxy,
        &format!(
            "GET http://127.0.0.1:{port}/page?x=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
             Proxy-Connection: keep-alive\r\nAccept: text/html\r\n\r\n"
        ),
    )
    .await;
    assert!(answer.starts_with("HTTP/1.1 200 OK"), "{answer}");
    assert!(answer.contains("GET /page?x=1 HTTP/1.1|"), "{answer}");
    assert!(answer.contains("Accept: text/html|"));
    assert!(answer.contains("Connection: close|"));
    assert!(!answer.to_lowercase().contains("proxy-connection"));
}

#[tokio::test]
async fn tunnels_carry_anything_to_allowed_addresses() {
    let port = echo_server().await;
    let proxy = Proxy::start(Reach::AnyAddress).await.unwrap();
    let mut stream = TcpStream::connect(proxy.address).await.unwrap();
    stream
        .write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut head = [0u8; 39];
    stream.read_exact(&mut head).await.unwrap();
    assert_eq!(&head[..], b"HTTP/1.1 200 Connection Established\r\n\r\n");
    stream
        .write_all(b"HEAD /inside HTTP/1.1\r\n\r\n")
        .await
        .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).await.unwrap();
    assert!(answer.contains("HEAD /inside HTTP/1.1|"), "{answer}");
}

#[tokio::test]
async fn under_public_reach_this_computer_and_its_network_are_refused() {
    let port = echo_server().await;
    let proxy = Proxy::start(Reach::Public).await.unwrap();
    for target in [
        format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\n"),
        format!("CONNECT localhost:{port} HTTP/1.1\r\n\r\n"),
        "CONNECT [::1]:443 HTTP/1.1\r\n\r\n".to_owned(),
        "CONNECT 192.168.1.1:443 HTTP/1.1\r\n\r\n".to_owned(),
        format!("GET http://127.0.0.1:{port}/ HTTP/1.1\r\n\r\n"),
        "GET http://10.0.0.8/admin HTTP/1.1\r\n\r\n".to_owned(),
    ] {
        let answer = ask(&proxy, &target).await;
        assert!(answer.starts_with("HTTP/1.1 403"), "{target}: {answer}");
    }
    // Nonsense is a bad request, not a connection.
    let answer = ask(&proxy, "GET /relative HTTP/1.1\r\n\r\n").await;
    assert!(answer.starts_with("HTTP/1.1 400"), "{answer}");
}
