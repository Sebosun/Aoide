# Phase 5: feeds

Read [plan.md](plan.md) first. Mockups: `design/Feeds.dc.html`, the feed offer in
`design/Add.dc.html`, the sidebar entry in `design/Main.dc.html`.

## Outcome

Subscribe to RSS, Atom or JSON feeds. New entries show up in a Feeds screen. Opening one reads
it in the normal reader; saving one keeps it in the library. Feeds are checked in the
background with conditional requests. Entries that were never saved are cleaned up after a
while.

## What exists after phase 4

`aoide-fetch` (HTTP with the `Reach` rule), `aoide-article` (`extract`, and `feeds:
Vec<FeedLink>` discovered on a page), `core/articles.rs` (save, fetch worker, stages), kind
`article`, the Add dialog's link field.

## New crate: `crates/feeds` (`aoide-feeds`): pure, no network

```
pub fn parse(bytes: &[u8], feed_url: &Url) -> Result<ParsedFeed, FeedError>
pub struct ParsedFeed { pub title: Option<String>, pub site_url: Option<Url>, pub entries: Vec<Entry> }
pub struct Entry {
    pub guid: String,                 // the feed's id; else the link; else a hash of title+date
    pub url: Option<Url>,             // resolved against the feed URL
    pub title: Option<String>, pub author: Option<String>,
    pub published_ms: Option<i64>,
    pub summary_html: Option<String>, pub content_html: Option<String>,
}
pub fn looks_like_feed(content_type: Option<&str>, head: &[u8]) -> bool
pub mod opml { pub fn read(xml: &str) -> Vec<Outline>; pub fn write(feeds: &[Outline]) -> String }
```

Wrap a feed-parsing library (see plan.md) behind this API. Fixtures in `crates/feeds/tests/`:
RSS 2.0, Atom, JSON Feed, relative links, missing ids, missing dates, entity-encoded titles,
a feed with full content and one with summaries only, malformed XML (an error, not a panic),
a 10 MiB feed (bounded by the caller's `max_bytes`; the parser keeps at most 500 entries,
newest first).

`aoide-fetch` gains `fetch_feed(url, validators) -> FeedResponse { NotModified | Body { bytes,
etag, last_modified, final_url } }`: same redirect, size, timeout and `Reach` rules as for
pages; sends `If-None-Match` / `If-Modified-Since`; accepts XML and JSON content types.

## Data (library v10)

```
ALTER TABLE books ADD COLUMN shelf TEXT NOT NULL DEFAULT 'library'
  CHECK(shelf IN ('library','feed'));

CREATE TABLE feeds (
  id INTEGER PRIMARY KEY,
  url TEXT NOT NULL UNIQUE, title TEXT, site_url TEXT,
  added_at INTEGER NOT NULL,
  last_checked_at INTEGER, last_ok_at INTEGER,
  etag TEXT, last_modified TEXT,
  error TEXT, failures INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE feed_entries (
  id INTEGER PRIMARY KEY,
  feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
  guid TEXT NOT NULL, url TEXT, title TEXT, author TEXT,
  published_at INTEGER, seen_at INTEGER NOT NULL,
  summary TEXT,                      -- plain text, at most 400 characters
  content_html TEXT,                 -- only when the feed carries the full text
  words INTEGER,                     -- of content_html, when present
  read_at INTEGER,
  book_id TEXT REFERENCES books(id) ON DELETE SET NULL,
  UNIQUE(feed_id, guid)
);
CREATE INDEX feed_entries_feed ON feed_entries(feed_id, published_at);
```

- `Library::books()` and everything built on it (`list_books`, `home`, stats lists, storage
  per-book rows) return shelf `library` only. Add `Library::feed_shelf_books()` for cleanup.
- An entry's summary is the feed's summary or content with markup removed.

## Shelf: opening an entry without keeping it

- Opening an entry needs a real item so the reader, reading place and narration work. The core
  creates the article item with `shelf = feed` (or finds it by canonical id) and links it
  through `feed_entries.book_id`.
- If the article is already in the library, the entry links to that item and shows "in your
  library".
- Where the text comes from:
  - the feed carried full content (`content_html` with at least 250 words): the snapshot is
    built from it (wrapped in a minimal HTML document with the entry's title, author and
    date), `via: Feed`, no page fetch;
  - otherwise the page is fetched as in phase 4.
- **Save** (`save_feed_entry(entry_id)`) sets `shelf = library`. **Remove from library** on an
  item that came from a feed sets it back to `feed` when its entry still exists, else deletes
  it.
- `Shelf` is an explicit enum in the core; `next(shelf, event)` with events `Opened`, `Saved`,
  `Unsaved`.

