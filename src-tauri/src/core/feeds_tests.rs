//! The feeds component without a `Core`: a scripted fetcher, an in-memory library and a clock
//! the test moves.

use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use aoide_fetch::FetchError;
use aoide_library::Library;

use super::article_tests::Pages;
use super::feeds::{
    next, FeedEvent, FeedSettings, FeedStage, FeedView, Feeds, FeedsHost, SubscribeResult,
};

pub(super) const HARBOUR: &str = "https://harbour.example/feed.xml";
const PAGE: &str = "https://harbour.example/";
const MINUTE: i64 = 60_000;
const DAY: i64 = 24 * 60 * MINUTE;

/// An RSS feed with entries `g{n}` for each `n` in `items`, entry `n` published on day `n`.
pub(super) fn rss(items: std::ops::Range<i64>) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Harbour Notes</title>\
         <link>https://harbour.example/</link>",
    );
    for n in items {
        xml.push_str(&format!(
            "<item><title>Notice {n}</title><guid>g{n}</guid>\
             <link>https://harbour.example/notices/{n}</link>\
             <pubDate>{}</pubDate><description>The harbour notice for day {n}.</description></item>",
            rfc2822(n * DAY)
        ));
    }
    xml.push_str("</channel></rss>");
    xml
}

fn rfc2822(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc2822()
}

/// A page linking to `feeds` (paths on the harbour site).
fn page_with(feeds: &[&str]) -> String {
    let links: String = feeds
        .iter()
        .map(|f| format!("<link rel=\"alternate\" type=\"application/rss+xml\" href=\"{f}\">"))
        .collect();
    format!(
        "<!doctype html><html><head><title>Harbour</title>{links}</head><body><article>\
         <p>News from the harbour, every day of the year, written by the harbour master.</p>\
         </article></body></html>"
    )
}

pub(super) struct Host {
    pub now: AtomicI64,
    pub settings: Mutex<FeedSettings>,
    pub changed: AtomicUsize,
}

impl Host {
    pub fn at(now: i64) -> Self {
        Self {
            now: AtomicI64::new(now),
            settings: Mutex::new(FeedSettings::default()),
            changed: AtomicUsize::new(0),
        }
    }

    pub fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }
}

impl FeedsHost for Host {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
    fn feed_settings(&self) -> FeedSettings {
        self.settings.lock().unwrap().clone()
    }
    fn feeds_changed(&self) {
        self.changed.fetch_add(1, Ordering::SeqCst);
    }
}

pub(super) fn feeds(pages: &Arc<Pages>) -> (Feeds, Arc<Mutex<Library>>) {
    let library = Arc::new(Mutex::new(Library::open_in_memory().unwrap()));
    (Feeds::new(Arc::clone(&library), pages.clone()), library)
}

fn subscribed(result: Result<SubscribeResult, String>) -> FeedView {
    match result {
        Ok(SubscribeResult::Subscribed(view)) => view,
        other => panic!("subscribed: {other:?}"),
    }
}

fn only(feeds: &Feeds) -> FeedView {
    let mut views = feeds.views().unwrap();
    assert_eq!(views.len(), 1);
    views.remove(0)
}

#[tokio::test]
async fn subscribing_to_a_feed_keeps_its_ten_newest_entries_unread() {
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..12), "v1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(20 * DAY);

    let view = subscribed(feeds.subscribe(&host, HARBOUR).await);
    assert_eq!(view.title.as_deref(), Some("Harbour Notes"));
    assert_eq!(view.url, HARBOUR);
    assert_eq!(view.site_url.as_deref(), Some(PAGE));
    assert_eq!(view.unread, 10, "not flooded");
    assert_eq!(view.stage, FeedStage::Idle);
    assert_eq!(view.last_ok_at, Some(20 * DAY));
    assert!(host.changed.load(Ordering::SeqCst) > 0);

    // Subscribing again is the same feed, not asked for again.
    let again = subscribed(feeds.subscribe(&host, HARBOUR).await);
    assert_eq!(again.id, view.id);
    assert_eq!(pages.feed_asks(HARBOUR), 1);
}

#[tokio::test]
async fn a_page_with_one_feed_subscribes_to_it_and_with_several_offers_them() {
    let pages = Pages::with(&[(PAGE, &page_with(&["/feed.xml"]))]);
    pages.feed(HARBOUR, &rss(0..3), "v1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(DAY);
    let view = subscribed(feeds.subscribe(&host, PAGE).await);
    assert_eq!(view.url, HARBOUR);
    assert_eq!(view.unread, 3);

    let two = "https://two.example/";
    pages.set(two, Ok(page_with(&["/news.xml", "/comments.xml"])));
    let Ok(SubscribeResult::Choose(links)) = feeds.subscribe(&host, two).await else {
        panic!("a choice")
    };
    let urls: Vec<&str> = links.iter().map(|l| l.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://two.example/news.xml",
            "https://two.example/comments.xml"
        ]
    );
    assert!(links.iter().all(|l| !l.subscribed));
    assert_eq!(
        feeds.views().unwrap().len(),
        1,
        "nothing subscribed for a choice"
    );

    let none = "https://none.example/";
    pages.set(none, Ok(page_with(&[])));
    assert!(matches!(
        feeds.subscribe(&host, none).await,
        Ok(SubscribeResult::NotAFeed)
    ));
    assert!(feeds.subscribe(&host, "not a link").await.is_err());
}

