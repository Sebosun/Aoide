//! Articles through the core, with a scripted fetcher in place of the network.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aoide_fetch::{FeedResponse, FetchError, Image, Snapshot, Validators, Via};
use aoide_library::{ArticleRow, BookId, ItemKind, Library, NewBook};
use aoide_tts::testing::FakeEngine;
use url::Url;

use super::articles::{ArticleSource, Fetcher, Fetching};
use super::fetching::{Reason, Stage};
use super::tests::{library_folder, start_fetching, until};
use super::Core;

/// A scripted feed's body, ETag and media type, or why it fails.
type FeedReply = Result<(String, String, &'static str), FetchError>;

/// Answers each address with the page, or failure, scripted for it, and counts the asks.
#[derive(Default)]
pub(super) struct Pages {
    pages: Mutex<HashMap<String, Result<String, FetchError>>>,
    /// The archive's copy of each address it has one of.
    archive: Mutex<HashMap<String, String>>,
    images: Mutex<HashMap<String, Image>>,
    /// Each feed's body and ETag, or why it fails.
    feeds: Mutex<HashMap<String, FeedReply>>,
    /// Feed addresses that moved for good, and where to.
    moved: Mutex<HashMap<String, String>>,
    pub asked: Mutex<Vec<String>>,
    /// The validators each feed request carried, in order.
    pub validators: Mutex<Vec<(String, Validators)>>,
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

    pub fn archive(&self, url: &str, copy: &str) {
        self.archive.lock().unwrap().insert(url.into(), copy.into());
    }

    pub fn image(&self, url: &str, media_type: &str, bytes: &[u8]) {
        let image = Image {
            media_type: media_type.into(),
            bytes: bytes.to_vec(),
        };
        self.images.lock().unwrap().insert(url.into(), image);
    }

    pub fn asks(&self) -> usize {
        self.asked.lock().unwrap().len()
    }

    /// Serves `body` at feed address `url`, with ETag `etag` (a request carrying it is told
    /// nothing changed).
    pub fn feed(&self, url: &str, body: &str, etag: &str) {
        self.feed_as(url, body, etag, "application/rss+xml");
    }

    /// As [`Pages::feed`], served as `content_type`.
    pub fn feed_as(&self, url: &str, body: &str, etag: &str, content_type: &'static str) {
        let reply = Ok((body.to_owned(), etag.to_owned(), content_type));
        self.feeds.lock().unwrap().insert(url.into(), reply);
    }

    pub fn feed_fails(&self, url: &str, error: FetchError) {
        self.feeds.lock().unwrap().insert(url.into(), Err(error));
    }

    /// Feed `from` has moved for good to `to`.
    pub fn feed_moved(&self, from: &str, to: &str) {
        self.moved.lock().unwrap().insert(from.into(), to.into());
    }

    /// How many times feed `url` was asked for.
    pub fn feed_asks(&self, url: &str) -> usize {
        let validators = self.validators.lock().unwrap();
        validators.iter().filter(|(u, _)| u == url).count()
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

    fn archived<'a>(&'a self, url: &'a Url) -> Fetching<'a, Option<Url>> {
        Box::pin(async move {
            let copy = self.archive.lock().unwrap().get(url.as_str()).cloned();
            Ok(copy.map(|c| Url::parse(&c).unwrap()))
        })
    }

    fn image<'a>(&'a self, url: &'a Url) -> Fetching<'a, Image> {
        Box::pin(async move {
            let image = self.images.lock().unwrap().get(url.as_str()).cloned();
            image.ok_or(FetchError::Status(404))
        })
    }

    fn feed<'a>(&'a self, url: &'a Url, validators: &'a Validators) -> Fetching<'a, FeedResponse> {
        Box::pin(async move {
            let asked = url.to_string();
            self.validators
                .lock()
                .unwrap()
                .push((asked.clone(), validators.clone()));
            let moved = self.moved.lock().unwrap().get(&asked).cloned();
            let target = moved.clone().unwrap_or(asked);
            let reply = self.feeds.lock().unwrap().get(&target).cloned();
            match reply {
                Some(Ok((body, etag, content_type))) => {
                    if !aoide_feeds::looks_like_feed(Some(content_type), body.as_bytes())
                        && !content_type.ends_with("xml")
                    {
                        return Err(FetchError::NotAFeed(content_type.into()));
                    }
                    if validators.etag.as_deref() == Some(etag.as_str()) {
                        return Ok(FeedResponse::NotModified);
                    }
                    Ok(FeedResponse::Body {
                        bytes: body.into_bytes(),
                        etag: Some(etag),
                        last_modified: None,
                        final_url: Url::parse(&target).unwrap(),
                        moved: moved.is_some(),
                    })
                }
                Some(Err(error)) => Err(error),
                // A page is not a feed.
                None if self.pages.lock().unwrap().contains_key(&target) => {
                    Err(FetchError::NotAFeed("text/html".into()))
                }
                None => Err(FetchError::Status(404)),
            }
        })
    }
}

