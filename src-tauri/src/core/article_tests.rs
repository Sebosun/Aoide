//! Articles through the core, with a scripted fetcher in place of the network.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aoide_fetch::{FetchError, Snapshot, Via};
use aoide_library::{ArticleRow, BookId, ItemKind, Library, NewBook};
use aoide_tts::testing::FakeEngine;
use url::Url;

use super::articles::{ArticleSource, Fetcher, Fetching};
use super::fetching::{Reason, Stage};
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

/// `LIGHTS` declaring far more words than it has.
fn cut_short() -> String {
    LIGHTS.replace(
        "</head>",
        r#"<script type="application/ld+json">{"@type":"NewsArticle","headline":"The Quay Lights","wordCount":2000}</script></head>"#,
    )
}

/// `LIGHTS` with more paragraphs.
fn longer() -> String {
    let more = "<p>In the spring the lamps are cleaned and their glass replaced where the winter storms cracked it, a job that takes the whole of one week.</p>";
    LIGHTS.replace("</article>", &format!("{more}{more}</article>"))
}

async fn saved(pages: &Arc<Pages>) -> (tempfile::TempDir, Arc<Core>, String) {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
    let id = core.save_article(LIGHTS_URL).unwrap();
    (dir, core, id)
}

async fn settled(core: &Core, id: &str) -> Stage {
    until("the fetch settles", || {
        !matches!(
            stage(core, id),
            Some(Stage::Queued | Stage::Fetching) | None
        )
    })
    .await;
    stage(core, id).unwrap()
}

fn snapshot(dir: &tempfile::TempDir, id: &str) -> std::path::PathBuf {
    dir.path().join(format!("data/articles/{id}/snapshot.html"))
}

#[tokio::test]
async fn a_cut_short_page_needs_the_user_and_its_text_is_readable() {
    let pages = Pages::with(&[(LIGHTS_URL, &cut_short())]);
    let (_dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::NeedsYou(Reason::CutShort));
    let view = core.article(&id).unwrap();
    assert_eq!(view.declared_words, Some(2000));
    assert!(view.captured_words > 0);
    let chapter = core.chapter(&id, 0).unwrap();
    assert!(!chapter.blocks.is_empty());
}

#[tokio::test]
async fn a_failed_fetch_stores_nothing_and_says_why() {
    let pages = Pages::with(&[]);
    pages.set(LIGHTS_URL, Err(FetchError::Status(503)));
    let (dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::Failed);
    let view = core.article(&id).unwrap();
    assert_eq!(
        view.error.as_deref(),
        Some("The site answered with status 503.")
    );
    assert!(!snapshot(&dir, &id).exists());
    let summary = core
        .list_books()
        .unwrap()
        .into_iter()
        .find(|b| b.id == id)
        .unwrap();
    assert_eq!(summary.article.unwrap().stage, Stage::Failed);
    assert!(core.open_book(&id).is_err());

    // Fetching again once the site is back.
    pages.set(LIGHTS_URL, Ok(LIGHTS.into()));
    core.refetch_article(&id, ArticleSource::Http).unwrap();
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    assert_eq!(core.article(&id).unwrap().error, None);
    assert!(snapshot(&dir, &id).exists());
}

#[tokio::test]
async fn a_refetch_keeps_whichever_copy_has_more() {
    let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
    let (dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    let first = core.article(&id).unwrap().captured_words;

    // More text: the new copy replaces the stored one, and the text read changes with it.
    pages.set(LIGHTS_URL, Ok(longer()));
    core.refetch_article(&id, ArticleSource::Http).unwrap();
    until("the longer copy is kept", || {
        core.article(&id).unwrap().captured_words > first
    })
    .await;
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    let more = core.article(&id).unwrap().captured_words;
    assert!(std::fs::read_to_string(snapshot(&dir, &id))
        .unwrap()
        .contains("lamps are cleaned"));
    assert_eq!(core.chapter(&id, 0).unwrap().blocks.len(), 6);

    // Less text, then a failure: the better copy stays, readable.
    pages.set(LIGHTS_URL, Ok(cut_short()));
    core.refetch_article(&id, ArticleSource::Http).unwrap();
    until("the shorter fetch is done", || pages.asks() == 3).await;
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    assert_eq!(core.article(&id).unwrap().captured_words, more);

    pages.set(LIGHTS_URL, Err(FetchError::Timeout));
    core.refetch_article(&id, ArticleSource::Http).unwrap();
    until("the failed fetch is done", || pages.asks() == 4).await;
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    let view = core.article(&id).unwrap();
    assert_eq!(view.captured_words, more);
    assert!(view.error.unwrap().starts_with("Could not fetch it again."));
    assert_eq!(core.chapter(&id, 0).unwrap().blocks.len(), 6);
}

#[tokio::test]
async fn removing_an_article_deletes_its_folder_and_it_can_be_saved_again() {
    let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
    let (dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    core.remove_book(&id).unwrap();
    assert!(!snapshot(&dir, &id).parent().unwrap().exists());
    assert!(core.article(&id).is_err());

    assert_eq!(core.save_article(LIGHTS_URL).unwrap(), id);
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    assert!(core.list_books().unwrap().iter().any(|b| b.id == id));
}
