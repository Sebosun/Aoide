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
use aoide_fetch::{FetchError, FetchOptions, HttpFetcher, Snapshot, Via};
use aoide_library::{ArticleRow, BookId, ItemKind, Library, NewBook};
use serde::{Deserialize, Serialize};
use url::Url;

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
}

impl Fetcher for HttpFetcher {
    fn page<'a>(&'a self, url: &'a Url) -> Fetching<'a, Snapshot> {
        Box::pin(self.fetch(url))
    }
}

/// Answers every fetch with the same failure.
struct Unavailable(String);

impl Fetcher for Unavailable {
    fn page<'a>(&'a self, _: &'a Url) -> Fetching<'a, Snapshot> {
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
}

impl ArticleSource {
    fn name(self) -> &'static str {
        match self {
            ArticleSource::Http => "http",
            ArticleSource::Archive => "archive",
        }
    }

    fn of(via: Via) -> Self {
        match via {
            Via::Http => ArticleSource::Http,
            Via::Archive => ArticleSource::Archive,
        }
    }

    fn stored(name: &str) -> Option<Self> {
        match name {
            "http" => Some(ArticleSource::Http),
            "archive" => Some(ArticleSource::Archive),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FeedView {
    pub url: String,
    pub title: Option<String>,
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
    pub feeds: Vec<FeedView>,
}

/// What a library card shows of an article.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ArticleSummary {
    pub site_name: Option<String>,
    pub stage: Stage,
    pub error: Option<String>,
}

/// What the articles need from the rest of the app.
pub(super) trait ArticlesHost: Send + Sync {
    /// Article `view.id` changed; `snapshot` when its text did too.
    fn article_changed(&self, view: ArticleView, snapshot: bool);
}

pub(super) struct Articles {
    library: Arc<Mutex<Library>>,
    dir: PathBuf,
    fetcher: Arc<dyn Fetcher>,
    queue: Mutex<VecDeque<(String, ArticleSource)>>,
    wake: tokio::sync::Notify,
    /// Feeds each extracted page links to.
    feeds: Mutex<HashMap<String, Vec<FeedView>>>,
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
    pub fn new(library: Arc<Mutex<Library>>, dir: PathBuf, fetcher: Arc<dyn Fetcher>) -> Self {
        Self {
            library,
            dir,
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

    /// Saves `link` as an article and queues its fetch; returns its id. A link already saved
    /// is the same item, fetched again unless it is ready.
    pub fn save(&self, link: &str, now: i64) -> Result<(String, Option<ArticleView>), String> {
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
        let (stage, _) = Stage::Queued.names();
        library
            .save_article(&ArticleRow {
                book_id: book,
                url: link.trim().to_owned(),
                canonical_url: aoide_article::normalise(&url).to_string(),
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
            })
            .map_err(|e| e.to_string())?;
        drop(library);
        tracing::info!(
            event = "article.save",
            book = crate::logging::book(&id),
            outcome = "ok",
            "Saved an article"
        );
        lock(&self.queue).push_back((id.clone(), ArticleSource::Http));
        self.wake.notify_one();
        Ok((id.clone(), self.view(&id).ok()))
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
        })
    }

    pub fn summary(&self, id: &str) -> Option<ArticleSummary> {
        let row = lock(&self.library).article(&BookId(id.into())).ok()??;
        Some(ArticleSummary {
            stage: Stage::from_stored(&row.stage, row.reason.as_deref()),
            site_name: row.site_name,
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
            .map(|f| FeedView {
                url: f.url.to_string(),
                title: f.title.clone(),
            })
            .collect();
        lock(&self.feeds).insert(id.into(), feeds);
    }

    /// Fetches queued articles, one at a time, for as long as the app runs.
    pub async fn run(&self, host: &dyn ArticlesHost) {
        loop {
            let job = lock(&self.queue).pop_front();
            match job {
                Some((id, source)) => self.fetch(&id, source, host).await,
                None => self.wake.notified().await,
            }
        }
    }

    async fn fetch(&self, id: &str, source: ArticleSource, host: &dyn ArticlesHost) {
        let book = BookId(id.into());
        let Ok(Some(row)) = lock(&self.library).article(&book) else {
            return;
        };
        let stage = Stage::from_stored(&row.stage, row.reason.as_deref());
        let fetching = next(stage, Event::Started);
        if fetching == stage {
            return;
        }
        self.store_stage(&book, &row.stage, fetching, None);
        if let Ok(view) = self.view(id) {
            host.article_changed(view, false);
        }
        let started = Instant::now();
        let url = Url::parse(&row.canonical_url);
        let got = match &url {
            Ok(url) => self.fetcher.page(url).await,
            Err(e) => Err(FetchError::Refused(e.to_string())),
        };
        let _ = source;
        let outcome = if got.is_ok() { "ok" } else { "failed" };
        tracing::info!(
            event = "article.fetch",
            book = crate::logging::book(id),
            outcome,
            took_ms = started.elapsed().as_millis() as u64,
            "Fetched an article"
        );
        let path = self.snapshot_path(id);
        let stored = std::fs::read_to_string(&path).ok();
        let base = url.unwrap_or_else(|_| Url::parse("https://invalid.example/").unwrap());
        let (to, error, written) = match got {
            Ok(snapshot) => {
                let fresh = extract(&snapshot.html, &snapshot.final_url);
                let old = stored.as_deref().map(|html| extract(html, &base));
                let better = match &old {
                    None => true,
                    Some(old) => {
                        old.completeness != Completeness::Complete
                            || fresh.meta.word_count > old.meta.word_count
                    }
                };
                if better {
                    match self.keep(id, &row, &snapshot, &fresh, &path) {
                        Ok(()) => (next(fetching, Event::Got(fresh.completeness)), None, true),
                        Err(error) => match old {
                            Some(old) => (
                                next(fetching, Event::Got(old.completeness)),
                                Some(error),
                                false,
                            ),
                            None => (next(fetching, Event::FetchFailed), Some(error), false),
                        },
                    }
                } else {
                    let old = old.expect("a stored copy to compare with");
                    self.keep_feeds(id, &old);
                    (next(fetching, Event::Got(old.completeness)), None, false)
                }
            }
            Err(error) => match stored.as_deref().map(|html| extract(html, &base)) {
                // A copy is kept: it stays readable and the failure is said.
                Some(old) => (
                    next(fetching, Event::Got(old.completeness)),
                    Some(format!("Could not fetch it again. {error}")),
                    false,
                ),
                None => (
                    next(fetching, Event::FetchFailed),
                    Some(error.to_string()),
                    false,
                ),
            },
        };
        let (from, _) = fetching.names();
        self.store_stage(&book, from, to, error.as_deref());
        if let Ok(view) = self.view(id) {
            host.article_changed(view, written);
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
        library
            .save_article(&ArticleRow {
                site_name: meta.site_name.clone(),
                byline: meta.byline.clone(),
                published_at: meta.published.as_ref().and_then(|p| p.at_ms),
                via: Some(ArticleSource::of(snapshot.via).name().into()),
                fetched_at: Some(snapshot.fetched_at_ms),
                word_count: Some(meta.word_count as u64),
                declared_words: meta.declared_words.map(|n| n as u64),
                ..row.clone()
            })
            .map_err(|e| e.to_string())
    }
}

fn extract(html: &str, url: &Url) -> Extracted {
    aoide_article::extract(html, url, &ExtractOptions::default())
}

impl ArticlesHost for super::Core {
    fn article_changed(&self, view: ArticleView, snapshot: bool) {
        if snapshot {
            // Its text is read again from the new copy.
            self.shelf.unload(&view.id);
        }
        self.emit(super::CoreEvent::LibraryChanged);
        self.emit(super::CoreEvent::ArticleState(view));
    }
}

impl super::Core {
    /// Saves `link` as an article (or finds it saved) and queues its fetch; returns its id.
    pub fn save_article(&self, link: &str) -> Result<String, String> {
        let (id, view) = self.articles.save(link, super::now_ms())?;
        if let Some(view) = view {
            self.emit(super::CoreEvent::LibraryChanged);
            self.emit(super::CoreEvent::ArticleState(view));
        }
        Ok(id)
    }

    /// Fetches article `id` again, from the page or an archived copy.
    pub fn refetch_article(&self, id: &str, source: ArticleSource) -> Result<(), String> {
        if let Some(view) = self.articles.queue_fetch(id, source)? {
            self.emit(super::CoreEvent::ArticleState(view));
        }
        Ok(())
    }

    pub fn article(&self, id: &str) -> Result<ArticleView, String> {
        self.articles.view(id)
    }
}