pub(super) const LIGHTS: &str = r#"<!doctype html><html lang="en"><head>
<title>The Quay Lights | Quay Weekly</title>
<meta property="og:site_name" content="Quay Weekly">
<meta property="og:title" content="The Quay Lights">
<meta name="author" content="Ada Brightwater">
<link rel="alternate" type="application/rss+xml" title="Quay Weekly" href="/feed.xml">
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
    // Its length is known before it is opened, for reading times in lists.
    assert!(
        item.word_count.is_some_and(|n| n > 60),
        "{:?}",
        item.word_count
    );
    let summary = item.article.as_ref().unwrap();
    assert_eq!(summary.site_name.as_deref(), Some("Quay Weekly"));
    assert!(summary
        .lead
        .as_deref()
        .is_some_and(|l| l.starts_with("Every evening at dusk")));
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
                lead: None,
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
pub(super) fn cut_short() -> String {
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
    let article = summary.article.unwrap();
    assert_eq!(article.stage, Stage::Failed);
    // A site that has not named itself goes by its address.
    assert_eq!(article.site_name.as_deref(), Some("quay.example"));
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
async fn a_partial_copy_is_replaced_only_by_a_fuller_one() {
    let pages = Pages::with(&[(LIGHTS_URL, &cut_short())]);
    let (_dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::NeedsYou(Reason::CutShort));
    let kept = core.article(&id).unwrap().captured_words;

    // The site now shows a sign-in page with less on it: the copy stays as it was.
    pages.set(
        LIGHTS_URL,
        Ok("<html><head><title>Sign in</title></head><body><p>Sign in to keep reading.</p></body></html>".into()),
    );
    core.refetch_article(&id, ArticleSource::Http).unwrap();
    until("the shorter fetch is done", || pages.asks() == 2).await;
    assert_eq!(settled(&core, &id).await, Stage::NeedsYou(Reason::CutShort));
    assert_eq!(core.article(&id).unwrap().captured_words, kept);
    assert_eq!(core.chapter(&id, 0).unwrap().blocks.len(), 4);
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

#[tokio::test]
async fn an_article_is_narrated_prepared_and_exported_like_a_book() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _) = start_fetching(dir.path(), &engine, Pages::with(&[(LIGHTS_URL, LIGHTS)]));
    let id = core.save_article(LIGHTS_URL).unwrap();
    until("the article is fetched", || {
        stage(&core, &id) == Some(Stage::Ready)
    })
    .await;

    core.play(&id, Some(0), None).await.unwrap();
    until("its first sentence is spoken", || {
        engine
            .calls()
            .iter()
            .any(|t| t.starts_with("Every evening at dusk"))
    })
    .await;
    core.stop();

    core.prepare_book(&id).await.unwrap();
    until("the article is prepared", || {
        core.prepare_view(&id)
            .is_some_and(|v| v.state == super::PrepareStage::Done)
    })
    .await;
    match core.export_book(&id).await {
        Ok(path) => assert!(std::path::Path::new(&path).exists(), "{path}"),
        // Exporting needs ffmpeg, which a test machine may not have.
        Err(error) => assert!(error.contains("ffmpeg"), "{error}"),
    }
}

#[tokio::test]
async fn an_article_cannot_be_paired_with_a_recording_or_looked_up() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let (core, _) = start_fetching(
        dir.path(),
        &FakeEngine::new(),
        Pages::with(&[(LIGHTS_URL, LIGHTS)]),
    );
    let id = core.save_article(LIGHTS_URL).unwrap();
    until("the article is fetched", || {
        stage(&core, &id) == Some(Stage::Ready)
    })
    .await;
    let refused = |result: Result<(), String>, what: &str| {
        assert_eq!(
            result,
            Err(super::articles::NOT_FOR_ARTICLES.to_string()),
            "{what}"
        );
    };
    let files = vec![dir.path().join("a.mp3").to_string_lossy().into_owned()];
    refused(
        core.attach_recording(&id, files.clone()).map(drop),
        "attach_recording",
    );
    refused(
        core.attach_recording_as_listed(&id, files).map(drop),
        "attach_recording_as_listed",
    );
    refused(
        core.lookup_metadata(&id, false).await.map(drop),
        "lookup_metadata",
    );
    assert!(core.recording(&id).is_none());
}

