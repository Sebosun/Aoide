//! Articles through the core, with a scripted fetcher in place of the network.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aoide_fetch::{FetchError, Snapshot, Via};
use aoide_library::{ArticleRow, BookId, ItemKind, Library, NewBook};
use aoide_tts::testing::FakeEngine;
use url::Url;

use super::articles::{Fetcher, Fetching};
use super::fetching::Stage;
use super::tests::{library_folder, start_fetching, until};
use super::Core;

/// Answers each address with the page, or failure, scripted for it, and counts the asks.
#[derive(Default)]
pub(super) struct Pages {
    pages: Mutex<HashMap<String, Result<String, FetchError>>>,
    pub asked: Mutex<Vec<String>>,
}

impl Pages {
    pub fn with(pages: &[(&str, &str)]) -> Arc<Self> {
        let this = Self::default();
        for (url, html) in pages {
            this.set(url, Ok(html.to_string()));
        }
        Arc::new(this)
    }

    pub fn set(&self, url: &str, page: Result<String, FetchError>) {
        self.pages.lock().unwrap().insert(url.into(), page);
    }

    pub fn asks(&self) -> usize {
        self.asked.lock().unwrap().len()
    }
}

impl Fetcher for Pages {
    fn page<'a>(&'a self, url: &'a Url) -> Fetching<'a, Snapshot> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(url.to_string());
            let page = self.pages.lock().unwrap().get(url.as_str()).cloned();
            let html = page.unwrap_or(Err(FetchError::Status(404)))?;
            Ok(Snapshot {
                requested: url.clone(),
                final_url: url.clone(),
                status: 200,
                html,
                fetched_at_ms: 1_000,
                via: Via::Http,
            })
        })
    }
}

pub(super) const LIGHTS: &str = r#"<!doctype html><html lang="en"><head>
<title>The Quay Lights | Quay Weekly</title>
<meta property="og:site_name" content="Quay Weekly">
<meta property="og:title" content="The Quay Lights">
<meta name="author" content="Ada Brightwater">
</head><body><nav><a href="/">Home</a></nav><article><h1>The Quay Lights</h1>
<p>Every evening at dusk the harbour master walks the length of the quay and lights the lamps one by one, starting at the slipway and ending by the lifeboat house.</p>
<h2>Why by hand</h2>
<p>The town voted twice to put the lamps on a timer, and twice the harbour master asked them to wait another year. The walk, she says, is how she sees which boats are in.</p>
<p>On foggy nights she rings the bell at the end of the pier as well, three times, and the fishermen still out answer with their horns.</p>
</article><footer>All rights reserved.</footer></body></html>"#;

pub(super) const LIGHTS_URL: &str = "https://quay.example/lights";

fn stage(core: &Core, id: &str) -> Option<Stage> {
    core.article(id).ok().map(|view| view.stage)
}

#[tokio::test]
async fn a_saved_link_is_fetched_and_read_like_a_book() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
    let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());

    let id = core
        .save_article("  https://Quay.example/lights?utm_source=feed#top ")
        .unwrap();
    assert_eq!(
        id,
        aoide_article::canonical_id(&Url::parse(LIGHTS_URL).unwrap())
    );
    // It is an item at once, named after its site until it is fetched.
    let books = core.list_books().unwrap();
    let item = books.iter().find(|b| b.id == id).unwrap();
    assert_eq!(item.kind, ItemKind::Article);

    until("the article is fetched", || {
        stage(&core, &id) == Some(Stage::Ready)
    })
    .await;
    let books = core.list_books().unwrap();
    let item = books.iter().find(|b| b.id == id).unwrap();
    assert_eq!(item.title, "The Quay Lights");
    assert_eq!(item.authors, ["Ada Brightwater"]);
    let summary = item.article.as_ref().unwrap();
    assert_eq!(summary.site_name.as_deref(), Some("Quay Weekly"));
    assert_eq!(summary.stage, Stage::Ready);

    let view = core.article(&id).unwrap();
    assert_eq!(view.url, "https://Quay.example/lights?utm_source=feed#top");
    assert_eq!(view.canonical_url, LIGHTS_URL);
    assert_eq!(view.byline.as_deref(), Some("Ada Brightwater"));
    assert!(view.captured_words > 60);

    let book = core.open_book(&id).unwrap();
    assert_eq!(book.title, "The Quay Lights");
    assert_eq!(book.chapters.len(), 1);
    let chapter = core.chapter(&id, 0).unwrap();
    let kinds: Vec<&str> = chapter.blocks.iter().map(|b| b.kind).collect();
    assert_eq!(kinds, ["paragraph", "heading", "paragraph", "paragraph"]);

    // The same link again is the same item, and a ready article is not fetched again.
    assert_eq!(
        core.save_article("https://quay.example/lights").unwrap(),
        id
    );
    assert_eq!(pages.asks(), 1);
    assert_eq!(stage(&core, &id), Some(Stage::Ready));
}

#[tokio::test]
async fn links_that_are_not_web_addresses_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), Pages::with(&[]));
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    for link in [
        "",
        "not a link",
        "ftp://files.example/a",
        "file:///etc/hosts",
    ] {
        assert!(core.save_article(link).is_err(), "{link:?}");
    }
    // A bare host is taken as a web address.
    assert!(core.save_article("quay.example/lights").is_ok());
    assert_eq!(core.list_books().unwrap().len(), 2);
}

#[tokio::test]
async fn an_unfinished_fetch_is_queued_again_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let id = aoide_article::canonical_id(&Url::parse(LIGHTS_URL).unwrap());
    {
        // What an earlier run leaves when it stops in the middle of a fetch.
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let library = Library::open(&dir.path().join("data/library.sqlite3")).unwrap();
        library
            .upsert_book(
                &NewBook {
                    id: BookId(id.clone()),
                    kind: ItemKind::Article,
                    path: dir.path().join(format!("data/articles/{id}/snapshot.html")),
                    title: Some("quay.example".into()),
                    authors: vec![],
                    language: None,
                    cover_path: None,
                    error: None,
                },
                5,
            )
            .unwrap();
        library
            .save_article(&ArticleRow {
                book_id: BookId(id.clone()),
                url: LIGHTS_URL.into(),
                canonical_url: LIGHTS_URL.into(),
                site_name: None,
                byline: None,
                published_at: None,
                stage: "fetching".into(),
                reason: None,
                error: None,
                via: None,
                fetched_at: None,
                word_count: None,
                declared_words: None,
            })
            .unwrap();
    }
    let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
    let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
    until("the interrupted fetch finishes", || {
        stage(&core, &id) == Some(Stage::Ready)
    })
    .await;
    assert_eq!(pages.asks(), 1);
    assert!(core.open_book(&id).is_ok());
}
