//! A real browser against pages served on loopback. Opt-in: `AOIDE_BROWSER_TESTS=1`, with
//! the browser's program in `AOIDE_BROWSER` (else whatever `find_binary` finds).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use aoide_browser::find::{find_binary, path_dirs};
use aoide_browser::{Browser, BrowserError, BrowserOptions, Reach, RenderRequest, Via};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

/// How a path is answered: status line and headers, body, and whether to keep the
/// connection open without finishing.
#[derive(Clone)]
struct Answer {
    head: String,
    body: String,
    hang: bool,
}

fn html(body: &str) -> Answer {
    Answer {
        head: "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8".into(),
        body: body.into(),
        hang: false,
    }
}

/// Serves `routes`; a request's `Cookie` header replaces `{cookie}` in the body.
async fn serve(routes: HashMap<&'static str, Answer>) -> Url {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let base = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
    let routes = Arc::new(routes);
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let routes = Arc::clone(&routes);
            tokio::spawn(async move {
                let mut got = Vec::new();
                let mut buffer = [0u8; 4096];
                while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => got.extend_from_slice(&buffer[..n]),
                    }
                }
                let head = String::from_utf8_lossy(&got).into_owned();
                let path = head.split(' ').nth(1).unwrap_or("/").to_owned();
                let cookie = head
                    .lines()
                    .find_map(|l| l.strip_prefix("Cookie: ").or(l.strip_prefix("cookie: ")))
                    .unwrap_or("none")
                    .to_owned();
                let answer = routes.get(path.as_str()).cloned().unwrap_or(Answer {
                    head: "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain".into(),
                    body: "not here".into(),
                    hang: false,
                });
                let body = answer.body.replace("{cookie}", &cookie);
                if answer.hang {
                    let _ = stream
                        .write_all(format!("{}\r\n\r\n", answer.head).as_bytes())
                        .await;
                    tokio::time::sleep(Duration::from_secs(120)).await;
                    return;
                }
                let reply = format!(
                    "{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    answer.head,
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes()).await;
            });
        }
    });
    base
}

fn wanted() -> bool {
    std::env::var("AOIDE_BROWSER_TESTS").is_ok_and(|v| v == "1")
}

async fn launch(reach: Reach, timeout: Duration) -> (tempfile::TempDir, Browser) {
    let binary = std::env::var_os("AOIDE_BROWSER")
        .map(Into::into)
        .or_else(|| {
            find_binary(None, &path_dirs(), std::path::Path::new("/nonexistent")).map(|f| f.path)
        })
        .expect("a browser: set AOIDE_BROWSER");
    let dir = tempfile::tempdir().unwrap();
    let browser = Browser::launch(BrowserOptions {
        binary,
        profile_dir: dir.path().join("profile"),
        page_timeout: timeout,
        reach,
        visible: false,
    })
    .await
    .unwrap();
    (dir, browser)
}

const ARTICLE: &str = "<p>The lamplighter walked the quay at dusk and lit every lamp by hand.</p>";

#[tokio::test]
async fn renders_static_and_script_built_pages_and_blocks_what_it_is_told() {
    if !wanted() {
        return;
    }
    let base = serve(HashMap::from([
        ("/static", html(&format!("<html><body><article>{ARTICLE}</article></body></html>"))),
        (
            "/later",
            html(
                "<html><body><main id=m>Loading…</main><script>setTimeout(() => {\
                 document.getElementById('m').innerHTML = \
                 '<article class=body><p>Inserted by the page after a second.</p></article>';\
                 }, 1000);</script></body></html>",
            ),
        ),
        (
            "/with-ads",
            html("<html><body><p id=p>Article text.</p><script src=\"/ads/a.js\"></script></body></html>"),
        ),
        (
            "/ads/a.js",
            Answer {
                head: "HTTP/1.1 200 OK\r\nContent-Type: text/javascript".into(),
                body: "document.getElementById('p').textContent += ' AD';".into(),
                hang: false,
            },
        ),
    ]))
    .await;
    let (_dir, browser) = launch(Reach::AnyAddress, Duration::from_secs(20)).await;
    println!("browser: {}", browser.version().await.unwrap());

    let page = browser
        .render(&RenderRequest::new(base.join("static").unwrap()))
        .await
        .unwrap();
    assert!(page.html.contains("lit every lamp by hand"));
    assert_eq!(page.via, Via::Browser);
    assert_eq!(page.final_url, base.join("static").unwrap());
    assert_eq!(page.status, 200);

    let mut later = RenderRequest::new(base.join("later").unwrap());
    later.wait_for = Some("article.body".into());
    let page = browser.render(&later).await.unwrap();
    assert!(page.html.contains("Inserted by the page after a second."));
    // Without a selector, the text settling is waited for instead.
    let page = browser
        .render(&RenderRequest::new(base.join("later").unwrap()))
        .await
        .unwrap();
    assert!(page.html.contains("Inserted by the page"), "{}", page.html);

    let page = browser
        .render(&RenderRequest::new(base.join("with-ads").unwrap()))
        .await
        .unwrap();
    assert!(page.html.contains("Article text. AD"));
    let mut blocked = RenderRequest::new(base.join("with-ads").unwrap());
    blocked.block = vec!["*/ads/*".into()];
    let page = browser.render(&blocked).await.unwrap();
    assert!(page.html.contains("Article text.") && !page.html.contains(" AD"));
    browser.shutdown().await;
}

