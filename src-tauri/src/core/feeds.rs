//! Feeds: subscribing, checking them in the background with conditional requests, and each
//! feed's health. Owns no lock but the library's (the leaf) and its own small ones; reaches the
//! rest of the app through [`FeedsHost`].

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_feeds::ParsedFeed;
use aoide_fetch::{FeedResponse, FetchError, Validators};
use aoide_library::{FeedRow, Library, NewEntry};
use serde::{Deserialize, Serialize};
use url::Url;

use super::articles::{parse_link, Fetcher};
use super::lock;

/// A feed's health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum FeedStage {
    Idle,
    Checking,
    /// Its checks fail, since the first failure of this run of them.
    #[serde(rename_all = "camelCase")]
    Failing {
        since_ms: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedEvent {
    CheckStarted,
    Checked,
    NotModified,
    CheckFailed { now_ms: i64 },
}

/// The one way a feed's stage changes. A failing feed stays failing while it is tried again,
/// so it keeps when its failures began; any success makes it idle.
pub fn next(stage: FeedStage, event: FeedEvent) -> FeedStage {
    use FeedEvent::*;
    use FeedStage::*;
    match (stage, event) {
        (Idle, CheckStarted) => Checking,
        (Checking | Failing { .. }, Checked | NotModified) => Idle,
        (Checking, CheckFailed { now_ms }) => Failing { since_ms: now_ms },
        (stage, _) => stage,
    }
}

impl FeedStage {
    /// The stage a stored feed is in when no check is running.
    fn stored(feed: &FeedRow) -> Self {
        match feed.failing_since {
            Some(since_ms) if feed.failures > 0 => Self::Failing { since_ms },
            _ => Self::Idle,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Checking => "checking",
            Self::Failing { .. } => "failing",
        }
    }
}

/// How often feeds are checked and how long entries are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FeedSettings {
    /// Minutes between checks of a healthy feed (15–1440).
    pub check_minutes: u32,
    /// Days a read entry that was not saved is kept.
    pub keep_read_days: u32,
    /// Days an unread entry is kept.
    pub keep_unread_days: u32,
}

/// Check intervals Settings offers, in minutes.
pub const CHECK_MINUTES: std::ops::RangeInclusive<u32> = 15..=1440;

impl Default for FeedSettings {
    fn default() -> Self {
        Self {
            check_minutes: 60,
            keep_read_days: 30,
            keep_unread_days: 90,
        }
    }
}

/// Entries a new subscription starts with unread; older ones are marked read.
const FIRST_UNREAD: u32 = 10;

/// A subscription as the Feeds screen lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct FeedView {
    pub id: i64,
    pub title: Option<String>,
    pub url: String,
    pub site_url: Option<String>,
    pub unread: u32,
    pub stage: FeedStage,
    /// Why the last check failed, while it fails.
    pub error: Option<String>,
    pub last_ok_at: Option<i64>,
}

/// A feed a page names.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DiscoveredFeed {
    pub url: String,
    pub title: Option<String>,
    pub subscribed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SubscribeResult {
    Subscribed(FeedView),
    /// The page names several feeds: the user picks.
    Choose(Vec<DiscoveredFeed>),
    NotAFeed,
}

/// Where an item is kept: in the library, or on the feed shelf (opened from a feed, not
/// saved; out of every library list).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemShelf {
    Library,
    Feed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfEvent {
    /// Opened from a feed's entry (a new item starts on the feed shelf).
    Opened,
    Saved,
    /// Removed from the library while its entry is still there.
    Unsaved,
}

/// The one way an item moves between shelves.
pub fn next_shelf(shelf: ItemShelf, event: ShelfEvent) -> ItemShelf {
    match (shelf, event) {
        (_, ShelfEvent::Saved) => ItemShelf::Library,
        (_, ShelfEvent::Unsaved) => ItemShelf::Feed,
        (shelf, ShelfEvent::Opened) => shelf,
    }
}