#[tokio::test]
async fn checks_add_new_entries_and_ask_only_for_changes() {
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..12), "v1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(20 * DAY);
    subscribed(feeds.subscribe(&host, HARBOUR).await);

    pages.feed(HARBOUR, &rss(0..13), "v2");
    feeds.check_all(&host).await;
    assert_eq!(only(&feeds).unread, 11, "one new entry");

    // Unchanged: the stored ETag is sent and nothing is added.
    let changes = host.changed.load(Ordering::SeqCst);
    feeds.check_all(&host).await;
    assert_eq!(only(&feeds).unread, 11);
    let sent = pages.validators.lock().unwrap().last().cloned().unwrap();
    assert_eq!(sent.1.etag.as_deref(), Some("v2"));
    assert_eq!(
        host.changed.load(Ordering::SeqCst),
        changes,
        "nothing to tell"
    );
}

#[tokio::test]
async fn a_feed_that_moved_for_good_is_kept_at_its_new_address() {
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..2), "v1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(DAY);
    subscribed(feeds.subscribe(&host, HARBOUR).await);
    let new = "https://news.harbour.example/feed.xml";
    pages.feed(new, &rss(0..3), "v2");
    pages.feed_moved(HARBOUR, new);
    feeds.check_all(&host).await;
    let view = only(&feeds);
    assert_eq!(view.url, new);
    assert_eq!(view.unread, 3);
}

#[tokio::test]
async fn failures_back_off_and_a_success_clears_them() {
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..2), "v1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(DAY);
    subscribed(feeds.subscribe(&host, HARBOUR).await);
    pages.feed_fails(HARBOUR, FetchError::Status(503));

    // Due every 60 minutes while healthy.
    host.advance(59 * MINUTE);
    feeds.check_due(&host).await;
    assert_eq!(pages.feed_asks(HARBOUR), 1, "not due yet");
    host.advance(MINUTE);
    feeds.check_due(&host).await;
    let first_failure = host.now_ms();
    let view = only(&feeds);
    assert_eq!(
        view.stage,
        FeedStage::Failing {
            since_ms: first_failure
        }
    );
    assert_eq!(
        view.error.as_deref(),
        Some("The site answered with status 503.")
    );
    assert_eq!(view.last_ok_at, Some(DAY));

    // Then every 2, 4 and 8 hours.
    for (wait, failures) in [(120, 2), (240, 3)] {
        host.advance((wait - 1) * MINUTE);
        feeds.check_due(&host).await;
        assert_eq!(pages.feed_asks(HARBOUR), failures, "waits {wait} minutes");
        host.advance(MINUTE);
        feeds.check_due(&host).await;
        assert_eq!(pages.feed_asks(HARBOUR), failures + 1);
    }
    assert_eq!(
        only(&feeds).stage,
        FeedStage::Failing {
            since_ms: first_failure
        },
        "since the first failure"
    );

    pages.feed(HARBOUR, &rss(0..3), "v2");
    host.advance(480 * MINUTE);
    feeds.check_due(&host).await;
    let view = only(&feeds);
    assert_eq!(view.stage, FeedStage::Idle);
    assert_eq!(view.error, None);
    assert_eq!(view.last_ok_at, Some(host.now_ms()));
    host.advance(60 * MINUTE);
    feeds.check_due(&host).await;
    assert_eq!(pages.feed_asks(HARBOUR), 6, "back to hourly");
}

#[tokio::test]
async fn one_failing_feed_does_not_stop_the_others() {
    let pages = Pages::with(&[]);
    let tides = "https://tides.example/feed.xml";
    pages.feed(HARBOUR, &rss(0..1), "v1");
    pages.feed(tides, &rss(0..1), "t1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(DAY);
    subscribed(feeds.subscribe(&host, HARBOUR).await);
    subscribed(feeds.subscribe(&host, tides).await);
    pages.feed_fails(HARBOUR, FetchError::Timeout);
    pages.feed(tides, &rss(0..2), "t2");
    feeds.check_all(&host).await;
    let views = feeds.views().unwrap();
    assert!(matches!(views[0].stage, FeedStage::Failing { .. }));
    assert_eq!(views[1].unread, 2);
}

#[tokio::test]
async fn unsubscribing_removes_the_feed_and_its_entries() {
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..3), "v1");
    let (feeds, library) = feeds(&pages);
    let host = Host::at(DAY);
    let view = subscribed(feeds.subscribe(&host, HARBOUR).await);
    feeds.unsubscribe(&host, view.id).unwrap();
    assert!(feeds.views().unwrap().is_empty());
    assert!(library
        .lock()
        .unwrap()
        .entries_of(view.id)
        .unwrap()
        .is_empty());
}

#[test]
fn a_feeds_stage_moves_only_on_its_events() {
    use FeedEvent::*;
    use FeedStage::*;
    let failing = Failing { since_ms: 5 };
    let table = [
        (Idle, CheckStarted, Checking),
        (Checking, Checked, Idle),
        (Checking, NotModified, Idle),
        (Checking, CheckFailed { now_ms: 9 }, Failing { since_ms: 9 }),
        // A failing feed stays failing while it is tried again, since its first failure.
        (failing, CheckStarted, failing),
        (failing, CheckFailed { now_ms: 9 }, failing),
        (failing, Checked, Idle),
        (failing, NotModified, Idle),
        // Out of order: no change.
        (Idle, Checked, Idle),
        (Idle, CheckFailed { now_ms: 9 }, Idle),
        (Checking, CheckStarted, Checking),
    ];
    for (from, event, to) in table {
        assert_eq!(next(from, event), to, "{from:?} + {event:?}");
    }
}