#[tokio::test]
async fn a_page_that_never_goes_quiet_is_taken_as_it_is_at_the_timeout() {
    if !wanted() {
        return;
    }
    let base = serve(HashMap::from([
        (
            "/busy",
            html(&format!(
                "<html><body>{ARTICLE}<script>fetch('/forever');</script></body></html>"
            )),
        ),
        (
            "/forever",
            Answer {
                head: "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 100".into(),
                body: String::new(),
                hang: true,
            },
        ),
    ]))
    .await;
    let (_dir, browser) = launch(Reach::AnyAddress, Duration::from_secs(4)).await;
    let started = std::time::Instant::now();
    let page = browser
        .render(&RenderRequest::new(base.join("busy").unwrap()))
        .await
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(4));
    assert!(started.elapsed() < Duration::from_secs(15));
    assert!(page.html.contains("lit every lamp by hand"));
    browser.shutdown().await;
}

#[tokio::test]
async fn cookies_are_kept_between_renders_until_the_site_is_cleared() {
    if !wanted() {
        return;
    }
    let base = serve(HashMap::from([
        (
            "/set",
            Answer {
                head: "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nSet-Cookie: session=kept; Max-Age=3600; Path=/".into(),
                body: "<p>set</p>".into(),
                hang: false,
            },
        ),
        ("/show", html("<p id=c>cookie: {cookie}</p>")),
    ]))
    .await;
    let (_dir, browser) = launch(Reach::AnyAddress, Duration::from_secs(20)).await;
    browser
        .render(&RenderRequest::new(base.join("set").unwrap()))
        .await
        .unwrap();
    let page = browser
        .render(&RenderRequest::new(base.join("show").unwrap()))
        .await
        .unwrap();
    assert!(page.html.contains("cookie: session=kept"), "{}", page.html);
    let mut cleared = RenderRequest::new(base.join("show").unwrap());
    cleared.clear_site_data = true;
    let page = browser.render(&cleared).await.unwrap();
    assert!(page.html.contains("cookie: none"), "{}", page.html);
    browser.shutdown().await;
}

#[tokio::test]
async fn downloads_are_denied_and_local_addresses_refused_under_public_reach() {
    if !wanted() {
        return;
    }
    let base = serve(HashMap::from([(
        "/file",
        Answer {
            head: "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\n\
                   Content-Disposition: attachment; filename=\"thing.bin\""
                .into(),
            body: "binary".into(),
            hang: false,
        },
    )]))
    .await;
    let (dir, browser) = launch(Reach::AnyAddress, Duration::from_secs(10)).await;
    let download = browser
        .render(&RenderRequest::new(base.join("file").unwrap()))
        .await;
    assert!(download.is_err(), "{download:?}");
    let saved = walk(dir.path())
        .into_iter()
        .any(|p| p.ends_with("thing.bin"));
    assert!(!saved, "nothing was saved");
    browser.shutdown().await;

    let (_dir, public) = launch(Reach::Public, Duration::from_secs(10)).await;
    let local = public
        .render(&RenderRequest::new(base.join("file").unwrap()))
        .await;
    let Err(BrowserError::Load(why)) = local else {
        panic!("refused: {local:?}")
    };
    assert!(why.contains("local network"), "{why}");
    public.shutdown().await;
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path);
        }
    }
    found
}