impl ItemShelf {
    fn stored(name: &str) -> Self {
        match name {
            "feed" => Self::Feed,
            _ => Self::Library,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Feed => "feed",
        }
    }
}

/// Which entries the Feeds screen lists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum EntryFilter {
    #[default]
    All,
    Unread,
    /// In the library.
    Saved,
}

impl EntryFilter {
    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Unread => "unread",
            Self::Saved => "saved",
        }
    }
}

/// A page of entries: of one feed or all, newest first, after entry `before`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryQuery {
    pub feed: Option<i64>,
    pub filter: EntryFilter,
    pub before: Option<i64>,
    /// At most this many (50 when unset).
    pub limit: Option<u32>,
}

/// An entry as the Feeds screen lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EntryView {
    pub id: i64,
    pub feed_id: i64,
    pub feed_title: Option<String>,
    pub title: Option<String>,
    /// Plain text.
    pub summary: Option<String>,
    pub url: Option<String>,
    pub published_at: Option<i64>,
    pub read: bool,
    /// Of the text the feed carried, when it carried any.
    pub words: Option<u32>,
    /// The feed carried the whole text: it opens without fetching the page.
    pub has_full_text: bool,
    /// The item it was opened as.
    pub book_id: Option<String>,
    pub in_library: bool,
}

/// Words a feed's text must have to be read as the whole article.
pub const FULL_TEXT_WORDS: u32 = 250;

fn full_text(entry: &aoide_library::EntryRow) -> Option<&str> {
    entry
        .content_html
        .as_deref()
        .filter(|_| entry.words.is_some_and(|w| w >= FULL_TEXT_WORDS))
}

/// What the feeds need from the rest of the app.
pub(super) trait FeedsHost: Send + Sync {
    fn now_ms(&self) -> i64;
    fn feed_settings(&self) -> FeedSettings;
    /// Feeds or their entries changed.
    fn feeds_changed(&self);
}

/// What one check got.
enum Got {
    NotModified,
    Feed {
        parsed: Box<ParsedFeed>,
        etag: Option<String>,
        last_modified: Option<String>,
        /// Its address now, when it moved for good.
        moved_to: Option<Url>,
    },
    Failed(String),
}

pub(super) struct Feeds {
    library: Arc<Mutex<Library>>,
    fetcher: Arc<dyn Fetcher>,
    /// Feeds being checked now.
    checking: Mutex<HashSet<i64>>,
    /// One round of checks at a time.
    round: tokio::sync::Mutex<()>,
    /// Ends the worker's wait early (a new subscription, changed settings).
    wake: tokio::sync::Notify,
}

impl Feeds {
    pub fn new(library: Arc<Mutex<Library>>, fetcher: Arc<dyn Fetcher>) -> Self {
        Self {
            library,
            fetcher,
            checking: Mutex::new(HashSet::new()),
            round: tokio::sync::Mutex::new(()),
            wake: tokio::sync::Notify::new(),
        }
    }

    /// Every subscription, oldest first.
    pub fn views(&self) -> Result<Vec<FeedView>, String> {
        let library = lock(&self.library);
        let feeds = library.feeds().map_err(|e| e.to_string())?;
        let unread = library.unread_counts().map_err(|e| e.to_string())?;
        drop(library);
        let checking = lock(&self.checking);
        Ok(feeds
            .into_iter()
            .map(|feed| self.view(&feed, unread.get(&feed.id).copied(), &checking))
            .collect())
    }

    fn view(&self, feed: &FeedRow, unread: Option<u32>, checking: &HashSet<i64>) -> FeedView {
        let stored = FeedStage::stored(feed);
        let stage = if checking.contains(&feed.id) {
            next(stored, FeedEvent::CheckStarted)
        } else {
            stored
        };
        FeedView {
            id: feed.id,
            title: feed.title.clone(),
            url: feed.url.clone(),
            site_url: feed.site_url.clone(),
            unread: unread.unwrap_or(0),
            stage,
            error: feed.error.clone(),
            last_ok_at: feed.last_ok_at,
        }
    }

