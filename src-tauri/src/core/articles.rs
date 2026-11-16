//! Articles saved from the web. A saved link becomes an item at once; a worker fetches one
//! article at a time and stores what it got as a snapshot file, which is extracted again each
//! time the article is opened. A later fetch replaces the snapshot only when it is better.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use aoide_article::{Completeness, ExtractOptions, Extracted};
use aoide_document::ParsedBook;
use aoide_fetch::{
    FeedResponse, FetchError, FetchOptions, HttpFetcher, Image, Snapshot, Validators, Via,
};
use aoide_library::{ArticleRow, BookId, ItemKind, Library, NewBook};
use serde::{Deserialize, Serialize};
use url::Url;

use super::browsing::{plan_fetch, BrowserUse, Pending, Route};
use super::feeds::{DiscoveredFeed, ItemShelf};
use super::fetching::{next, Event, Stage};
use super::lock;

/// Said when something only an EPUB has is asked of an article.
pub const NOT_FOR_ARTICLES: &str =
    "Not available for an article: it has no recording to pair or book details to look up.";

/// A fetch under way.
pub type Fetching<'a, T> = Pin<Box<dyn Future<Output = Result<T, FetchError>> + Send + 'a>>;

/// Gets pages from the web: plain HTTP in the app, a script in tests.
pub trait Fetcher: Send + Sync {
    fn page<'a>(&'a self, url: &'a Url) -> Fetching<'a, Snapshot>;
    /// Where an archive keeps its closest copy of `url`, if it has one.
    fn archived<'a>(&'a self, url: &'a Url) -> Fetching<'a, Option<Url>>;
    /// An image of at most [`COVER_BYTES`].
    fn image<'a>(&'a self, url: &'a Url) -> Fetching<'a, Image>;
    /// The feed at `url`, unless it is unchanged since `validators`.
    fn feed<'a>(&'a self, url: &'a Url, validators: &'a Validators) -> Fetching<'a, FeedResponse>;
}

/// The largest lead image kept as a cover.
pub const COVER_BYTES: usize = 2 * 1024 * 1024;

impl Fetcher for HttpFetcher {
    fn page<'a>(&'a self, url: &'a Url) -> Fetching<'a, Snapshot> {
        Box::pin(self.fetch(url))
    }

    fn archived<'a>(&'a self, url: &'a Url) -> Fetching<'a, Option<Url>> {
        Box::pin(HttpFetcher::archived(self, url))
    }

    fn image<'a>(&'a self, url: &'a Url) -> Fetching<'a, Image> {
        Box::pin(HttpFetcher::image(self, url, COVER_BYTES))
    }

    fn feed<'a>(&'a self, url: &'a Url, validators: &'a Validators) -> Fetching<'a, FeedResponse> {
        Box::pin(self.fetch_feed(url, validators, aoide_feeds::looks_like_feed))
    }
}

/// Answers every fetch with the same failure.
struct Unavailable(String);

impl Fetcher for Unavailable {
    fn page<'a>(&'a self, _: &'a Url) -> Fetching<'a, Snapshot> {
        let error = FetchError::Transport(self.0.clone());
        Box::pin(async move { Err(error) })
    }

    fn archived<'a>(&'a self, _: &'a Url) -> Fetching<'a, Option<Url>> {
        let error = FetchError::Transport(self.0.clone());
        Box::pin(async move { Err(error) })
    }

    fn image<'a>(&'a self, _: &'a Url) -> Fetching<'a, Image> {
        let error = FetchError::Transport(self.0.clone());
        Box::pin(async move { Err(error) })
    }

    fn feed<'a>(&'a self, _: &'a Url, _: &'a Validators) -> Fetching<'a, FeedResponse> {
        let error = FetchError::Transport(self.0.clone());
        Box::pin(async move { Err(error) })
    }
}

/// The app's fetcher: public addresses only.
pub fn web_fetcher() -> Arc<dyn Fetcher> {
    match HttpFetcher::new(FetchOptions::default()) {
        Ok(fetcher) => Arc::new(fetcher),
        Err(error) => Arc::new(Unavailable(error.to_string())),
    }
}

