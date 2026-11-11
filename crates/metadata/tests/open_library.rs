//! Open Library client behaviour over loopback HTTP with synthetic responses.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aoide_metadata::{Error, OpenLibrary, Options, Query};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Clone)]
struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    location: Option<String>,
}

fn json(body: &str) -> Reply {
    Reply {
        status: 200,
        content_type: "application/json",
        body: body.as_bytes().to_vec(),
        location: None,
    }
}

/// Answers by path (without query); records request targets in order.
struct Server {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

async fn serve(routes: Vec<(&'static str, Reply)>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let routes: HashMap<&str, Reply> = routes.into_iter().collect();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0; 8192];
            let n = stream.read(&mut buffer).await.unwrap_or(0);
            let head = String::from_utf8_lossy(&buffer[..n]).to_string();
            let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
            log.lock().unwrap().push(target.clone());
            let path = target.split('?').next().unwrap();
            let reply = routes.get(path).cloned().unwrap_or(Reply {
                status: 404,
                content_type: "text/plain",
                body: b"missing".to_vec(),
                location: None,
            });
            let location = reply
                .location
                .map(|l| format!("Location: {l}\r\n"))
                .unwrap_or_default();
            let headers = format!(
                "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n",
                reply.status, reply.content_type, reply.body.len()
            );
            let _ = stream.write_all(headers.as_bytes()).await;
            let _ = stream.write_all(&reply.body).await;
        }
    });
    Server { url, seen }
}

fn client(server: &Server, max_json_bytes: usize) -> OpenLibrary {
    OpenLibrary::new(Options {
        base_url: server.url.clone(),
        covers_url: server.url.clone(),
        max_json_bytes,
        ..Default::default()
    })
    .unwrap()
}

fn query(isbn: Option<&str>, authors: &[&str]) -> Query {
    Query {
        isbn: isbn.map(Into::into),
        title: "Quiet Places".into(),
        authors: authors.iter().map(|a| a.to_string()).collect(),
    }
}

#[tokio::test]
async fn isbn_search_comes_first_and_prefers_the_queried_isbn() {
    let server = serve(vec![(
        "/search.json",
        json(r#"{"docs":[{"key":"/works/OL2W","title":"Quiet Places","isbn":["9780000000002","978-0-00-000001-9"]},{"title":"no key"},{"key":"/works/OL3W"}]}"#),
    )])
    .await;
    let matches = client(&server, 1 << 20)
        .search(&query(Some("978-0-00-000001-9"), &["Ellis Reed"]))
        .await
        .unwrap();
    assert_eq!(matches.len(), 1, "docs without key or title are skipped");
    assert_eq!(matches[0].isbn.as_deref(), Some("978-0-00-000001-9"));
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].contains("isbn=9780000000019"), "{}", seen[0]);
    assert!(seen[0].contains("limit=8"), "{}", seen[0]);
}

#[tokio::test]
async fn empty_isbn_results_fall_back_to_title_and_author() {
    let server = serve(vec![("/search.json", json(r#"{"numFound":0,"docs":[]}"#))]).await;
    let matches = client(&server, 1 << 20)
        .search(&query(Some("0-00-000000-0"), &["", "Ellis Reed"]))
        .await
        .unwrap();
    assert!(matches.is_empty());
    let seen = server.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].contains("isbn=0000000000"), "{}", seen[0]);
    assert!(seen[1].contains("title=Quiet+Places"), "{}", seen[1]);
    assert!(seen[1].contains("author=Ellis+Reed"), "{}", seen[1]);
}

#[tokio::test]
async fn author_is_omitted_when_unknown_and_subjects_are_capped() {
    let subjects: Vec<String> = (0..14).map(|i| format!("\"S{i}\"")).collect();
    let body = format!(
        r#"{{"docs":[{{"key":"/works/OL4W","title":"T","subject":[{}]}}]}}"#,
        subjects.join(",")
    );
    let server = serve(vec![("/search.json", json(&body))]).await;
    let matches = client(&server, 1 << 20)
        .search(&query(None, &[]))
        .await
        .unwrap();
    assert_eq!(matches[0].subjects.len(), 10);
    assert_eq!(matches[0].cover_url, None);
    assert!(!server.seen.lock().unwrap()[0].contains("author="));
}

#[tokio::test]
async fn descriptions_accept_both_shapes() {
    let server = serve(vec![
        (
            "/works/OL1W.json",
            json(r#"{"description":"  Plain text. "}"#),
        ),
        (
            "/works/OL2W.json",
            json(r#"{"description":{"type":"/type/text","value":"Typed text."}}"#),
        ),
        ("/works/OL3W.json", json(r#"{"title":"No description"}"#)),
    ])
    .await;
    let c = client(&server, 1 << 20);
    assert_eq!(
        c.description("/works/OL1W").await.unwrap().as_deref(),
        Some("Plain text.")
    );
    assert_eq!(
        c.description("/works/OL2W").await.unwrap().as_deref(),
        Some("Typed text.")
    );
    assert_eq!(c.description("/works/OL3W").await.unwrap(), None);
}

#[tokio::test]
async fn http_errors_and_oversized_json_are_reported() {
    let server = serve(vec![(
        "/search.json",
        json(&format!(r#"{{"docs":[],"pad":"{}"}}"#, "x".repeat(400))),
    )])
    .await;
    assert!(matches!(
        client(&server, 1 << 20).description("/works/OL9W").await,
        Err(Error::Http(404))
    ));
    assert!(matches!(
        client(&server, 100).search(&query(None, &[])).await,
        Err(Error::TooLarge(100))
    ));
}

#[tokio::test]
async fn covers_follow_redirects_and_check_the_image_type() {
    let server = serve(vec![
        (
            "/b/id/7-L.jpg",
            Reply {
                status: 302,
                content_type: "text/plain",
                body: vec![],
                location: Some("/img/7.jpg".into()),
            },
        ),
        (
            "/img/7.jpg",
            Reply {
                status: 200,
                content_type: "image/jpeg",
                body: vec![0xFF, 0xD8, 0xFF],
                location: None,
            },
        ),
        (
            "/b/id/8-L.jpg",
            Reply {
                status: 200,
                content_type: "text/html",
                body: b"<html>".to_vec(),
                location: None,
            },
        ),
    ])
    .await;
    let c = client(&server, 1 << 20);
    let (media_type, bytes) = c
        .cover(&format!("{}/b/id/7-L.jpg", server.url))
        .await
        .unwrap();
    assert_eq!(media_type, "image/jpeg");
    assert_eq!(bytes, vec![0xFF, 0xD8, 0xFF]);
    assert!(matches!(
        c.cover(&format!("{}/b/id/8-L.jpg", server.url)).await,
        Err(Error::InvalidResponse(_))
    ));
}

#[test]
fn options_must_be_plain_origins() {
    for base in [
        "ftp://example.org",
        "https://openlibrary.org/api",
        "not a url",
    ] {
        let options = Options {
            base_url: base.into(),
            ..Default::default()
        };
        assert!(
            matches!(OpenLibrary::new(options), Err(Error::InvalidOptions(_))),
            "{base}"
        );
    }
    assert!(OpenLibrary::new(Options::default()).is_ok());
}
