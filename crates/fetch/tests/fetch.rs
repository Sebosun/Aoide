//! The fetcher over loopback HTTP with synthetic responses (`Reach::AnyAddress`), and its
//! refusal of local addresses under `Reach::Public`.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_fetch::{FetchError, FetchOptions, HttpFetcher, Reach, Via};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use url::Url;

#[derive(Clone)]
struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    delay: Duration,
}

fn html(content_type: &str, body: &[u8]) -> Reply {
    Reply {
        status: 200,
        headers: vec![("Content-Type", content_type.into())],
        body: body.to_vec(),
        delay: Duration::ZERO,
    }
}

fn redirect(status: u16, location: &str) -> Reply {
    Reply {
        status,
        headers: vec![("Location", location.into())],
        body: Vec::new(),
        delay: Duration::ZERO,
    }
}

struct Server {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn at(&self, path: &str) -> Url {
        Url::parse(&format!("{}{path}", self.url)).unwrap()
    }
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

/// Answers by path; records request targets in order.
async fn serve(routes: Vec<(&'static str, Reply)>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let routes: HashMap<&str, Reply> = routes.into_iter().collect();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let routes = routes.clone();
            let log = Arc::clone(&log);
            tokio::spawn(async move {
                let mut buffer = vec![0; 8192];
                let n = stream.read(&mut buffer).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buffer[..n]).to_string();
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                log.lock().unwrap().push(target.clone());
                let reply = routes.get(target.as_str()).cloned().unwrap_or(Reply {
                    status: 404,
                    headers: vec![("Content-Type", "text/html".into())],
                    body: b"<p>missing</p>".to_vec(),
                    delay: Duration::ZERO,
                });
                tokio::time::sleep(reply.delay).await;
                let mut out = format!("HTTP/1.1 {} X\r\n", reply.status);
                for (name, value) in &reply.headers {
                    out.push_str(&format!("{name}: {value}\r\n"));
                }
                out.push_str(&format!(
                    "Content-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.body.len()
                ));
                let _ = stream.write_all(out.as_bytes()).await;
                let _ = stream.write_all(&reply.body).await;
            });
        }
    });
    Server { url, seen }
}

fn options() -> FetchOptions {
    FetchOptions {
        reach: Reach::AnyAddress,
        ..FetchOptions::default()
    }
}

fn fetcher(options: FetchOptions) -> HttpFetcher {
    HttpFetcher::new(options).unwrap()
}

#[tokio::test]
async fn a_page_is_kept_as_it_was_served() {
    let page = "<html><body><p>The tide tables for May.</p></body></html>";
    let server = serve(vec![(
        "/tides",
        html("text/html; charset=utf-8", page.as_bytes()),
    )])
    .await;
    let snapshot = fetcher(options())
        .fetch(&server.at("/tides"))
        .await
        .unwrap();
    assert_eq!(snapshot.requested, server.at("/tides"));
    assert_eq!(snapshot.final_url, server.at("/tides"));
    assert_eq!(snapshot.status, 200);
    assert_eq!(snapshot.html, page);
    assert_eq!(snapshot.via, Via::Http);
    assert!(snapshot.fetched_at_ms > 1_700_000_000_000);

    let xhtml = html("application/xhtml+xml", page.as_bytes());
    let server = serve(vec![("/x", xhtml)]).await;
    assert!(fetcher(options()).fetch(&server.at("/x")).await.is_ok());
}