/// For tests that save no articles.
#[cfg(test)]
pub fn offline() -> Arc<dyn Fetcher> {
    Arc::new(Unavailable("no network in this test".into()))
}

/// Where a fetch gets the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ArticleSource {
    /// The page itself.
    Http,
    /// An archived copy of it.
    Archive,
    /// The whole text a feed carried (fetching "from the feed" again fetches the page).
    Feed,
    /// The page rendered in the browser.
    Browser,
}

impl ArticleSource {
    fn name(self) -> &'static str {
        match self {
            ArticleSource::Http => "http",
            ArticleSource::Archive => "archive",
            ArticleSource::Feed => "feed",
            ArticleSource::Browser => "browser",
        }
    }

    fn of(via: Via) -> Self {
        match via {
            Via::Http => ArticleSource::Http,
            Via::Archive => ArticleSource::Archive,
            Via::Browser => ArticleSource::Browser,
        }
    }

    fn stored(name: &str) -> Option<Self> {
        match name {
            "http" => Some(ArticleSource::Http),
            "archive" => Some(ArticleSource::Archive),
            "feed" => Some(ArticleSource::Feed),
            "browser" => Some(ArticleSource::Browser),
            _ => None,
        }
    }
}

/// An article's source and fetch.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ArticleView {
    pub id: String,
    /// As the user gave it.
    pub url: String,
    pub canonical_url: String,
    pub site_name: Option<String>,
    pub byline: Option<String>,
    pub published_at: Option<i64>,
    pub stage: Stage,
    /// Why the last fetch failed, when it did.
    pub error: Option<String>,
    /// How the stored copy was got.
    pub via: Option<ArticleSource>,
    pub fetched_at: Option<i64>,
    pub captured_words: u64,
    pub declared_words: Option<u64>,
    /// Feeds the page links to.
    pub feeds: Vec<DiscoveredFeed>,
    /// Kept in the library; false while it was only opened from a feed.
    pub in_library: bool,
}

/// What a library card shows of an article.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ArticleSummary {
    /// The site's name, else its address's host.
    pub site_name: Option<String>,
    /// Its first lines.
    pub lead: Option<String>,
    pub stage: Stage,
    pub error: Option<String>,
}

/// What the articles need from the rest of the app.
pub(super) trait ArticlesHost: Send + Sync {
    /// Article `view.id` changed; `snapshot` when its text did too.
    fn article_changed(&self, view: ArticleView, snapshot: bool);
    /// Whether the browser is used, and the rule for the site at `url`.
    fn fetch_rules(&self, url: &Url) -> (BrowserUse, Option<aoide_config::SiteRule>);
    /// The page rendered in the browser.
    fn render<'a>(
        &'a self,
        request: &'a aoide_browser::RenderRequest,
    ) -> Pending<'a, Result<Snapshot, String>>;
}

pub(super) struct Articles {
    library: Arc<Mutex<Library>>,
    dir: PathBuf,
    /// The books' covers folder, which the interface may show images from: lead images go
    /// there as `<id>.<ext>`.
    covers: PathBuf,
    fetcher: Arc<dyn Fetcher>,
    queue: Mutex<VecDeque<(String, ArticleSource)>>,
    wake: tokio::sync::Notify,
    /// Feeds each extracted page links to.
    feeds: Mutex<HashMap<String, Vec<DiscoveredFeed>>>,
}

/// A saved link as a web address: trimmed, and `https://` when it names no scheme.
pub fn parse_link(link: &str) -> Result<Url, String> {
    let link = link.trim();
    let invalid = || "That is not a web address.".to_string();
    if link.is_empty() || link.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    let url = match Url::parse(link) {
        Ok(url) => url,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            Url::parse(&format!("https://{link}")).map_err(|_| invalid())?
        }
        Err(_) => return Err(invalid()),
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().map_or(true, str::is_empty) {
        return Err(invalid());
    }
    Ok(url)
}

impl Articles {
    pub fn new(
        library: Arc<Mutex<Library>>,
        dir: PathBuf,
        covers: PathBuf,
        fetcher: Arc<dyn Fetcher>,
    ) -> Self {
        Self {
            library,
            dir,
            covers,
            fetcher,
            queue: Mutex::new(VecDeque::new()),
            wake: tokio::sync::Notify::new(),
            feeds: Mutex::new(HashMap::new()),
        }
    }