## State: a feed's health (`src-tauri/src/core/feeds.rs`, new)

```
enum FeedStage { Idle, Checking, Failing { since_ms: i64 } }
enum FeedEvent { CheckStarted, Checked, NotModified, CheckFailed { now_ms: i64 } }
fn next(stage: FeedStage, event: FeedEvent) -> FeedStage
```

`Failing.since_ms` is the first failure of the current run of failures; a success returns to
`Idle` and clears `error` and `failures`.

## Checking feeds

- One worker, one feed at a time. At start, then on a timer. A feed is due when
  `now - last_checked_at >= interval`, with `interval = base * 2^min(failures, 5)`.
- `AppSettings.feeds: FeedSettings { check_minutes: u32 (default 60, 15–1440), keep_read_days:
  u32 (30), keep_unread_days: u32 (90) }`.
- `check_feeds()` (command) checks every feed now, ignoring intervals; it backs the "Check
  now" button.
- A check: `fetch_feed` with stored validators → `NotModified` updates `last_checked_at` only;
  a body is parsed, new entries (by `(feed_id, guid)`) are inserted, existing ones are left
  alone except for a missing title or URL being filled in. The feed's title and site URL are
  updated from the feed. A permanent redirect updates the stored URL.
- Emits `CoreEvent::FeedsChanged` (`feeds://changed`) when anything changed.
- **Cleanup**, once a day and after each full check: delete entries with no saved item that
  are read and older than `keep_read_days`, or unread and older than `keep_unread_days`
  (age from `seen_at`); delete `feed` shelf items no entry points to, with their snapshot
  folders. Saved items are never touched.

## Core API

| Command | Result |
|---|---|
| `feeds()` | `Vec<FeedView { id, title, url, site_url, unread, stage, error, last_ok_at }>` |
| `feed_entries(feed: Option<i64>, filter: All \| Unread \| Saved, before: Option<i64>, limit)` | `Vec<EntryView { id, feed_id, feed_title, title, summary, url, published_at, read, words, has_full_text, book_id, in_library }>`, newest first, paged by `before` |
| `subscribe(url)` | `SubscribeResult { Subscribed(FeedView) \| Choose(Vec<FeedLink>) \| NotAFeed }` |
| `unsubscribe(id)` | removes the feed and its unsaved entries |
| `open_feed_entry(id)` | the item id to open; marks the entry read |
| `save_feed_entry(id)` / `mark_entry_read(id, read)` / `mark_all_read(feed: Option<i64>)` | |
| `check_feeds()` | |
| `import_opml(path)` / `export_opml(path)` | counts of added and skipped feeds |

`subscribe(url)`: fetch the URL; if it is a feed (`looks_like_feed`), subscribe; if it is a
page, run `extract` and return its discovered feeds (`Choose`, or subscribe directly when
there is exactly one); otherwise `NotAFeed`. Subscribing an already-subscribed feed returns
the existing one. The first check of a new feed marks all but its newest 10 entries read, so a
new subscription does not flood the list.

`ArticleView` (phase 4) already carries the page's discovered feeds; add `subscribed: bool` to
each so the UI can offer or hide "Subscribe".

## Interface

- **Sidebar**: Feeds entry between Library and Activity, with the unread count; the count
  badge is hidden at zero.
- **Feeds view** (`src/views/FeedsView.vue`, store `src/stores/feeds.ts`) from
  `Feeds.dc.html`:
  - header: unread count, "checked N minutes ago", Check now, Mark all read, Subscribe (opens
    the Add dialog);
  - left: All new, Saved, then each subscription with its unread count; a failing feed shows
    "Not reachable since <day>" under its name; a context menu on a subscription offers
    Unsubscribe, Copy feed address, Open site;
  - right: entries grouped by day; each shows feed, age, read time when known, title, summary,
    an unread dot, and the save button (filled when in the library). Entries without full
    text in the feed say "summary only" and that the full article is fetched when opened.
  - Infinite paging by `before`. Empty states: no subscriptions (invite to subscribe, offer
    OPML import); no new entries.
- **Reader** for a `feed` shelf item: the text-only reader with a "Save to library" button in
  the header; it becomes "In your library" once saved.
- **Add dialog**: the link field's label and hint as in `Add.dc.html`. Submitting a feed
  address subscribes. Submitting an article link saves the article and, when the page has a
  feed that is not subscribed, shows the "This site has a feed" row with Subscribe.
- **Settings**: a Feeds section (check interval, retention, OPML import and export).
- Preview params: `view=feeds`, `feeds=empty|failing`.

## Slices

### 5.1 Parse feeds
Test (`crates/feeds/tests/`): the fixtures listed above.

