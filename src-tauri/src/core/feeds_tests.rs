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

#[test]
fn an_item_moves_shelves_once_and_says_from_where() {
    use super::feeds::{ItemShelf, ShelfEvent};
    use aoide_library::{BookId, ItemKind, NewBook};

    let pages = Pages::with(&[]);
    let (feeds, library) = feeds(&pages);
    let book = BookId("harbour.example/notices/1".into());
    {
        let library = library.lock().unwrap();
        library
            .upsert_book(
                &NewBook {
                    id: book.clone(),
                    kind: ItemKind::Article,
                    path: "articles/1/snapshot.html".into(),
                    title: None,
                    authors: vec![],
                    language: None,
                    cover_path: None,
                    error: None,
                },
                0,
            )
            .unwrap();
        library.set_shelf(&book, "feed").unwrap();
    }
    assert_eq!(feeds.move_shelf(&book.0, ShelfEvent::Opened), Ok(None));
    assert_eq!(
        feeds.move_shelf(&book.0, ShelfEvent::Saved),
        Ok(Some((ItemShelf::Feed, ItemShelf::Library)))
    );
    assert_eq!(feeds.shelf_of(&book.0), Some(ItemShelf::Library));
    assert_eq!(
        feeds.move_shelf(&book.0, ShelfEvent::Saved),
        Ok(None),
        "already there"
    );
    assert!(feeds.move_shelf("gone", ShelfEvent::Saved).is_err());
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
    fn forget_items(&self, _: &[String]) {}
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
async fn a_feed_served_as_a_page_or_plain_text_is_still_a_feed() {
    let pages = Pages::with(&[]);
    pages.feed_as(HARBOUR, &rss(0..2), "v1", "text/html");
    let plain = "https://plain.example/index.rss";
    pages.feed_as(plain, &rss(0..1), "v1", "text/plain");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(DAY);

    assert_eq!(subscribed(feeds.subscribe(&host, HARBOUR).await).unread, 2);
    assert_eq!(subscribed(feeds.subscribe(&host, plain).await).unread, 1);
    assert!(pages.asks() == 0, "no page fetched for a feed");
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

#[tokio::test]
async fn an_opml_list_adds_the_feeds_not_subscribed_and_the_first_check_fills_them() {
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..12), "v1");
    let (feeds, _) = feeds(&pages);
    let host = Host::at(20 * DAY);
    let opml = r#"<?xml version="1.0"?><opml version="2.0"><head><title>Mine</title></head><body>
        <outline text="News"><outline text="Harbour" type="rss" xmlUrl="https://harbour.example/feed.xml"/>
        <outline text="Again" type="rss" xmlUrl="https://harbour.example/feed.xml"/></outline>
        <outline text="Local file" type="rss" xmlUrl="file:///etc/feed.xml"/>
        </body></opml>"#;
    let result = feeds.import(&host, opml).unwrap();
    assert_eq!((result.added, result.skipped), (1, 2));
    let view = only(&feeds);
    assert_eq!(
        view.title.as_deref(),
        Some("Harbour"),
        "named by the list until checked"
    );
    assert!(host.changed.load(Ordering::SeqCst) > 0);

    feeds.check_due(&host).await;
    let view = only(&feeds);
    assert_eq!(view.title.as_deref(), Some("Harbour Notes"));
    assert_eq!(view.unread, 10, "a first check, as for a new subscription");

    let written = feeds.export().unwrap();
    let outlines = aoide_feeds::opml::read(&written);
    assert_eq!(outlines.len(), 1);
    assert_eq!(outlines[0].xml_url, HARBOUR);
    assert_eq!(outlines[0].title.as_deref(), Some("Harbour Notes"));
    assert_eq!(outlines[0].html_url.as_deref(), Some(PAGE));
}