    /// Queues the fetches an earlier run left queued or under way.
    pub fn load(&self) {
        let rows = lock(&self.library).articles().unwrap_or_default();
        let mut queue = lock(&self.queue);
        for row in rows {
            let stage = Stage::from_stored(&row.stage, row.reason.as_deref());
            if stage == Stage::Queued {
                if stage.names().0 != row.stage {
                    self.store_stage(
                        &row.book_id,
                        row.stage.as_str(),
                        stage,
                        row.error.as_deref(),
                    );
                }
                queue.push_back((row.book_id.0, ArticleSource::Http));
            }
        }
        if !queue.is_empty() {
            self.wake.notify_one();
        }
    }

    fn snapshot_path(&self, id: &str) -> PathBuf {
        self.dir.join(id).join("snapshot.html")
    }

    /// Saves `link` as an article, made on `shelf`, and queues its fetch; returns its id. A
    /// link already saved is the same item (on the shelf it was), fetched again unless it is
    /// ready.
    pub fn save(
        &self,
        link: &str,
        shelf: ItemShelf,
        now: i64,
    ) -> Result<(String, Option<ArticleView>), String> {
        let url = parse_link(link)?;
        let id = aoide_article::canonical_id(&url);
        let book = BookId(id.clone());
        let existing = lock(&self.library)
            .article(&book)
            .map_err(|e| e.to_string())?;
        if let Some(row) = existing {
            let stage = Stage::from_stored(&row.stage, row.reason.as_deref());
            if stage == Stage::Ready {
                return Ok((id, None));
            }
            return Ok((id.clone(), self.queue_fetch(&id, ArticleSource::Http)?));
        }
        self.create(&url, link, shelf, now)?;
        lock(&self.queue).push_back((id.clone(), ArticleSource::Http));
        self.wake.notify_one();
        Ok((id.clone(), self.view(&id).ok()))
    }

    /// Stores a new article item for `url` (given as `link`) on `shelf`, queued but not yet
    /// fetched.
    fn create(&self, url: &Url, link: &str, shelf: ItemShelf, now: i64) -> Result<String, String> {
        let id = aoide_article::canonical_id(url);
        let book = BookId(id.clone());
        let host = url.host_str().unwrap_or_default().to_owned();
        let library = lock(&self.library);
        library
            .upsert_book(
                &NewBook {
                    id: book.clone(),
                    kind: ItemKind::Article,
                    path: self.snapshot_path(&id),
                    title: Some(host),
                    authors: Vec::new(),
                    language: None,
                    cover_path: None,
                    error: None,
                },
                now,
            )
            .map_err(|e| e.to_string())?;
        // On its shelf before anything can list it.
        library
            .set_shelf(&book, shelf.name())
            .map_err(|e| e.to_string())?;
        let (stage, _) = Stage::Queued.names();
        library
            .save_article(&ArticleRow {
                book_id: book,
                url: link.trim().to_owned(),
                canonical_url: aoide_article::normalise(url).to_string(),
                site_name: None,
                byline: None,
                published_at: None,
                stage: stage.into(),
                reason: None,
                error: None,
                via: None,
                fetched_at: None,
                word_count: None,
                declared_words: None,
                lead: None,
            })
            .map_err(|e| e.to_string())?;
        drop(library);
        tracing::info!(
            event = "article.save",
            book = crate::logging::book(&id),
            outcome = "ok",
            "Saved an article"
        );
        Ok(id)
    }