const COPY_URL: &str = "https://web.archive.org/web/20250102030405id_/https://quay.example/lights";

#[tokio::test]
async fn an_archived_copy_is_fetched_when_the_page_cannot_be() {
    let pages = Pages::with(&[(COPY_URL, LIGHTS)]);
    pages.set(LIGHTS_URL, Err(FetchError::Status(403)));
    let (dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::Failed);

    // No copy in the archive: nothing changes but the message.
    core.refetch_article(&id, ArticleSource::Archive).unwrap();
    until("the archive is asked", || {
        core.article(&id).unwrap().error.as_deref()
            == Some("No archived copy of this page was found.")
    })
    .await;
    assert_eq!(settled(&core, &id).await, Stage::Failed);
    assert!(!snapshot(&dir, &id).exists());

    pages.archive(LIGHTS_URL, COPY_URL);
    core.refetch_article(&id, ArticleSource::Archive).unwrap();
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    let view = core.article(&id).unwrap();
    assert_eq!(view.via, Some(ArticleSource::Archive));
    assert_eq!(view.error, None);
    assert!(pages.asked.lock().unwrap().contains(&COPY_URL.to_string()));
    assert_eq!(core.open_book(&id).unwrap().title, "The Quay Lights");
}

#[tokio::test]
async fn a_lead_image_becomes_the_cover_and_a_missing_one_changes_nothing() {
    let with_image = LIGHTS.replace(
        "</head>",
        r#"<meta property="og:image" content="https://quay.example/lamps.jpg"></head>"#,
    );
    let pages = Pages::with(&[(LIGHTS_URL, &with_image)]);
    pages.image(
        "https://quay.example/lamps.jpg",
        "image/jpeg",
        b"\xff\xd8lamps",
    );
    let (dir, core, id) = saved(&pages).await;
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    // Beside the books' covers, where the interface may show images from.
    let cover = dir.path().join(format!("data/covers/{id}.jpg"));
    until("the cover is stored", || {
        core.list_books()
            .unwrap()
            .iter()
            .any(|b| b.id == id && b.cover_path.as_deref() == cover.to_str())
    })
    .await;
    assert_eq!(std::fs::read(&cover).unwrap(), b"\xff\xd8lamps");
    core.remove_book(&id).unwrap();
    assert!(!cover.exists(), "removing the article removes its cover");

    // An image that cannot be fetched leaves the article as it is.
    let other = "https://quay.example/harbour";
    pages.set(
        other,
        Ok(LIGHTS.replace(
            "</head>",
            r#"<meta property="og:image" content="https://quay.example/gone.jpg"></head>"#,
        )),
    );
    let second = core.save_article(other).unwrap();
    assert_eq!(settled(&core, &second).await, Stage::Ready);
    let item = core
        .list_books()
        .unwrap()
        .into_iter()
        .find(|b| b.id == second)
        .unwrap();
    assert_eq!(item.cover_path, None);
}

#[tokio::test]
async fn open_original_opens_the_articles_page_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let (core, events) = start_fetching(
        dir.path(),
        &FakeEngine::new(),
        Pages::with(&[(LIGHTS_URL, LIGHTS)]),
    );
    let id = core
        .save_article("https://quay.example/lights?utm_source=feed")
        .unwrap();
    core.open_original(&id).unwrap();
    // The page as it was kept (without the tracking query), in the system's browser.
    assert_eq!(
        events.payloads("test://opened-link"),
        [serde_json::json!(LIGHTS_URL)]
    );
    assert_eq!(
        core.open_original("not-an-item"),
        Err("Not an article.".to_string())
    );
}