### 5.2 OPML
Test: read nested outlines into a flat list; write and read back.

### 5.3 `fetch_feed`
Test (loopback): body with validators; `304`; redirect updates `final_url`; wrong content
type; size cap.

### 5.4 Library v10
Test (`library_v10.rs`): migration keeps rows on shelf `library`; `books()` excludes the feed
shelf; entry uniqueness; `ON DELETE SET NULL` for `book_id`.

### 5.5 Subscribe and check
Test (core, scripted fetcher): subscribe to a feed URL; subscribe through a page with one
feed, and with two (`Choose`); the first check keeps 10 unread; a second check with one new
entry adds one; `NotModified` adds none; the transition table for `FeedStage`; three failures
back off the interval; a success clears the failure.

### 5.6 Open, save, unsave
Test (core): opening a full-content entry creates a `feed` shelf item with `via: Feed` and no
page fetch; opening a summary-only entry fetches the page; `list_books` shows neither;
`save_feed_entry` makes it appear; removing it returns it to the feed shelf; an entry for an
article already in the library links to it. The transition table for `Shelf`.

### 5.7 Cleanup
Test (core, supplied clock): the two retention ages; saved items survive; orphaned feed-shelf
items and their folders are removed.

### 5.8 Feeds view
Test (`feeds.test.ts`): lists and counts from the mock; selecting a subscription filters;
opening an entry opens the reader and clears its dot; save toggles; a failing feed shows its
line; paging loads the next page; both empty states.

### 5.9 Reader and Add dialog
Test: "Save to library" on a feed-shelf item; the Add dialog subscribes for a feed address,
offers the feed for an article page, and shows the choice list for `Choose`.

### 5.10 Settings, OPML, docs
Test (`settings.test.ts`): the settings round-trip; import reports added and skipped.
Update `README.md`, `docs/roadmap.md`, `AGENTS.md` (where feed parsing lives).

## Done when

- All slices committed, all checks pass.
- Native app: subscribe to a few real feeds by site address and by feed address; entries
  arrive; a full-text entry opens instantly; a summary-only entry fetches its page; saving
  puts it in the library; after a restart the unread counts are right and a check sends
  conditional requests.
- A feed that stops answering is marked and does not stop the others from updating.

## Notes from implementation

- The schema is library **v11**, not v10: the ledger is one ahead of the plan since phase 2.
- 5.1: `aoide-feeds` wraps feed-rs 2.4 (the last line on Rust edition 2021; 3.0 needs 2024).
  feed-rs names an RSS `<author>` as a person whose name is the text and email empty, so
  `Entry.author` takes the name, else the email. Titles are plain text (markup removed,
  entities decoded). An entry without an id, link or date gets `sha256:` of its title.
- 5.3: `FeedResponse::Body` also says `moved` (the address answered with a permanent
  redirect), which the check stores as the feed's new address.
- 5.4: `feeds` also has `failing_since`, so `FeedStage::Failing { since_ms }` survives a
  restart; and `books()` keeps to the library shelf as planned.
- 5.5: `FeedStage` stays `Failing` (keeping `since_ms`) through `CheckStarted` and further
  failures; any answer returns it to `Idle`. A page's discovered feeds are `DiscoveredFeed`
  (renamed from phase 4's `FeedView`, which is now a subscription's view).
- 5.6: the core's shelf enum is `ItemShelf` (`core::shelf::Shelf` was taken). Paging is by
  `before = entry id`, in the list's order: published time, else when a check first found the
  entry; `EntryView.at` carries that time, so the view groups and orders by what the core
  sorted by. Subscriptions list by title.
- 5.7: cleanup runs daily in the worker and after Check now; an item removed for being no
  entry's goes through `remove_book`, so its folder and cover go too.
- 5.8: a subscription's menu also has Open site (`open_feed_site`, through the same
  `open_link` hook as Open original). Older entries load when the end of the list comes into
  view, with a "Show older entries" button for the same.
- 5.9: the core decides what a link given to Add is (`add_link`): a feed subscribes, anything
  else is saved as an article (one extra request to the address to tell). The dialog closes
  on saving, as in phase 4, so "This site has a feed" is offered under the article's byline
  in the reader instead of in the dialog. Opened from Feeds, the dialog subscribes
  (`subscribe`) and shows a site's feeds to choose from. The reader's Save to library is
  `save_to_library(book_id)`; `ArticleView.in_library` tells the reader which items came from
  a feed and are not kept.
- 5.10: `import_opml`/`export_opml` take paths from `choose_opml`/`choose_opml_target`
  (the system's file dialogs). Imported feeds are named by the list until the worker's first
  check, which treats them as new subscriptions (ten newest unread). The empty Feeds screen
  offers the import too.