    /// Stores article `url` (given as `link`) on `shelf` from the whole text a feed carried:
    /// `html` is read as the page, nothing is fetched. Returns its id.
    pub fn create_from_feed(
        &self,
        url: &Url,
        link: &str,
        html: String,
        shelf: ItemShelf,
        now: i64,
    ) -> Result<String, String> {
        let id = self.create(url, link, shelf, now)?;
        let book = BookId(id.clone());
        let row = lock(&self.library)
            .article(&book)
            .map_err(|e| e.to_string())?
            .ok_or("The article was removed.")?;
        let snapshot = Snapshot {
            requested: url.clone(),
            final_url: url.clone(),
            status: 200,
            html,
            fetched_at_ms: now,
            via: Via::Http,
        };
        let extracted = extract(&snapshot.html, url);
        let path = self.snapshot_path(&id);
        self.keep(&id, &row, &snapshot, &extracted, &path, ArticleSource::Feed)?;
        // Got at once: it goes through fetching as a fetch would.
        let queued = Stage::from_stored(&row.stage, row.reason.as_deref());
        let to = next(
            next(queued, Event::Started),
            Event::Got(extracted.completeness),
        );
        self.store_stage(&book, &row.stage, to, None);
        Ok(id)
    }

    /// Whether item `id` is an article.
    pub fn exists(&self, id: &str) -> bool {
        lock(&self.library)
            .article(&BookId(id.into()))
            .is_ok_and(|row| row.is_some())
    }

    /// Fetches article `id` again from `source`; returns its view when that queued it.
    pub fn queue_fetch(
        &self,
        id: &str,
        source: ArticleSource,
    ) -> Result<Option<ArticleView>, String> {
        let book = BookId(id.into());
        let row = lock(&self.library)
            .article(&book)
            .map_err(|e| e.to_string())?
            .ok_or("Not an article.")?;
        let stage = Stage::from_stored(&row.stage, row.reason.as_deref());
        let to = next(stage, Event::Retried);
        if to == stage && stage != Stage::Queued {
            return Ok(None);
        }
        self.store_stage(&book, &row.stage, to, None);
        let mut queue = lock(&self.queue);
        if !queue.iter().any(|(queued, _)| queued == id) {
            queue.push_back((id.into(), source));
        }
        drop(queue);
        self.wake.notify_one();
        Ok(self.view(id).ok())
    }

    /// Takes article `id` out of the queue and deletes its folder (the item is removed).
    pub fn forget(&self, id: &str) {
        lock(&self.queue).retain(|(queued, _)| queued != id);
        lock(&self.feeds).remove(id);
        let folder = self.dir.join(id);
        if folder.exists() {
            if let Err(error) = std::fs::remove_dir_all(&folder) {
                tracing::warn!(event = "article.forget", book = crate::logging::book(id), %error, "Could not delete the article's folder");
            }
        }
        super::item::drop_covers(&self.covers, id);
    }

    fn store_stage(&self, book: &BookId, from: &str, to: Stage, error: Option<&str>) {
        let (name, reason) = to.names();
        if let Err(error) = lock(&self.library).set_article_stage(book, name, reason, error) {
            tracing::warn!(event = "library.save", book = crate::logging::book(&book.0), %error, "Could not store the article's stage");
        }
        tracing::info!(
            event = "article.stage",
            book = crate::logging::book(&book.0),
            from,
            to = name,
            reason,
            "Article fetch stage changed"
        );
    }