    fn view_of(&self, id: i64) -> Result<FeedView, String> {
        self.views()?
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| "No such feed.".into())
    }

    /// A page of entries.
    pub fn entries(&self, query: &EntryQuery) -> Result<Vec<EntryView>, String> {
        let rows = lock(&self.library)
            .list_entries(
                query.feed,
                query.filter.name(),
                query.before,
                query.limit.unwrap_or(50).min(200),
            )
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|listing| {
                let e = listing.entry;
                EntryView {
                    has_full_text: full_text(&e).is_some(),
                    id: e.id,
                    feed_id: e.feed_id,
                    feed_title: listing.feed_title,
                    title: e.title,
                    summary: e.summary,
                    url: e.url,
                    published_at: e.published_at,
                    read: e.read_at.is_some(),
                    words: e.words,
                    book_id: e.book_id.map(|b| b.0),
                    in_library: listing.in_library,
                }
            })
            .collect())
    }

    pub fn entry(&self, id: i64) -> Result<aoide_library::EntryRow, String> {
        lock(&self.library)
            .entry(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "No such entry.".into())
    }

    fn feed_row(&self, id: i64) -> Result<FeedRow, String> {
        lock(&self.library)
            .feed(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "No such feed.".into())
    }

    pub fn mark_read(&self, id: i64, read: bool, now: i64) -> Result<(), String> {
        let library = lock(&self.library);
        let entry = library.entry(id).map_err(|e| e.to_string())?;
        let Some(entry) = entry else {
            return Err("No such entry.".into());
        };
        if entry.read_at.is_some() != read {
            library
                .set_entry_read(id, read.then_some(now))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    pub fn mark_all_read(&self, feed: Option<i64>, now: i64) -> Result<(), String> {
        lock(&self.library)
            .mark_all_read(feed, now)
            .map_err(|e| e.to_string())
    }

    fn link(&self, entry: i64, item: &str) -> Result<(), String> {
        lock(&self.library)
            .link_entry(entry, &aoide_library::BookId(item.into()))
            .map_err(|e| e.to_string())
    }

    /// Whether `url` is a subscribed feed's address.
    pub fn subscribed(&self, url: &str) -> bool {
        lock(&self.library)
            .feeds()
            .is_ok_and(|feeds| feeds.iter().any(|f| f.url == url))
    }

    /// Subscribes to the feed at `link`, or to the one feed the page at `link` names; offers
    /// the choice when it names several.
    pub async fn subscribe(
        &self,
        host: &dyn FeedsHost,
        link: &str,
    ) -> Result<SubscribeResult, String> {
        let url = parse_link(link)?;
        if let Some(view) = self.subscribe_feed(host, &url).await? {
            return Ok(SubscribeResult::Subscribed(view));
        }
        let snapshot = self.fetcher.page(&url).await.map_err(|e| e.to_string())?;
        let extracted = aoide_article::extract(
            &snapshot.html,
            &snapshot.final_url,
            &aoide_article::ExtractOptions::default(),
        );
        match extracted.feeds.as_slice() {
            [] => Ok(SubscribeResult::NotAFeed),
            [one] => Ok(match self.subscribe_feed(host, &one.url).await? {
                Some(view) => SubscribeResult::Subscribed(view),
                None => SubscribeResult::NotAFeed,
            }),
            several => Ok(SubscribeResult::Choose(
                several
                    .iter()
                    .map(|f| DiscoveredFeed {
                        url: f.url.to_string(),
                        title: f.title.clone(),
                        subscribed: self.subscribed(f.url.as_str()),
                    })
                    .collect(),
            )),
        }
    }

    /// Subscribes to `url` when it is a feed; `None` when it is not one.
    async fn subscribe_feed(
        &self,
        host: &dyn FeedsHost,
        url: &Url,
    ) -> Result<Option<FeedView>, String> {
        let existing = lock(&self.library)
            .feeds()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|f| f.url == url.as_str());
        if let Some(feed) = existing {
            return self.view_of(feed.id).map(Some);
        }
        let got = match self.fetcher.feed(url, &Validators::default()).await {
            Err(FetchError::NotAFeed(_)) => return Ok(None),
            Err(error) => return Err(error.to_string()),
            Ok(response) => read(response),
        };
        let got = match got {
            Got::Failed(_) => return Ok(None),
            got => got,
        };
        let now = host.now_ms();
        let address = match &got {
            Got::Feed {
                moved_to: Some(to), ..
            } => to.clone(),
            _ => url.clone(),
        };
        let id = lock(&self.library)
            .add_feed(address.as_str(), now)
            .map_err(|e| e.to_string())?;
        tracing::info!(
            event = "feed.subscribe",
            feed = id,
            outcome = "ok",
            "Subscribed to a feed"
        );
        self.store(id, got, now)?;
        host.feeds_changed();
        self.wake.notify_one();
        self.view_of(id).map(Some)
    }

    pub fn unsubscribe(&self, host: &dyn FeedsHost, id: i64) -> Result<(), String> {
        lock(&self.library)
            .remove_feed(id)
            .map_err(|e| e.to_string())?;
        tracing::info!(
            event = "feed.unsubscribe",
            feed = id,
            "Unsubscribed from a feed"
        );
        host.feeds_changed();
        Ok(())
    }

    /// Checks every feed that is due: `check_minutes` after its last check, doubled for each
    /// failure in a row (up to 32 times).
    pub async fn check_due(&self, host: &dyn FeedsHost) {
        let base = i64::from(
            host.feed_settings()
                .check_minutes
                .clamp(*CHECK_MINUTES.start(), *CHECK_MINUTES.end()),
        ) * 60_000;
        self.check_where(host, |feed, now| {
            feed.last_checked_at
                .map_or(true, |last| now - last >= base << feed.failures.min(5))
        })
        .await;
    }

    /// Checks every feed now.
    pub async fn check_all(&self, host: &dyn FeedsHost) {
        self.check_where(host, |_, _| true).await;
    }

    async fn check_where(&self, host: &dyn FeedsHost, due: impl Fn(&FeedRow, i64) -> bool) {
        let _round = self.round.lock().await;
        let feeds = lock(&self.library).feeds().unwrap_or_default();
        let mut changed = false;
        for feed in feeds {
            if due(&feed, host.now_ms()) {
                changed |= self.check(host, feed).await;
            }
        }
        if changed {
            host.feeds_changed();
        }
    }

    /// Checks one feed; whether anything the user sees changed.
    async fn check(&self, host: &dyn FeedsHost, feed: FeedRow) -> bool {
        let from = FeedStage::stored(&feed);
        lock(&self.checking).insert(feed.id);
        let started = Instant::now();
        let got = match Url::parse(&feed.url) {
            Ok(url) => {
                let validators = Validators {
                    etag: feed.etag.clone(),
                    last_modified: feed.last_modified.clone(),
                };
                match self.fetcher.feed(&url, &validators).await {
                    Ok(response) => read(response),
                    Err(error) => Got::Failed(error.to_string()),
                }
            }
            Err(error) => Got::Failed(error.to_string()),
        };
        let outcome = match &got {
            Got::NotModified => "not_modified",
            Got::Feed { .. } => "ok",
            Got::Failed(_) => "failed",
        };
        let changed = match self.store(feed.id, got, host.now_ms()) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::warn!(event = "library.save", feed = feed.id, %error, "Could not store a feed's check");
                false
            }
        };
        lock(&self.checking).remove(&feed.id);
        tracing::info!(
            event = "feed.check",
            feed = feed.id,
            outcome,
            took_ms = started.elapsed().as_millis() as u64,
            "Checked a feed"
        );
        let to = lock(&self.library)
            .feed(feed.id)
            .ok()
            .flatten()
            .map(|f| FeedStage::stored(&f));
        if let Some(to) = to.filter(|to| *to != from) {
            tracing::info!(
                event = "feed.stage",
                feed = feed.id,
                from = from.name(),
                to = to.name(),
                "Feed stage changed"
            );
        }
        changed || to != Some(from)
    }

    /// Stores what a check of feed `id` got at `now`; whether entries were added.
    fn store(&self, id: i64, got: Got, now: i64) -> Result<bool, String> {
        let library = lock(&self.library);
        let Some(mut feed) = library.feed(id).map_err(|e| e.to_string())? else {
            return Ok(false);
        };
        let first = feed.last_ok_at.is_none();
        let stage = next(FeedStage::stored(&feed), FeedEvent::CheckStarted);
        feed.last_checked_at = Some(now);
        let mut added = 0;
        let stage = match got {
            Got::NotModified => next(stage, FeedEvent::NotModified),
            Got::Feed {
                parsed,
                etag,
                last_modified,
                moved_to,
            } => {
                for entry in &parsed.entries {
                    let stored = library
                        .insert_entry(id, &new_entry(entry), now)
                        .map_err(|e| e.to_string())?;
                    let Some(stored) = stored else { continue };
                    added += 1;
                    // An article already kept is the item this entry opens.
                    let item = entry.url.as_ref().map(aoide_article::canonical_id);
                    if let Some(item) = item.map(aoide_library::BookId) {
                        if library.book(&item).is_ok_and(|b| b.is_some()) {
                            library
                                .link_entry(stored, &item)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
                if first {
                    library
                        .mark_read_except_newest(id, FIRST_UNREAD, now)
                        .map_err(|e| e.to_string())?;
                }
                let ParsedFeed {
                    title, site_url, ..
                } = *parsed;
                feed.title = title.or(feed.title);
                feed.site_url = site_url.map(String::from).or(feed.site_url);
                feed.etag = etag;
                feed.last_modified = last_modified;
                if let Some(to) = moved_to {
                    feed.url = to.to_string();
                }
                next(stage, FeedEvent::Checked)
            }
            Got::Failed(error) => {
                feed.error = Some(error);
                feed.failures += 1;
                next(stage, FeedEvent::CheckFailed { now_ms: now })
            }
        };
        match stage {
            FeedStage::Failing { since_ms } => feed.failing_since = Some(since_ms),
            _ => {
                feed.failures = 0;
                feed.failing_since = None;
                feed.error = None;
                feed.last_ok_at = Some(now);
            }
        }
        library.update_feed(&feed).map_err(|e| e.to_string())?;
        Ok(added > 0)
    }

    /// The check interval changed: feeds due sooner are checked now.
    pub fn settings_changed(&self) {
        self.wake.notify_one();
    }

    /// Checks feeds as they fall due, for as long as the app runs.
    pub async fn run(&self, host: &dyn FeedsHost) {
        loop {
            self.check_due(host).await;
            let _ = tokio::time::timeout(Duration::from_secs(60), self.wake.notified()).await;
        }
    }
}

/// What a feed response holds.
fn read(response: FeedResponse) -> Got {
    match response {
        FeedResponse::NotModified => Got::NotModified,
        FeedResponse::Body {
            bytes,
            etag,
            last_modified,
            final_url,
            moved,
        } => match aoide_feeds::parse(&bytes, &final_url) {
            Ok(parsed) => Got::Feed {
                parsed: Box::new(parsed),
                etag,
                last_modified,
                moved_to: moved.then_some(final_url),
            },
            Err(error) => Got::Failed(error.to_string()),
        },
    }
}

/// Characters of plain text kept as an entry's summary.
const SUMMARY_CHARS: usize = 400;

fn new_entry(entry: &aoide_feeds::Entry) -> NewEntry {
    let summary = entry
        .summary_html
        .as_deref()
        .or(entry.content_html.as_deref())
        .map(aoide_feeds::plain_text)
        .filter(|s| !s.is_empty())
        .map(|s| cut(&s, SUMMARY_CHARS));
    NewEntry {
        guid: entry.guid.clone(),
        url: entry.url.as_ref().map(Url::to_string),
        title: entry.title.clone(),
        author: entry.author.clone(),
        published_at: entry.published_ms,
        summary,
        content_html: entry.content_html.clone(),
        words: entry
            .content_html
            .as_deref()
            .map(|html| u32::try_from(aoide_feeds::word_count(html)).unwrap_or(u32::MAX)),
    }
}

/// `text` cut at a word within `max` characters, with an ellipsis when cut.
fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let head: String = text.chars().take(max).collect();
    let head = head.rsplit_once(' ').map_or(head.as_str(), |(h, _)| h);
    format!("{}…", head.trim_end())
}

impl FeedsHost for super::Core {
    fn now_ms(&self) -> i64 {
        super::now_ms()
    }

    fn feed_settings(&self) -> FeedSettings {
        self.app_settings().feeds
    }

    fn feeds_changed(&self) {
        self.emit(super::CoreEvent::FeedsChanged);
    }
}

impl super::Core {
    /// Every subscription, oldest first.
    pub fn feeds(&self) -> Result<Vec<FeedView>, String> {
        self.feeds.views()
    }

    /// Subscribes to a feed, by its address or a page naming it.
    pub async fn subscribe(&self, url: &str) -> Result<SubscribeResult, String> {
        self.feeds.subscribe(self, url).await
    }

    pub fn unsubscribe(&self, id: i64) -> Result<(), String> {
        self.feeds.unsubscribe(self, id)
    }

    /// Checks every feed now ("Check now").
    pub async fn check_feeds(&self) {
        self.feeds.check_all(self).await;
    }
}

/// The minimal page an entry's whole text is read from: its title, author and date, and the
/// feed's name as the site's.
fn entry_document(entry: &aoide_library::EntryRow, site: Option<&str>, html: &str) -> String {
    let escape = |text: &str| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    let title = escape(entry.title.as_deref().unwrap_or_default());
    let mut head = format!("<meta charset=\"utf-8\"><title>{title}</title>");
    if let Some(author) = &entry.author {
        head.push_str(&format!(
            "<meta name=\"author\" content=\"{}\">",
            escape(author)
        ));
    }
    if let Some(site) = site {
        head.push_str(&format!(
            "<meta property=\"og:site_name\" content=\"{}\">",
            escape(site)
        ));
    }
    if let Some(at) = entry
        .published_at
        .and_then(chrono::DateTime::from_timestamp_millis)
    {
        head.push_str(&format!(
            "<meta property=\"article:published_time\" content=\"{}\">",
            at.to_rfc3339()
        ));
    }
    format!(
        "<!doctype html><html><head>{head}</head><body><article><h1>{title}</h1>{html}</article></body></html>"
    )
}

impl super::Core {
    /// A page of entries.
    pub fn feed_entries(&self, query: &EntryQuery) -> Result<Vec<EntryView>, String> {
        self.feeds.entries(query)
    }

    /// Opens entry `id`: the item to read (made on the feed shelf when there is none yet,
    /// from the feed's own text when it carried all of it, else fetched from its page). The
    /// entry is marked read.
    pub fn open_feed_entry(&self, id: i64) -> Result<String, String> {
        let item = self.entry_item(id)?;
        self.feeds.mark_read(id, true, super::now_ms())?;
        self.emit(super::CoreEvent::FeedsChanged);
        Ok(item)
    }

    /// Keeps entry `id` in the library; returns its item.
    pub fn save_feed_entry(&self, id: i64) -> Result<String, String> {
        let item = self.entry_item(id)?;
        self.move_shelf(&item, ShelfEvent::Saved)?;
        Ok(item)
    }

    /// Marks entry `id` read or unread.
    pub fn mark_entry_read(&self, id: i64, read: bool) -> Result<(), String> {
        self.feeds.mark_read(id, read, super::now_ms())?;
        self.emit(super::CoreEvent::FeedsChanged);
        Ok(())
    }

    /// Marks every entry read: of `feed`, or of all feeds.
    pub fn mark_all_read(&self, feed: Option<i64>) -> Result<(), String> {
        self.feeds.mark_all_read(feed, super::now_ms())?;
        self.emit(super::CoreEvent::FeedsChanged);
        Ok(())
    }

    /// Instead of deleting item `id` from the library, returns it to the feed shelf when an
    /// entry still points at it; whether it did.
    pub(super) fn unsave(&self, id: &str) -> Result<bool, String> {
        let book = aoide_library::BookId(id.into());
        let (shelf, entry) = {
            let library = lock(&self.library);
            let shelf = library
                .book(&book)
                .map_err(|e| e.to_string())?
                .map(|b| ItemShelf::stored(&b.shelf));
            (
                shelf,
                library.entry_for_book(&book).map_err(|e| e.to_string())?,
            )
        };
        if shelf != Some(ItemShelf::Library) || entry.is_none() {
            return Ok(false);
        }
        self.move_shelf(id, ShelfEvent::Unsaved)?;
        Ok(true)
    }

    fn move_shelf(&self, id: &str, event: ShelfEvent) -> Result<(), String> {
        let book = aoide_library::BookId(id.into());
        let library = lock(&self.library);
        let from = library
            .book(&book)
            .map_err(|e| e.to_string())?
            .map(|b| ItemShelf::stored(&b.shelf))
            .ok_or("The item was removed.")?;
        let to = next_shelf(from, event);
        if to != from {
            library
                .set_shelf(&book, to.name())
                .map_err(|e| e.to_string())?;
            drop(library);
            tracing::info!(
                event = "item.shelf",
                book = crate::logging::book(id),
                from = from.name(),
                to = to.name(),
                "Item moved shelves"
            );
            self.emit(super::CoreEvent::LibraryChanged);
            self.emit(super::CoreEvent::FeedsChanged);
        }
        Ok(())
    }

    /// The item entry `id` opens as, made on the feed shelf when there is none.
    fn entry_item(&self, id: i64) -> Result<String, String> {
        let entry = self.feeds.entry(id)?;
        if let Some(book) = &entry.book_id {
            if self.articles.exists(&book.0) {
                return Ok(book.0.clone());
            }
        }
        let feed = self.feeds.feed_row(entry.feed_id)?;
        // An entry with its whole text and no link is addressed within its feed.
        let link = match (&entry.url, full_text(&entry)) {
            (Some(url), _) => url.clone(),
            (None, Some(_)) => format!("{}#{}", feed.url, entry.guid),
            (None, None) => return Err("This entry has no link and no text to read.".into()),
        };
        let url = parse_link(&link)?;
        let item = aoide_article::canonical_id(&url);
        if !self.articles.exists(&item) {
            let now = super::now_ms();
            match full_text(&entry) {
                Some(html) => {
                    let page = entry_document(&entry, feed.title.as_deref(), html);
                    self.articles.create_from_feed(&url, &link, page, now)?;
                }
                None => {
                    self.articles.save(&link, now)?;
                }
            }
            let shelf = next_shelf(ItemShelf::Feed, ShelfEvent::Opened);
            lock(&self.library)
                .set_shelf(&aoide_library::BookId(item.clone()), shelf.name())
                .map_err(|e| e.to_string())?;
        }
        self.feeds.link(id, &item)?;
        Ok(item)
    }
}