#[tokio::test]
async fn redirects_are_followed_one_hop_at_a_time_up_to_the_limit() {
    let server = serve(vec![
        ("/a", redirect(301, "/b")),
        ("/b", redirect(302, "c?n=1")),
        ("/c?n=1", html("text/html", b"<p>Arrived.</p>")),
        ("/loop", redirect(307, "/loop")),
    ])
    .await;
    let snapshot = fetcher(options()).fetch(&server.at("/a")).await.unwrap();
    assert_eq!(snapshot.final_url, server.at("/c?n=1"));
    assert_eq!(snapshot.requested, server.at("/a"));
    assert_eq!(server.seen(), ["/a", "/b", "/c?n=1"]);

    let limited = FetchOptions {
        max_redirects: 2,
        ..options()
    };
    match fetcher(limited).fetch(&server.at("/loop")).await {
        Err(FetchError::Refused(why)) => assert!(why.contains("redirect"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(server.seen().iter().filter(|t| *t == "/loop").count(), 3);
}

#[tokio::test]
async fn each_failure_has_its_own_error() {
    let mut slow = html("text/html", b"<p>late</p>");
    slow.delay = Duration::from_millis(800);
    let server = serve(vec![
        ("/pdf", html("application/pdf", b"%PDF-1.7")),
        ("/plain", html("text/plain", b"just text")),
        ("/big", html("text/html", &vec![b'a'; 4096])),
        ("/slow", slow),
        (
            "/gone",
            Reply {
                status: 410,
                ..html("text/html", b"<p>gone</p>")
            },
        ),
        ("/away", redirect(302, "ftp://files.example/x")),
    ])
    .await;
    let quick = FetchOptions {
        timeout: Duration::from_millis(300),
        max_bytes: 1024,
        ..options()
    };
    let fetch = |path: &'static str| {
        let fetcher = fetcher(quick.clone());
        let url = server.at(path);
        async move { fetcher.fetch(&url).await }
    };
    assert!(matches!(fetch("/pdf").await, Err(FetchError::NotHtml(t)) if t == "application/pdf"));
    assert!(matches!(fetch("/plain").await, Err(FetchError::NotHtml(_))));
    assert!(matches!(fetch("/big").await, Err(FetchError::TooLarge)));
    assert!(matches!(fetch("/slow").await, Err(FetchError::Timeout)));
    assert!(matches!(fetch("/gone").await, Err(FetchError::Status(410))));
    assert!(matches!(fetch("/away").await, Err(FetchError::Refused(_))));

    let mailto = Url::parse("mailto:someone@example.com").unwrap();
    assert!(matches!(
        fetcher(options()).fetch(&mailto).await,
        Err(FetchError::Refused(_))
    ));

    // Nothing listening.
    let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    let url = Url::parse(&format!("http://{address}/")).unwrap();
    assert!(matches!(
        fetcher(options()).fetch(&url).await,
        Err(FetchError::Transport(_))
    ));
}

#[tokio::test]
async fn the_charset_comes_from_the_header_then_the_page() {
    // "café" in Latin-1 and in Windows-1252, with a curly quote that only 1252 has.
    let latin1 = b"<html><body><p>caf\xe9</p></body></html>";
    let declared = b"<html><head><meta charset=\"windows-1252\"></head><body><p>\x93caf\xe9\x94</p></body></html>";
    let equiv = b"<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=ISO-8859-1\"></head><body>caf\xe9</body></html>";
    let server = serve(vec![
        ("/header", html("text/html; charset=ISO-8859-1", latin1)),
        ("/meta", html("text/html", declared)),
        ("/equiv", html("text/html", equiv)),
        ("/bad", html("text/html", b"<p>caf\xe9</p>")),
    ])
    .await;
    let get = |path: &'static str| {
        let fetcher = fetcher(options());
        let url = server.at(path);
        async move { fetcher.fetch(&url).await.unwrap().html }
    };
    assert!(get("/header").await.contains("<p>café</p>"));
    assert!(get("/meta").await.contains("\u{201c}café\u{201d}"));
    assert!(get("/equiv").await.contains("café"));
    assert!(get("/bad").await.contains("caf\u{fffd}"));
}

#[tokio::test]
async fn public_reach_refuses_local_addresses_before_connecting() {
    let server = serve(vec![("/", html("text/html", b"<p>local</p>"))]).await;
    let public = fetcher(FetchOptions::default());
    for url in [
        server.at("/"),
        Url::parse("http://localhost:9/").unwrap(),
        Url::parse("http://[::1]:9/").unwrap(),
        Url::parse("http://10.0.0.1/").unwrap(),
        Url::parse("http://192.168.1.1/").unwrap(),
        Url::parse("http://169.254.169.254/latest/").unwrap(),
        Url::parse("http://0.0.0.0/").unwrap(),
        Url::parse("http://[fd00::1]/").unwrap(),
        Url::parse("http://[::ffff:127.0.0.1]/").unwrap(),
    ] {
        match public.fetch(&url).await {
            Err(FetchError::Refused(_)) => {}
            other => panic!("{url}: {other:?}"),
        }
    }
    assert!(server.seen().is_empty());
}