    pub fn view(&self, id: &str) -> Result<ArticleView, String> {
        let row = lock(&self.library)
            .article(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("Not an article.")?;
        let feeds = lock(&self.feeds).get(id).cloned().unwrap_or_default();
        Ok(ArticleView {
            id: id.into(),
            stage: Stage::from_stored(&row.stage, row.reason.as_deref()),
            url: row.url,
            canonical_url: row.canonical_url,
            site_name: row.site_name,
            byline: row.byline,
            published_at: row.published_at,
            error: row.error,
            via: row.via.as_deref().and_then(ArticleSource::stored),
            fetched_at: row.fetched_at,
            captured_words: row.word_count.unwrap_or(0),
            declared_words: row.declared_words,
            feeds,
            in_library: true,
        })
    }

    pub fn summary(&self, id: &str) -> Option<ArticleSummary> {
        let row = lock(&self.library).article(&BookId(id.into())).ok()??;
        // A site that has not named itself goes by its address.
        let host = || {
            Url::parse(&row.canonical_url)
                .ok()
                .and_then(|u| u.host_str().map(str::to_owned))
        };
        Some(ArticleSummary {
            stage: Stage::from_stored(&row.stage, row.reason.as_deref()),
            site_name: row.site_name.clone().or_else(host),
            lead: row.lead,
            error: row.error,
        })
    }

    /// Article `id`'s stored page, read.
    pub fn document(&self, id: &str) -> Result<ParsedBook, String> {
        let row = lock(&self.library)
            .article(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("Not an article.")?;
        let html = match std::fs::read_to_string(self.snapshot_path(id)) {
            Ok(html) => html,
            Err(_) => return Err(row.error.unwrap_or_else(|| "Not fetched yet.".into())),
        };
        let url = Url::parse(&row.canonical_url).map_err(|e| e.to_string())?;
        let extracted = extract(&html, &url);
        self.keep_feeds(id, &extracted);
        Ok(extracted.document)
    }

    fn keep_feeds(&self, id: &str, extracted: &Extracted) {
        let feeds = extracted
            .feeds
            .iter()
            .map(|f| DiscoveredFeed {
                url: f.url.to_string(),
                title: f.title.clone(),
                // The core says which are subscribed as it sends the view.
                subscribed: false,
            })
            .collect();
        lock(&self.feeds).insert(id.into(), feeds);
    }

    /// Fetches queued articles, one at a time, for as long as the app runs.
    pub async fn run(&self, host: &dyn ArticlesHost) {
        loop {
            let job = lock(&self.queue).pop_front();
            match job {
                Some((id, source)) => {
                    self.fetch(&id, Getting::Routes(source), host).await;
                }
                None => self.wake.notified().await,
            }
        }
    }

    /// Takes `snapshot` (a page the user reached in the browser) as article `id`'s fetch: it is
    /// kept when fuller than the stored copy, and the article's stage follows. Returns that
    /// stage; `None` when a fetch of it is already under way.
    pub async fn receive(
        &self,
        id: &str,
        snapshot: Snapshot,
        host: &dyn ArticlesHost,
    ) -> Result<Option<Stage>, String> {
        let book = BookId(id.into());
        let row = lock(&self.library)
            .article(&book)
            .map_err(|e| e.to_string())?
            .ok_or("Not an article.")?;
        let stage = Stage::from_stored(&row.stage, row.reason.as_deref());
        let queued = next(stage, Event::Retried);
        if queued != stage {
            self.store_stage(&book, &row.stage, queued, None);
        }
        Ok(self
            .fetch(id, Getting::Given(Box::new(snapshot)), host)
            .await)
    }

    /// Fetches article `id`; returns the stage it ends in, or `None` when it was not waiting
    /// to be fetched.
    async fn fetch(&self, id: &str, getting: Getting, host: &dyn ArticlesHost) -> Option<Stage> {
        let book = BookId(id.into());
        let Ok(Some(row)) = lock(&self.library).article(&book) else {
            return None;
        };
        let stage = Stage::from_stored(&row.stage, row.reason.as_deref());
        let fetching = next(stage, Event::Started);
        if fetching == stage {
            return None;
        }
        self.store_stage(&book, &row.stage, fetching, None);
        if let Ok(view) = self.view(id) {
            host.article_changed(view, false);
        }
        let started = Instant::now();
        let path = self.snapshot_path(id);
        let stored = std::fs::read_to_string(&path).ok();
        let (to, error, written) = match Url::parse(&row.canonical_url) {
            Ok(url) => {
                let old = stored.as_deref().map(|html| extract(html, &url));
                let (best, failures) = match getting {
                    Getting::Routes(source) => self.walk(&url, source, old.as_ref(), host).await,
                    Getting::Given(snapshot) => {
                        let fresh = extract(&snapshot.html, &snapshot.final_url);
                        (Some((*snapshot, fresh)), Vec::new())
                    }
                };
                tracing::info!(
                    event = "article.fetch",
                    book = crate::logging::book(id),
                    outcome = if best.is_some() { "ok" } else { "failed" },
                    took_ms = started.elapsed().as_millis() as u64,
                    "Fetched an article"
                );
                self.settle(id, &row, fetching, &path, best, old, failures)
                    .await
            }
            Err(error) => (
                next(fetching, Event::FetchFailed),
                Some(error.to_string()),
                false,
            ),
        };
        let (from, _) = fetching.names();
        self.store_stage(&book, from, to, error.as_deref());
        if let (Stage::NeedsYou(reason), Some(site)) = (to, site_of(&row.canonical_url)) {
            super::sites::asked(&self.library, &site, reason);
        }
        if let Ok(view) = self.view(id) {
            host.article_changed(view, written);
        }
        Some(to)
    }

    /// Gets `url` along the routes `source` and the settings call for, stopping at the first
    /// whole article. Returns the fullest copy got, and why each route that failed did.
    async fn walk(
        &self,
        url: &Url,
        source: ArticleSource,
        old: Option<&Extracted>,
        host: &dyn ArticlesHost,
    ) -> (Option<(Snapshot, Extracted)>, Vec<String>) {
        let (use_, rule) = host.fetch_rules(url);
        let routes = match source {
            ArticleSource::Archive => vec![Route::Archive],
            ArticleSource::Browser => vec![Route::Browser],
            ArticleSource::Http | ArticleSource::Feed => {
                plan_fetch(use_, rule.as_ref(), old.map(|o| o.completeness))
            }
        };
        let mut best: Option<(Snapshot, Extracted)> = None;
        let mut failures = Vec::new();
        if super::browsing::browser_wanted_but_off(use_, rule.as_ref()) {
            failures.push("This site's rule asks for the browser, which is off.".to_owned());
        }
        for route in routes {
            let got = match route {
                Route::Http => self.fetcher.page(url).await.map_err(|e| e.to_string()),
                Route::Archive => self.archived(url).await.map_err(|e| e.to_string()),
                Route::Browser => {
                    let request = super::browsing::render_request(url, rule.as_ref());
                    host.render(&request)
                        .await
                        .map_err(|e| format!("The browser could not get it: {e}"))
                }
            };
            match got {
                Ok(snapshot) => {
                    let fresh = extract(&snapshot.html, &snapshot.final_url);
                    let whole = fresh.completeness == Completeness::Complete;
                    if best.as_ref().map_or(true, |(_, kept)| better(&fresh, kept)) {
                        best = Some((snapshot, fresh));
                    }
                    if whole {
                        break;
                    }
                }
                Err(error) => failures.push(error),
            }
        }
        (best, failures)
    }

    /// Stores what a fetch got (when it is fuller than the copy kept) and says where the
    /// article stands: its stage, what went wrong, and whether its text changed.
    #[allow(clippy::too_many_arguments)]
    async fn settle(
        &self,
        id: &str,
        row: &ArticleRow,
        fetching: Stage,
        path: &Path,
        best: Option<(Snapshot, Extracted)>,
        old: Option<Extracted>,
        failures: Vec<String>,
    ) -> (Stage, Option<String>, bool) {
        let failed = (!failures.is_empty()).then(|| failures.join(" "));
        let Some((snapshot, fresh)) = best else {
            return match old {
                // A copy is kept: it stays readable and the failure is said.
                Some(old) => (
                    next(fetching, Event::Got(old.completeness)),
                    failed.map(|e| format!("Could not fetch it again. {e}")),
                    false,
                ),
                None => (next(fetching, Event::FetchFailed), failed, false),
            };
        };
        // A copy is replaced only by a fuller one: more of the text, or all of it.
        if old.as_ref().is_some_and(|old| !better(&fresh, old)) {
            let old = old.expect("checked");
            self.keep_feeds(id, &old);
            return (next(fetching, Event::Got(old.completeness)), failed, false);
        }
        let source = ArticleSource::of(snapshot.via);
        match self.keep(id, row, &snapshot, &fresh, path, source) {
            Ok(()) => {
                if let Some(lead) = &fresh.meta.lead_image {
                    self.keep_cover(id, lead).await;
                }
                // Once the whole article is got, earlier routes' failures do not matter.
                let failed = failed.filter(|_| fresh.completeness != Completeness::Complete);
                (next(fetching, Event::Got(fresh.completeness)), failed, true)
            }
            Err(error) => match old {
                Some(old) => (
                    next(fetching, Event::Got(old.completeness)),
                    Some(error),
                    false,
                ),
                None => (next(fetching, Event::FetchFailed), Some(error), false),
            },
        }
    }

    /// The archive's copy of `url`, read as the page it was taken from.
    async fn archived(&self, url: &Url) -> Result<Snapshot, FetchError> {
        let copy = self.fetcher.archived(url).await?.ok_or_else(|| {
            FetchError::Refused("No archived copy of this page was found.".into())
        })?;
        let mut snapshot = self.fetcher.page(&copy).await?;
        snapshot.requested = url.clone();
        snapshot.final_url = url.clone();
        snapshot.via = Via::Archive;
        Ok(snapshot)
    }

    /// Downloads the page's lead image as article `id`'s cover. A failure leaves the cover
    /// it had.
    async fn keep_cover(&self, id: &str, lead: &Url) {
        let image = match self.fetcher.image(lead).await {
            Ok(image) => image,
            Err(error) => {
                tracing::info!(event = "article.cover", book = crate::logging::book(id), outcome = "failed", %error, "Could not fetch the article's image");
                return;
            }
        };
        let Some(path) = super::item::store_cover(
            &self.covers,
            id,
            &image.media_type,
            &image.bytes,
            super::item::Untyped::Refused,
        ) else {
            tracing::warn!(event = "article.cover", book = crate::logging::book(id), outcome = "failed", media_type = %image.media_type, "Could not store the article's image");
            return;
        };
        let library = lock(&self.library);
        if let Err(error) = library.set_cover(&BookId(id.into()), Some(&path)) {
            tracing::warn!(event = "article.cover", book = crate::logging::book(id), outcome = "failed", %error, "Could not store the article's image");
        } else {
            tracing::info!(
                event = "article.cover",
                book = crate::logging::book(id),
                outcome = "ok",
                "Stored the article's image as its cover"
            );
        }
    }

    /// Stores `snapshot` as article `id`'s copy, and what it says about itself.
    fn keep(
        &self,
        id: &str,
        row: &ArticleRow,
        snapshot: &Snapshot,
        extracted: &Extracted,
        path: &Path,
        source: ArticleSource,
    ) -> Result<(), String> {
        let folder = path.parent().ok_or("no folder")?;
        std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        let partial = folder.join("snapshot.html.part");
        std::fs::write(&partial, &snapshot.html).map_err(|e| e.to_string())?;
        std::fs::rename(&partial, path).map_err(|e| e.to_string())?;
        self.keep_feeds(id, extracted);
        let meta = &extracted.meta;
        let library = lock(&self.library);
        let book = library
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("The article was removed.")?;
        library
            .upsert_book(
                &NewBook {
                    id: book.id,
                    kind: ItemKind::Article,
                    path: path.to_owned(),
                    title: meta.title.clone().or(book.title),
                    authors: meta.byline.iter().cloned().collect(),
                    language: meta.language.clone(),
                    cover_path: book.cover_path,
                    error: None,
                },
                snapshot.fetched_at_ms,
            )
            .map_err(|e| e.to_string())?;
        // Its length for lists' reading times; opening it stores the exact count.
        let words = u32::try_from(meta.word_count).unwrap_or(u32::MAX);
        library
            .set_word_count(&BookId(id.into()), words)
            .map_err(|e| e.to_string())?;
        library
            .save_article(&ArticleRow {
                site_name: meta.site_name.clone(),
                byline: meta.byline.clone(),
                published_at: meta.published.as_ref().and_then(|p| p.at_ms),
                via: Some(source.name().into()),
                fetched_at: Some(snapshot.fetched_at_ms),
                word_count: Some(meta.word_count as u64),
                declared_words: meta.declared_words.map(|n| n as u64),
                lead: lead(extracted),
                ..row.clone()
            })
            .map_err(|e| e.to_string())
    }
}

/// Where a fetch gets the page.
enum Getting {
    /// Along the routes this source and the settings call for.
    Routes(ArticleSource),
    /// A page already got.
    Given(Box<Snapshot>),
}

/// The site an address is listed under (`www.` dropped).
pub(super) fn site_of(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    url.host_str().map(aoide_config::site_host)
}

/// Whether `fresh` is a fuller copy than `kept`: more of the text, or all of it.
fn better(fresh: &Extracted, kept: &Extracted) -> bool {
    fresh.meta.word_count > kept.meta.word_count
        || (fresh.completeness == Completeness::Complete
            && kept.completeness != Completeness::Complete)
}

/// The page's own summary, else its first paragraph, cut at about 200 characters.
fn lead(extracted: &Extracted) -> Option<String> {
    let text = extracted.document.description.clone().or_else(|| {
        extracted.document.chapters.first().and_then(|c| {
            c.blocks
                .iter()
                .find(|b| b.kind == aoide_document::BlockKind::Paragraph)
                .map(|b| b.text.clone())
        })
    })?;
    if text.chars().count() <= 200 {
        return Some(text);
    }
    let cut: String = text.chars().take(200).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    Some(format!(
        "{}…",
        cut.trim_end_matches(|c: char| c.is_ascii_punctuation())
    ))
}

/// `text` as a web address (http or https), the only kind Aoide opens in the system's browser.
pub(super) fn web_address(text: &str) -> Result<Url, String> {
    let url = Url::parse(text).map_err(|e| e.to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("That is not a web address.".into());
    }
    Ok(url)
}

pub(super) fn extract(html: &str, url: &Url) -> Extracted {
    aoide_article::extract(html, url, &ExtractOptions::default())
}

impl ArticlesHost for super::Core {
    /// The one way the window is told an article changed: the library, and the article's view
    /// with what the feeds know of it.
    fn article_changed(&self, view: ArticleView, snapshot: bool) {
        if snapshot {
            // Its text is read again from the new copy.
            self.shelf.unload(&view.id);
        }
        self.emit(super::CoreEvent::LibraryChanged);
        self.emit(super::CoreEvent::ArticleState(self.completed(view)));
    }

    fn fetch_rules(&self, url: &Url) -> (BrowserUse, Option<aoide_config::SiteRule>) {
        let rule = url.host_str().and_then(|host| self.browsing.rule_for(host));
        (self.app_settings().browser.use_, rule)
    }

    fn render<'a>(
        &'a self,
        request: &'a aoide_browser::RenderRequest,
    ) -> Pending<'a, Result<Snapshot, String>> {
        Box::pin(self.browsing.render(self, request))
    }
}

