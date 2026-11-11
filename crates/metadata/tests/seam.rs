//! First behaviour tests for the agreed seam (owned by Claude; do not edit).
use aoide_metadata::{Error, OpenLibrary, Options, Query};

fn client(base: &str) -> OpenLibrary {
    OpenLibrary::new(Options {
        base_url: base.into(),
        covers_url: "http://127.0.0.1:9".into(),
        ..Default::default()
    })
    .unwrap()
}

#[tokio::test]
async fn search_maps_open_library_docs_to_matches() {
    let body = r#"{"numFound":1,"docs":[{"key":"/works/OL1W","title":"An Original Tale",
        "author_name":["Mira Vale","Ellis Reed"],"first_publish_year":2019,
        "publisher":["Preview Press","Other"],"isbn":["0123456789","9780123456786"],
        "subject":["Fiction","Sea"],"cover_i":42}]}"#;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0; 4096];
        let n = stream.read(&mut buffer).await.unwrap();
        let head = String::from_utf8_lossy(&buffer[..n]).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        head
    });
    let matches = client(&base)
        .search(&Query {
            isbn: None,
            title: "An Original Tale".into(),
            authors: vec!["Mira Vale".into()],
        })
        .await
        .unwrap();
    let head = server.await.unwrap();
    let path = head.lines().next().unwrap();
    assert!(path.starts_with("GET /search.json?"), "{path}");
    assert!(
        path.contains("title=An+Original+Tale") || path.contains("title=An%20Original%20Tale"),
        "{path}"
    );
    assert!(path.contains("author=Mira"), "{path}");
    assert_eq!(matches.len(), 1);
    let m = &matches[0];
    assert_eq!(m.key, "/works/OL1W");
    assert_eq!(m.authors, vec!["Mira Vale", "Ellis Reed"]);
    assert_eq!(m.published.as_deref(), Some("2019"));
    assert_eq!(m.publisher.as_deref(), Some("Preview Press"));
    assert_eq!(m.isbn.as_deref(), Some("9780123456786"));
    assert_eq!(m.subjects, vec!["Fiction", "Sea"]);
    assert_eq!(
        m.cover_url.as_deref(),
        Some("http://127.0.0.1:9/b/id/42-M.jpg")
    );
}

#[tokio::test]
async fn blank_queries_and_bad_keys_are_rejected_without_requests() {
    let c = client("http://127.0.0.1:9");
    assert!(matches!(
        c.search(&Query::default()).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        c.description("/works/../../etc").await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        c.cover("http://example.com/x.jpg").await,
        Err(Error::InvalidInput(_))
    ));
}