#[tokio::test]
async fn a_finished_article_is_offered_for_the_archive_or_archived_by_the_setting() {
    let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let (core, events) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
    let id = core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(settled(&core, &id).await, Stage::Ready);
    let archived = |core: &Core| {
        let books = core.list_books().unwrap();
        books.iter().find(|b| b.id == id).unwrap().archived
    };

    core.mark_finished(&id).unwrap();
    assert!(!archived(&core));
    let offered = events.payloads("item://finished");
    assert_eq!(
        offered,
        [serde_json::json!({"id": id, "title": "The Quay Lights"})]
    );
    // Finishing it again, or a book, offers nothing.
    core.mark_finished(&id).unwrap();
    assert_eq!(events.count("item://finished"), 1);

    core.mark_unread(&id).unwrap();
    core.update_app_settings(super::settings::AppSettingsPatch {
        archive_finished_articles: Some(true),
        ..Default::default()
    })
    .unwrap();
    core.mark_finished(&id).unwrap();
    assert!(archived(&core));
    assert_eq!(events.count("item://finished"), 1);
}

/// A newsletter as a mail client saves it: no "view online" link.
pub(super) const NEWSLETTER: &str = "From: \"Harbour Weekly\" <letters@harbour.example>\r\n\
Subject: Tides this week\r\n\
Date: Tue, 10 Nov 2026 07:30:00 +0000\r\n\
Message-ID: <issue-41@harbour.example>\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<html><body><h1>Tides this week</h1>\
<p>The spring tides come early this year, and the harbour master has moved the moorings \
to the inner basin until the end of the month.</p>\
<p>Boats on the outer pontoon should expect a strong set on the ebb.</p></body></html>\r\n";

#[tokio::test]
async fn a_saved_email_becomes_an_article_without_a_request() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let pages = Pages::with(&[]);
    let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
    let mail = dir.path().join("Tides.eml");
    std::fs::write(&mail, NEWSLETTER).unwrap();
    let path = mail.to_string_lossy().into_owned();

    let results = core.import_files(vec![path.clone()]);
    assert_eq!(results[0].error, None);
    let id = results[0].book_id.clone().unwrap();
    assert_eq!(stage(&core, &id), Some(Stage::Ready), "read at once");
    let books = core.list_books().unwrap();
    let item = books.iter().find(|b| b.id == id).unwrap();
    assert_eq!(item.kind, ItemKind::Article);
    assert_eq!(item.title, "Tides this week");
    assert_eq!(item.authors, ["Harbour Weekly"]);
    assert_eq!(
        item.article.as_ref().unwrap().site_name.as_deref(),
        Some("Harbour Weekly")
    );
    let view = core.article(&id).unwrap();
    assert_eq!(view.via, Some(ArticleSource::Email));
    assert_eq!(view.canonical_url, "email:issue-41@harbour.example");
    assert!(view.published_at.is_some());
    let chapter = core.chapter(&id, 0).unwrap();
    let text: String = chapter
        .blocks
        .iter()
        .flat_map(|b| &b.segments)
        .flat_map(|s| &s.words)
        .map(|w| format!("{w} "))
        .collect();
    assert!(text.contains("The spring tides come early"), "{text}");

    // The same email again is the same item; the file is not copied into the library.
    assert_eq!(core.import_files(vec![path]).remove(0).book_id, Some(id));
    let articles = core
        .list_books()
        .unwrap()
        .into_iter()
        .filter(|b| b.kind == ItemKind::Article)
        .count();
    assert_eq!(articles, 1);
    assert!(!dir.path().join("Books/Tides.eml").exists());
    assert_eq!(pages.asks(), 0, "nothing is fetched");
}

#[tokio::test]
async fn an_email_with_a_web_copy_is_kept_under_its_address_and_a_non_email_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let pages = Pages::with(&[]);
    let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
    let online = NEWSLETTER.replace(
        "<h1>",
        "<p><a href=\"https://harbour.example/issues/41\">View in browser</a></p><h1>",
    );
    let mail = dir.path().join("Tides.eml");
    std::fs::write(&mail, online).unwrap();
    let not_mail = dir.path().join("notes.eml");
    std::fs::write(&not_mail, "just some words, no headers").unwrap();
    let results = core.import_files(vec![
        mail.to_string_lossy().into_owned(),
        not_mail.to_string_lossy().into_owned(),
    ]);
    let id = results[0].book_id.clone().unwrap();
    assert_eq!(
        core.article(&id).unwrap().canonical_url,
        "https://harbour.example/issues/41"
    );
    assert_eq!(results[1].book_id, None);
    assert_eq!(results[1].error.as_deref(), Some("not an email"));
    assert_eq!(pages.asks(), 0);
}
