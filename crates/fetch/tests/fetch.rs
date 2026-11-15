//! The fetcher over loopback HTTP with synthetic responses (`Reach::AnyAddress`), and its
//! refusal of local addresses under `Reach::Public`.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_fetch::{FeedResponse, FetchError, FetchOptions, HttpFetcher, Reach, Validators, Via};
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
    /// Each request's head, lowercased.
    heads: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn at(&self, path: &str) -> Url {
        Url::parse(&format!("{}{path}", self.url)).unwrap()
    }
    fn seen(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
    fn last_head(&self) -> String {
        self.heads
            .lock()
            .unwrap()
            .last()
            .cloned()
            .unwrap_or_default()
    }
}

/// Answers by path; records request targets in order.
async fn serve(routes: Vec<(&'static str, Reply)>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let routes: HashMap<&str, Reply> = routes.into_iter().collect();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    let heads = Arc::new(Mutex::new(Vec::new()));
    let head_log = Arc::clone(&heads);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let routes = routes.clone();
            let log = Arc::clone(&log);
            let head_log = Arc::clone(&head_log);
            tokio::spawn(async move {
                let mut buffer = vec![0; 8192];
                let n = stream.read(&mut buffer).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buffer[..n]).to_string();
                let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                log.lock().unwrap().push(target.clone());
                head_log.lock().unwrap().push(head.to_lowercase());
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
    Server { url, seen, heads }
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

#[tokio::test]
async fn an_image_is_fetched_within_its_own_limit() {
    let mut big = html("image/png", &vec![1; 2048]);
    big.headers = vec![("Content-Type", "image/png".into())];
    let server = serve(vec![
        ("/lead.jpg", html("image/jpeg", b"\xff\xd8jpeg")),
        ("/big.png", big),
        ("/page", html("text/html", b"<p>not an image</p>")),
        ("/moved", redirect(301, "/lead.jpg")),
    ])
    .await;
    let fetcher = fetcher(options());
    let image = fetcher.image(&server.at("/moved"), 1024).await.unwrap();
    assert_eq!(image.media_type, "image/jpeg");
    assert_eq!(image.bytes, b"\xff\xd8jpeg");
    assert!(matches!(
        fetcher.image(&server.at("/big.png"), 1024).await,
        Err(FetchError::TooLarge)
    ));
    assert!(matches!(
        fetcher.image(&server.at("/page"), 1024).await,
        Err(FetchError::NotHtml(t)) if t == "text/html"
    ));
}

#[tokio::test]
async fn the_archive_names_its_closest_copy_of_a_page() {
    let found = r#"{"url":"https://news.example/ferry","archived_snapshots":{"closest":{"status":"200","available":true,"url":"http://web.archive.org/web/20250102030405/https://news.example/ferry","timestamp":"20250102030405"}}}"#;
    let server = serve(vec![
        (
            "/wayback/available?url=https%3A%2F%2Fnews.example%2Fferry",
            html("application/json", found.as_bytes()),
        ),
        (
            "/wayback/available?url=https%3A%2F%2Fnews.example%2Fnone",
            html("application/json", br#"{"archived_snapshots":{}}"#),
        ),
    ])
    .await;
    let options = FetchOptions {
        archive: format!("{}/wayback/available", server.url),
        ..options()
    };
    let fetcher = fetcher(options);
    let copy = fetcher
        .archived(&Url::parse("https://news.example/ferry").unwrap())
        .await
        .unwrap()
        .unwrap();
    // The capture as it was served, without the archive's own page around it.
    assert_eq!(
        copy.as_str(),
        "https://web.archive.org/web/20250102030405id_/https://news.example/ferry"
    );
    let none = fetcher
        .archived(&Url::parse("https://news.example/none").unwrap())
        .await;
    assert_eq!(none, Ok(None));
}

#[tokio::test]
async fn a_page_that_only_refreshes_to_another_is_followed() {
    // A static site's moved page: a script and a `<meta http-equiv="refresh">` to the new place.
    let moved = br#"<!doctype html><title>Redirect</title>
<script>window.location.replace("/new/");</script>
<noscript><meta http-equiv="Refresh" content="0; URL='/new/'"></noscript>
<p><a href="/new/">Click here</a> to be redirected.</p>"#;
    let slow = br#"<meta http-equiv="refresh" content="300; url=/new/"><p>Live scores.</p>"#;
    let server = serve(vec![
        ("/old.html", html("text/html", moved)),
        ("/new/", html("text/html", b"<p>The article itself.</p>")),
        ("/scores", html("text/html", slow)),
        (
            "/again",
            html(
                "text/html",
                br#"<meta http-equiv="refresh" content="0;url=/again">"#,
            ),
        ),
    ])
    .await;
    let snapshot = fetcher(options())
        .fetch(&server.at("/old.html"))
        .await
        .unwrap();
    assert_eq!(snapshot.requested, server.at("/old.html"));
    assert_eq!(snapshot.final_url, server.at("/new/"));
    assert_eq!(snapshot.html, "<p>The article itself.</p>");

    // A page that reloads itself later is the page; one that refreshes to itself is not followed.
    let scores = fetcher(options())
        .fetch(&server.at("/scores"))
        .await
        .unwrap();
    assert_eq!(scores.final_url, server.at("/scores"));
    let again = fetcher(options())
        .fetch(&server.at("/again"))
        .await
        .unwrap();
    assert_eq!(again.final_url, server.at("/again"));
}

fn with_headers(mut reply: Reply, headers: &[(&'static str, &str)]) -> Reply {
    reply
        .headers
        .extend(headers.iter().map(|(n, v)| (*n, v.to_string())));
    reply
}

/// A sniff that takes anything naming an RSS root for a feed.
fn rss_head(_: Option<&str>, head: &[u8]) -> bool {
    head.windows(4).any(|w| w == b"<rss")
}

const RSS: &[u8] =
    b"<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>T</title></channel></rss>";

#[tokio::test]
async fn a_feed_comes_with_its_validators_and_is_asked_for_conditionally() {
    let modified = "Tue, 03 Mar 2026 08:00:00 GMT";
    let server = serve(vec![
        (
            "/feed",
            with_headers(
                html("application/rss+xml; charset=utf-8", RSS),
                &[("ETag", "\"v1\""), ("Last-Modified", modified)],
            ),
        ),
        (
            "/same",
            Reply {
                status: 304,
                headers: vec![("ETag", "\"v1\"".into())],
                body: Vec::new(),
                delay: Duration::ZERO,
            },
        ),
    ])
    .await;
    let fetcher = fetcher(options());
    let got = fetcher
        .fetch_feed(&server.at("/feed"), &Validators::default(), rss_head)
        .await
        .unwrap();
    let FeedResponse::Body {
        bytes,
        etag,
        last_modified,
        final_url,
        moved,
    } = got
    else {
        panic!("a body: {got:?}")
    };
    assert_eq!(bytes, RSS);
    assert_eq!(etag.as_deref(), Some("\"v1\""));
    assert_eq!(last_modified.as_deref(), Some(modified));
    assert_eq!(final_url, server.at("/feed"));
    assert!(!moved);
    assert!(!server.last_head().contains("if-none-match"));

    let validators = Validators {
        etag: etag.clone(),
        last_modified: last_modified.clone(),
    };
    let again = fetcher
        .fetch_feed(&server.at("/same"), &validators, rss_head)
        .await
        .unwrap();
    assert_eq!(again, FeedResponse::NotModified);
    let head = server.last_head();
    assert!(head.contains("if-none-match: \"v1\""), "{head}");
    assert!(head.contains(&format!("if-modified-since: {}", modified.to_lowercase())));
}

#[tokio::test]
async fn a_moved_feed_says_whether_it_moved_for_good() {
    let server = serve(vec![
        ("/old", redirect(301, "/feed")),
        ("/for-now", redirect(302, "/feed")),
        ("/feed", html("application/atom+xml", RSS)),
        ("/page", html("text/html", b"<p>A page.</p>")),
        ("/plain", html("text/plain", RSS)),
        ("/image", html("image/png", b"PNG")),
        ("/json", html("application/feed+json", b"{}")),
        ("/big", html("application/xml", &vec![b' '; 2048])),
    ])
    .await;
    let fetcher = fetcher(FetchOptions {
        max_bytes: 1024,
        ..options()
    });
    let body = |r: FeedResponse| match r {
        FeedResponse::Body {
            final_url, moved, ..
        } => (final_url, moved),
        FeedResponse::NotModified => panic!("a body"),
    };
    let none = Validators::default();
    let got = fetcher
        .fetch_feed(&server.at("/old"), &none, rss_head)
        .await
        .unwrap();
    assert_eq!(body(got), (server.at("/feed"), true));
    let got = fetcher
        .fetch_feed(&server.at("/for-now"), &none, rss_head)
        .await
        .unwrap();
    assert_eq!(body(got), (server.at("/feed"), false));

    assert!(fetcher
        .fetch_feed(&server.at("/json"), &none, rss_head)
        .await
        .is_ok());
    // Servers often send feeds as pages or plain text: the sniff tells them apart.
    let plain = fetcher
        .fetch_feed(&server.at("/plain"), &none, rss_head)
        .await;
    assert!(matches!(plain, Ok(FeedResponse::Body { .. })), "{plain:?}");
    for page in ["/page", "/image"] {
        assert!(matches!(
            fetcher.fetch_feed(&server.at(page), &none, rss_head).await,
            Err(FetchError::NotAFeed(_))
        ));
    }
    assert!(matches!(
        fetcher
            .fetch_feed(&server.at("/big"), &none, rss_head)
            .await,
        Err(FetchError::TooLarge)
    ));
}