impl super::Core {
    /// Saves `link` as an article (or finds it saved, and keeps it in the library when it was
    /// only opened from a feed) and queues its fetch; returns its id.
    pub fn save_article(&self, link: &str) -> Result<String, String> {
        let (id, view) = self
            .articles
            .save(link, ItemShelf::Library, super::now_ms())?;
        self.move_shelf(&id, super::feeds::ShelfEvent::Saved)?;
        if let Some(view) = view {
            self.article_changed(view, false);
        }
        Ok(id)
    }

    /// Fetches article `id` again: from the page, an archived copy, or in the browser.
    pub fn refetch_article(&self, id: &str, source: ArticleSource) -> Result<(), String> {
        if source == ArticleSource::Browser {
            self.require_browser()?;
        }
        if let Some(view) = self.articles.queue_fetch(id, source)? {
            self.article_changed(view, false);
        }
        Ok(())
    }

    pub fn article(&self, id: &str) -> Result<ArticleView, String> {
        self.articles.view(id).map(|view| self.completed(view))
    }

    /// `view` with the feeds it names marked when they are subscribed, and whether it is kept
    /// in the library: an article view as the window is sent it.
    fn completed(&self, mut view: ArticleView) -> ArticleView {
        for feed in &mut view.feeds {
            feed.subscribed = self.feeds.subscribed(&feed.url);
        }
        view.in_library = self.feeds.shelf_of(&view.id) != Some(super::feeds::ItemShelf::Feed);
        view
    }

    /// Opens article `id`'s page, as it was kept, in the system's browser. Only web addresses
    /// are opened.
    pub fn open_original(&self, id: &str) -> Result<(), String> {
        let view = self.articles.view(id)?;
        let url = web_address(&view.canonical_url)?;
        (self.open_link)(url.as_str())
    }
}