#[tokio::test]
async fn old_entries_go_unless_saved_and_items_no_entry_needs_are_named() {
    use aoide_library::{BookId, ItemKind, NewBook, NewEntry};
    let pages = Pages::with(&[]);
    pages.feed(HARBOUR, &rss(0..0), "v1");
    let (feeds, library) = feeds(&pages);
    let host = Host::at(DAY);
    let feed = subscribed(feeds.subscribe(&host, HARBOUR).await).id;
    let item = |id: &str, shelf: &str| {
        let library = library.lock().unwrap();
        let book = BookId(id.into());
        library
            .upsert_book(
                &NewBook {
                    id: book.clone(),
                    kind: ItemKind::Article,
                    path: format!("articles/{id}/snapshot.html").into(),
                    title: None,
                    authors: vec![],
                    language: None,
                    cover_path: None,
                    error: None,
                },
                0,
            )
            .unwrap();
        library.set_shelf(&book, shelf).unwrap();
        book
    };
    // (guid, seen on day, read, item and its shelf)
    let entries = [
        ("read-old", 0, true, None),
        ("read-new", 80, true, None),
        ("unread-old", 0, false, None),
        ("unread-mid", 40, false, None),
        ("saved-old", 0, true, Some(("saved", "library"))),
        ("opened-old", 0, true, Some(("opened", "feed"))),
        ("opened-new", 80, true, Some(("kept", "feed"))),
    ];
    for (guid, day, read, linked) in entries {
        let library_lock = library.lock().unwrap();
        let entry = NewEntry {
            guid: guid.into(),
            url: None,
            title: Some(guid.into()),
            author: None,
            published_at: None,
            summary: None,
            content_html: None,
            words: None,
        };
        let id = library_lock
            .insert_entry(feed, &entry, DAY + day * DAY)
            .unwrap()
            .unwrap();
        if read {
            library_lock
                .set_entry_read(id, Some(DAY + day * DAY))
                .unwrap();
        }
        drop(library_lock);
        if let Some((name, shelf)) = linked {
            let book = item(name, shelf);
            library.lock().unwrap().link_entry(id, &book).unwrap();
        }
    }
    item("orphan", "feed");
    item("mine", "library");

    // Day 101: read entries are kept 30 days, unread ones 90.
    host.now.store(DAY + 100 * DAY, Ordering::SeqCst);
    let mut orphans = feeds.clean(&host).unwrap();
    orphans.sort();
    let left: Vec<String> = library
        .lock()
        .unwrap()
        .entries_of(feed)
        .unwrap()
        .into_iter()
        .map(|e| e.guid)
        .collect();
    let mut left = left;
    left.sort();
    assert_eq!(left, ["opened-new", "read-new", "saved-old", "unread-mid"]);
    // Items on the feed shelf that no entry opens any more; saved items are never named.
    assert_eq!(orphans, ["opened", "orphan"]);
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

// --- Opening, saving and unsaving entries, through the core -----------------------------------

mod through_core {
    use aoide_tts::testing::FakeEngine;

    use super::super::article_tests::{Pages, LIGHTS, LIGHTS_URL};
    use super::super::articles::ArticleSource;
    use super::super::feeds::{
        next_shelf, AddedLink, EntryFilter, EntryQuery, ItemShelf, ShelfEvent,
    };
    use super::super::fetching::Stage;
    use super::super::tests::{library_folder, start_fetching, until};
    use super::super::Core;
    use super::HARBOUR;
    use std::sync::Arc;

    /// A feed with one entry carrying its whole text, and one linking to `LIGHTS_URL` with a
    /// summary only.
    fn feed() -> String {
        let words =
            "The night ferry crossed in calm water with the lamps lit along the quay. ".repeat(22);
        format!(
            "<?xml version=\"1.0\"?><rss version=\"2.0\" \
             xmlns:content=\"http://purl.org/rss/1.0/modules/content/\"><channel>\
             <title>Harbour Notes</title><link>https://harbour.example/</link>\
             <item><title>The night ferry</title><guid>full</guid>\
             <link>https://harbour.example/ferry</link><author>Coll Fenwick</author>\
             <pubDate>Tue, 03 Mar 2026 08:00:00 GMT</pubDate>\
             <description>A short summary.</description>\
             <content:encoded><![CDATA[<p>{words}</p><p>The end of the crossing.</p>]]></content:encoded></item>\
             <item><title>The Quay Lights</title><guid>summary</guid><link>{LIGHTS_URL}</link>\
             <pubDate>Mon, 02 Mar 2026 08:00:00 GMT</pubDate>\
             <description>Lamps lit by hand.</description></item>\
             </channel></rss>"
        )
    }

    async fn subscribed(pages: &Arc<Pages>) -> (tempfile::TempDir, Arc<Core>) {
        pages.feed(HARBOUR, &feed(), "v1");
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        core.subscribe(HARBOUR).await.unwrap();
        (dir, core)
    }

    fn entry(core: &Core, guid_title: &str) -> super::super::feeds::EntryView {
        core.feed_entries(&EntryQuery::default())
            .unwrap()
            .into_iter()
            .find(|e| e.title.as_deref() == Some(guid_title))
            .unwrap()
    }

    fn in_library(core: &Core, id: &str) -> bool {
        core.list_books().unwrap().iter().any(|b| b.id == id)
    }

    #[tokio::test]
    async fn an_entry_with_its_whole_text_opens_without_fetching_the_page() {
        let pages = Pages::with(&[]);
        let (_dir, core) = subscribed(&pages).await;
        let ferry = entry(&core, "The night ferry");
        assert!(ferry.has_full_text);
        assert!(!ferry.read);
        assert_eq!(ferry.feed_title.as_deref(), Some("Harbour Notes"));

        let id = core.open_feed_entry(ferry.id).unwrap();
        let article = core.article(&id).unwrap();
        assert_eq!(article.via, Some(ArticleSource::Feed));
        assert_eq!(article.stage, Stage::Ready);
        assert_eq!(pages.asks(), 0, "no page was fetched");
        assert!(!in_library(&core, &id), "not saved");
        let book = core.open_book(&id).unwrap();
        assert_eq!(book.title, "The night ferry");
        assert_eq!(book.authors, ["Coll Fenwick"]);
        assert!(core.chapter(&id, 0).unwrap().blocks.len() >= 2);

        let opened = entry(&core, "The night ferry");
        assert!(opened.read);
        assert_eq!(opened.book_id.as_deref(), Some(id.as_str()));
        assert!(!opened.in_library);
        // Opening it again is the same item.
        assert_eq!(core.open_feed_entry(ferry.id).unwrap(), id);
    }

    #[tokio::test]
    async fn an_entry_whose_page_fell_short_says_what_it_needs() {
        let pages = Pages::with(&[(LIGHTS_URL, &super::super::article_tests::cut_short())]);
        let (_dir, core) = subscribed(&pages).await;
        assert_eq!(entry(&core, "The Quay Lights").needs_you, None);
        let id = core
            .open_feed_entry(entry(&core, "The Quay Lights").id)
            .unwrap();
        until("the page is fetched", || {
            core.article(&id)
                .is_ok_and(|a| matches!(a.stage, Stage::NeedsYou(_)))
        })
        .await;
        assert_eq!(
            entry(&core, "The Quay Lights").needs_you,
            Some(super::super::fetching::Reason::CutShort)
        );
        assert_eq!(entry(&core, "The night ferry").needs_you, None);
    }

    #[tokio::test]
    async fn a_summary_only_entry_fetches_its_page() {
        let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
        let (_dir, core) = subscribed(&pages).await;
        let lights = entry(&core, "The Quay Lights");
        assert!(!lights.has_full_text);
        let id = core.open_feed_entry(lights.id).unwrap();
        until("the page is fetched", || {
            core.article(&id).is_ok_and(|a| a.stage == Stage::Ready)
        })
        .await;
        assert_eq!(pages.asks(), 1);
        assert_eq!(core.article(&id).unwrap().via, Some(ArticleSource::Http));
        assert!(!in_library(&core, &id));
    }

    #[tokio::test]
    async fn saving_puts_an_entry_in_the_library_and_removing_returns_it_to_the_feed() {
        let pages = Pages::with(&[]);
        let (_dir, core) = subscribed(&pages).await;
        let ferry = entry(&core, "The night ferry");

        // Saved without being opened first.
        let id = core.save_feed_entry(ferry.id).unwrap();
        assert!(in_library(&core, &id));
        assert!(entry(&core, "The night ferry").in_library);
        let saved = core
            .feed_entries(&EntryQuery {
                filter: EntryFilter::Saved,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(saved.len(), 1);

        core.remove_book(&id).unwrap();
        assert!(!in_library(&core, &id));
        assert!(core.article(&id).is_ok(), "kept on the feed shelf");
        let back = entry(&core, "The night ferry");
        assert!(!back.in_library);
        assert_eq!(back.book_id.as_deref(), Some(id.as_str()));
    }

    #[tokio::test]
    async fn the_reader_saves_an_item_opened_from_a_feed() {
        let pages = Pages::with(&[]);
        let (_dir, core) = subscribed(&pages).await;
        let id = core
            .open_feed_entry(entry(&core, "The night ferry").id)
            .unwrap();
        assert!(!core.article(&id).unwrap().in_library);

        core.save_to_library(&id).unwrap();
        assert!(core.article(&id).unwrap().in_library);
        assert!(in_library(&core, &id));
        assert!(entry(&core, "The night ferry").in_library);
        // Saving again changes nothing.
        core.save_to_library(&id).unwrap();
        assert!(in_library(&core, &id));
    }

    #[tokio::test]
    async fn a_link_added_is_subscribed_when_it_is_a_feed_and_saved_when_it_is_a_page() {
        let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
        pages.feed(HARBOUR, &feed(), "v1");
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());

        match core.add_link(HARBOUR).await.unwrap() {
            AddedLink::Subscribed(feed) => assert_eq!(feed.url, HARBOUR),
            other => panic!("{other:?}"),
        }
        let id = match core.add_link(LIGHTS_URL).await.unwrap() {
            AddedLink::Saved(id) => id,
            other => panic!("{other:?}"),
        };
        assert!(in_library(&core, &id));
        assert!(core.add_link("not a link").await.is_err());
    }

    #[tokio::test]
    async fn adding_the_link_of_an_item_opened_from_a_feed_keeps_it() {
        let pages = Pages::with(&[]);
        let (_dir, core) = subscribed(&pages).await;
        let id = core
            .open_feed_entry(entry(&core, "The night ferry").id)
            .unwrap();
        assert!(!in_library(&core, &id));

        let added = core.add_link("https://harbour.example/ferry").await;
        assert_eq!(added, Ok(AddedLink::Saved(id.clone())));
        assert!(in_library(&core, &id));
        assert!(entry(&core, "The night ferry").in_library);
    }

    #[tokio::test]
    async fn subscriptions_go_out_to_an_opml_file_and_come_in_from_one() {
        let pages = Pages::with(&[]);
        let (dir, core) = subscribed(&pages).await;
        let file = dir.path().join("subscriptions.opml");
        assert_eq!(core.export_opml(&file.to_string_lossy()).unwrap(), 1);
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains(HARBOUR));

        let list = dir.path().join("more.opml");
        std::fs::write(
            &list,
            format!(
                r#"<opml version="2.0"><body><outline text="Harbour" xmlUrl="{HARBOUR}"/>
                <outline text="Lanterns" xmlUrl="https://lanterns.example/feed.xml"/></body></opml>"#
            ),
        )
        .unwrap();
        let result = core.import_opml(&list.to_string_lossy()).unwrap();
        assert_eq!((result.added, result.skipped), (1, 1));
        assert_eq!(core.feeds().unwrap().len(), 2);
        assert!(core.import_opml("/no/such/file.opml").is_err());
    }

    #[tokio::test]
    async fn a_feed_opens_its_site_in_the_browser() {
        let pages = Pages::with(&[]);
        pages.feed(HARBOUR, &feed(), "v1");
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let (core, events) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let feed = match core.subscribe(HARBOUR).await.unwrap() {
            super::SubscribeResult::Subscribed(feed) => feed,
            other => panic!("{other:?}"),
        };
        core.open_feed_site(feed.id).unwrap();
        assert_eq!(
            events.payloads("test://opened-link"),
            [serde_json::json!("https://harbour.example/")]
        );
    }

    #[tokio::test]
    async fn an_entry_for_an_article_already_saved_opens_that_article() {
        let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
        pages.feed(HARBOUR, &feed(), "v1");
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let saved = core.save_article(LIGHTS_URL).unwrap();
        until("the article is fetched", || {
            core.article(&saved).is_ok_and(|a| a.stage == Stage::Ready)
        })
        .await;
        core.subscribe(HARBOUR).await.unwrap();
        let lights = entry(&core, "The Quay Lights");
        assert!(lights.in_library);
        assert_eq!(core.open_feed_entry(lights.id).unwrap(), saved);
        assert_eq!(pages.asks(), 1, "not fetched again");
        assert!(in_library(&core, &saved));
        // Removing it from the library keeps it for the feed.
        core.remove_book(&saved).unwrap();
        assert!(core.article(&saved).is_ok());
    }

    #[tokio::test]
    async fn items_no_entry_opens_are_deleted_with_their_folders() {
        let pages = Pages::with(&[]);
        let (dir, core) = subscribed(&pages).await;
        let ferry = entry(&core, "The night ferry");
        let id = core.open_feed_entry(ferry.id).unwrap();
        let folder = dir.path().join(format!("data/articles/{id}"));
        assert!(folder.exists());
        let feed = core.feeds().unwrap()[0].id;
        core.unsubscribe(feed).unwrap();
        assert!(core.article(&id).is_err(), "gone with its feed");
        assert!(!folder.exists());
    }

    #[test]
    fn an_items_shelf_moves_only_on_its_events() {
        use ItemShelf::*;
        use ShelfEvent::*;
        let table = [
            (Feed, Opened, Feed),
            (Library, Opened, Library),
            (Feed, Saved, Library),
            (Library, Saved, Library),
            (Library, Unsaved, Feed),
            (Feed, Unsaved, Feed),
        ];
        for (from, event, to) in table {
            assert_eq!(next_shelf(from, event), to, "{from:?} + {event:?}");
        }
    }
}
