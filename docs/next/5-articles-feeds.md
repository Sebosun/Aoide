# 5. Articles and feeds

Read [plan.md](plan.md) first.

Checked against the code on 2026-11-16: the "Check" items are answered in place, and decided
questions are under **Settled**.

Articles are `core/articles.rs` (save, the fetch worker, stages) over `crates/fetch` (the only
network path, `Reach::Public`) and `crates/article` (extraction from a stored snapshot). Feeds
are `core/feeds.rs` over `crates/feeds` (parsing, OPML) and `crates/library/src/feeds.rs`.
An article's fetch stage is `core/fetching.rs` (`Stage`, `next`); where it is kept (library or
feed shelf) is `next_shelf` in `core/feeds.rs`.

New dependencies must build on the workspace's Rust 1.77.2 (`rust-version` in
`src-tauri/Cargo.toml`); pin them with a comment, as `crates/feeds/Cargo.toml` does for
`feed-rs` and `crates/article/Cargo.toml` for `dom_smoothie`.

---

## 5.1 Import from read-later apps

**Size:** M

### Outcome

Choose the export file from Pocket, Instapaper, Omnivore, Wallabag or Readwise Reader, or a
plain list of links, and the articles arrive in the library, fetched quietly in the background.

### What exists

- `Articles::save` (`core/articles.rs:281`) makes the item at once (`create`: a `books` row of
  kind `Article` on the given shelf, and an `articles` row in stage `queued`) and pushes it on
  the worker's queue. `Core::save_article` adds the move to the library shelf.
  `create_from_feed` (`core/articles.rs:363`) makes an article from HTML already in hand with
  no request: it stores the page as the snapshot, extracts it and moves the stage through
  `Started` and `Got`. It fetches no cover either (only `settle` calls `keep_cover`).
- A saved link's id is `aoide_article::canonical_id` (`crates/article/src/address.rs`): the
  hash of `normalise(url)`, which drops the fragment and tracking parameters. The same page
  saved twice is the same item.
- `import_opml` (`core/feeds.rs:578`, and `Core::import_opml`, at most 5 MiB read) is the model
  for "choose a file, parse it purely, apply the result"; `choose_opml` in
  `commands/feeds.rs` is the model for the file chooser.
- **Pacing (was a Check).** The worker (`Articles::run`, `core/articles.rs:530`) takes one
  article at a time from a single FIFO `VecDeque` and fetches it; there is no delay and no
  notion of host. Every save, refetch and feed entry opened (`entry_item` calls
  `Articles::save`) is pushed on the **back** of the same queue, so two thousand imported links
  would sit in front of anything the user asks for. One fetch may make several requests: the
  routes `plan_fetch` (`core/browsing.rs:77`) picks (HTTP, then the browser when it is on
  `Fallback` and the page fell short, then an archive when a site rule asks), plus the lead
  image as a cover. Each request is bounded by `FetchOptions` (20 s, 8 MiB). Feed checks run
  in their own worker (`core/feeds.rs`), beside the article worker.
- **Restart.** `Articles::load` (`core/articles.rs:252`) queues every article stored as
  `queued` (or `fetching`, through `Event::Interrupted`) again, so links saved as items do
  survive a restart. It queues them in `book_id` order (hash order, `Library::articles`) and
  all on the one queue.
- **Failed links.** A fetch that gets nothing ends in `Stage::Failed` with its error, and no
  snapshot. The item page then offers only **Fetch again** (`ItemView.vue`, `refetch` with
  `"http"`). **Try an archived copy** is offered only in `NeedsYouNotice.vue`, after a partly
  captured article; `refetch_article(id, ArticleSource::Archive)` itself works from any stage
  (`queue_fetch`: `Failed` + `Retried` is `Queued`).
- A non-success status is an error (`FetchError::Status`, `crates/fetch/src/lib.rs`), so a
  dead link's 404 page is not stored as the article.
- Collections: `create_collection(name, now)` (names unique ignoring case) and
  `set_book_collections(book, ids)`, which **replaces** a book's collections
  (`crates/library/src/collections.rs`). Finished: `set_finished_at`
  (`crates/library/src/activity.rs`). Home's newest shelf sorts by `added_at`
  (`core/facets.rs`), and `upsert_book` sets `added_at` only on insert, so a later `keep`
  does not move it.
- Background progress: there is no general background-work card. `EngineCard.vue` in the
  sidebar is the speech engine's (narration queue, recording sync, engine failure).
  `CoreEvent::DiscoveryImport(ImportProgress { done, total, current })` is a model for a
  progress event.
- `zip` 2.4 is already used (`crates/epub-parser`, `src-tauri`); `scraper` by
  `crates/article`.

### Design

**Pure parsing: a new crate `crates/imports`** (not `crates/feeds`: it needs `zip`, `csv`,
`serde_json` and an HTML parser, none of them feed parsing). No network, no clock. Add a line
for it to `AGENTS.md` in slice 1.

```
pub struct SavedLink { pub url: Url, pub title: Option<String>, pub saved_at: Option<i64>,
                       pub labels: Vec<String>, pub archived: bool,
                       pub content_html: Option<String> }
pub enum Source { Pocket, Instapaper, Omnivore, Wallabag, Readwise, Bookmarks, PlainList }
pub fn detect(name: &str, head: &[u8]) -> Option<Source>
pub fn read(source: Source, bytes: &[u8]) -> Result<Read, ImportError>
pub struct Read { pub links: Vec<SavedLink>, pub not_web: u32 }
```

| Source | File |
|--------|------|
| Pocket | CSV (`title,url,time_added,tags,status`); the older `ril_export.html` |
| Instapaper | CSV (`URL,Title,Selection,Folder,Timestamp`) |
| Omnivore | The export zip: metadata JSON plus the article HTML it saved |
| Wallabag | JSON, with `content` |
| Readwise Reader | CSV |
| Browser bookmarks | Netscape bookmark HTML |
| Plain list | One address per line |

Check each column list against a real export before writing its parser; the synthetic test
files copy the real header with invented rows.

Only `http` and `https` addresses are kept (`not_web` counts the rest). **Duplicates are
dropped in the core, not the crate**, by `aoide_article::canonical_id`: that is the same rule
that makes a saved page one item, it also finds links already in the library, and it keeps
`crates/imports` from depending on `crates/article`.

**Where an export carries the page's text** (Omnivore, Wallabag), the article is made through
`create_from_feed`, generalised to take its `ArticleSource` (`create_from_html`), with a new
`ArticleSource::Import` (stored as `"import"`). No request is made, not even for a cover. Its
original address is kept, so **Fetch again** fetches the page (a `Ready` article goes back to
`Queued` on `Retried`, and `settle` keeps the fuller copy).

**The job: component `core/link_import.rs`.**

```
enum Stage { Idle, Reading, Choosing { found }, Fetching { done, of, failed }, Paused, Done }
```

One transition function over named events, with a table-driven test.

- After reading, the user chooses: all, only unread (not archived in the old app), or the most
  recent N. Nothing is made or fetched before that; the links read wait in memory while
  `Choosing` (a restart then simply forgets them).
- **Every chosen link is fetched at import** (decided 2026-11-16), or made from its export's
  text when it has one. There is no "fetch on open".
- **Data (next library version).** The import and its items, so a restart knows which queued
  articles belong to it:

  ```
  CREATE TABLE link_imports (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    paused INTEGER NOT NULL DEFAULT 0,
    finished_at INTEGER
  );
  CREATE TABLE link_import_items (
    import_id INTEGER NOT NULL REFERENCES link_imports(id) ON DELETE CASCADE,
    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    PRIMARY KEY (import_id, book_id)
  );
  ```

  `Fetching { done, of, failed }` is counted from the items' article stages, not kept apart,
  so it is right after a restart.
- **Two lanes in the article worker.** `Articles` gains a second, background queue for
  imported links. The worker always takes the user's lane first (saves, refetches, entries
  opened from feeds), so a link the user saves during an import is fetched next. From the
  background lane it takes the next link whose host was not the last one asked when another
  host is waiting, and waits a fixed pause (a constant, 2 s to start; zero in tests) between
  background fetches. Still one fetch at a time, so no site gets two of the import's requests
  at once. While the import is paused the background lane is not taken from.
- **Restart.** `Articles::load` puts a queued article that is an item of an unfinished import
  on the background lane, in the import's order (`link_import_items` rowid), and every other
  queued article on the user's lane, as today. A paused import stays paused.
- **Routes.** An imported link takes the same routes as a saved one (`plan_fetch`): with the
  browser on **First**, or a site rule asking for it, imported pages are rendered in the
  browser one at a time, and the browser's idle stop closes it afterwards.
- **Old links are often dead.** A failed link stays as an item with its reason. Failed items
  get **Try an archived copy** on their item page (beside **Fetch again**, which they have
  today). The summary at the end lists counts by outcome (ready, partly captured, failed,
  already in the library, not web addresses) and offers **Try archived copies of the N that
  failed**, which queues each on the background lane with `ArticleSource::Archive`.
- **Cancel** takes the import's links that were never fetched off the queue and removes their
  items (nothing was stored for them, so this is `remove_book`); fetched ones stay.
- **Labels become collections** (created as needed, reusing one whose name matches ignoring
  case). Since `set_book_collections` replaces, a library method adds one book to one
  collection (`INSERT OR IGNORE`), so a link already in a collection keeps it. Items archived
  in the old app are archived here (4.3, if built; else they are imported as finished with
  `set_finished_at`).
- **`saved_at` becomes the item's added date**: it is passed as `now` to `Articles::create`,
  whose `upsert_book` sets `added_at` only on insert. A link already in the library keeps its
  date, its shelf moves to the library as `save_article` does, and it is not fetched again
  when it is `Ready`.

**IPC.** `choose_links_file()`, `read_links(path) -> LinkImportView`,
`start_link_import(choice)`, `pause_link_import`, `resume_link_import`, `cancel_link_import`,
`retry_link_import_archived`, and `CoreEvent::LinkImport(LinkImportView)` at stage changes and
as items finish (no more than once a second).

**Interface.** In the Add dialog (`AddDialog.vue`): **Import from another app…**. Progress in
a small sidebar card of its own in `AppSidebar.vue`, modelled on `EngineCard.vue`, shown only
while an import is fetching, paused or has a summary not yet dismissed.

### Slices

1. `crates/imports`: `read` for each source on small synthetic files; `detect`; non-web
   addresses counted. Seam: `cargo test -p aoide-imports`.
2. Duplicates dropped and counted by `canonical_id` in the core, including links already in
   the library.
3. Stages (table).
4. An export with content makes articles without a request. Seam: core test with the scripted
   fetcher (`Pages` in `core/article_tests.rs`), asserting `pages.asked` is empty.
5. The two lanes: a user's save during an import is fetched next; the background lane does not
   take the same host twice in a row while another waits. Seam: the order in `pages.asked`.
6. Restart during an import continues it on the background lane; a paused one stays paused.
7. Labels to collections, dates, archived state.
8. Failed items: **Try an archived copy** on the item page, and the summary's retry of all.
9. Interface, the end summary, cancel; mock scenario.

### Logging

`link_import.read` (`source`, `found`, `dropped`), `link_import.stage` (`from`, `to`) at
each transition, `link_import.done` (`fetched`, `partial`, `failed`, `took_ms`). Each article
keeps its own `article.save`, `article.stage` and `article.fetch` lines.

### Done when

- A 1,000-link Pocket export imports overnight without the app feeling slower, and a link
  saved during it is fetched at once.
- The import never has more than one request in flight.

### Settled

- Every chosen link is fetched at import, not kept as "not fetched yet" to fetch on open
  (decided 2026-11-16).

---

## 5.2 Send a page from the browser

**Size:** S. Needs 6.1 (one running copy that receives addresses).
**Built:** 2026-11-17 (dd83404..f8d6ff3)

### Outcome

A bookmark in your browser's toolbar saves the page you are on to Aoide.

### What exists

- 6.1 is built: `src-tauri/src/launch.rs` `requests` already turns `aoide://save?url=…` into
  `Request::SaveLink` (only an `http`/`https` address with a host inside; tested there), and
  `core/launching.rs` `Launching::act` ignores and logs it. The single-instance plugin hands a
  second start's arguments over; no deep-link plugin.
- `parse_link` (`core/articles.rs`) already refuses anything but `http` and `https` with a
  host, so `javascript:`, `file:` and a nested `aoide:` address are refused by it.
- `Core::save_article` saves a link as an article and only that. `Core::add_link` (what Add
  uses) subscribes when the address is a feed, so a save from the browser must call
  `save_article`, not `add_link`.
- Toasts (`src/stores/ui.ts`) already take an optional action (`ToastAction`, the Undo of
  3.2's highlights), shown as a button by `ToastStack.vue`.
- `remove_book` (`core/shelf.rs`) returns an item a feed still lists to the feed shelf
  instead of deleting it.
- Settings › Sites is `SiteSettings.vue`.

### Design

- 6.1 gives Aoide the `aoide:` address scheme and a single running copy. This feature is the
  `aoide://save?url=…` request and a bookmarklet:

  ```
  javascript:void(location.href='aoide://save?url='+encodeURIComponent(location.href))
  ```

  (`void`, because a `javascript:` address whose expression gives a string replaces the page
  with that string in Firefox.) Settings › Sites shows it as a link to drag to the bookmarks
  bar, with **Copy** beside it for a browser that will not take the drag, and two sentences on
  how.
- **Any web page can make this request**, so it is treated as untrusted input:
  - only `http` and `https` addresses (`parse_link`), and only `Reach::Public` as for every
    fetch;
  - it saves an article and nothing else (`save_article`): it never subscribes to a feed,
    never opens the sign-in window, never changes a setting;
  - at most one save a second, decided in the core; requests held before the interface is
    ready (6.1) are capped at twenty; the rest are dropped and logged;
  - a toast names what was saved with **Undo**, so a save the user did not ask for is visible.
    Toasts gain an optional action. The core's event for the save carries whether the item
    was new to the library: **Undo** is offered only then, and calls `remove_book` (which
    puts an item a feed lists back on the feed shelf).
- The browser asks the user before opening `aoide:` links; that prompt is the browser's.

**A browser extension** could also send the page's HTML, which would save signed-in pages
without the optional browser. It needs a native-messaging host or a local listening port, and
a local port any page can post to is an attack surface. Not planned; note it as the next step
if the bookmarklet proves popular.

### Slices

1. `aoide://save?url=…` parsed and validated (in 6.1's pure `requests` function): bad schemes,
   missing address, an address inside the address. Built with 6.1 (`launch.rs`
   `bad_save_addresses_are_ignored`).
2. A save request saves an article and the event says whether it was new; a feed address is
   saved as an article page, not subscribed. The event is `CoreEvent::LinkSaved`
   (`article://saved`, a `SavedLink`: `id`, `title`, `newToLibrary`), held with the window's
   other requests until it listens (`take_open_requests`).
3. The rate limit.
4. The toast's action and **Undo**; Settings › Sites: the bookmarklet. The toast is
   `showSavedLink` (`src/opening.ts`), listened for before `take_open_requests`; the preview
   shows it with `saved=new|again`.

### Logging

`launch.save_link` (`outcome`, `why`). The address is not logged (the article's own events
carry the book id).

### Done when

- One click in Firefox or Chromium saves the page into a running Aoide, or starts Aoide and
  saves it.

---

## 5.3 Feed folders

**Size:** S
**Built:** 2026-11-17 (be0285e..63a25ff)

### Outcome

Group feeds into folders. A folder shows its unread count and can be read as one stream.
Folders in an OPML file are kept on import and written on export.

### What exists

- `opml::read` flattens nested outlines (`crates/feeds/src/opml.rs`): it takes every
  `outline` start or empty tag with an `xmlUrl` and ignores the rest, so it never sees an
  outline end. `Outline { title, xml_url, html_url }`; `write` writes a flat list.
- `FeedView` (`core/feeds.rs:104`), `Feeds::views` (one list, by title). `feed_entries(feed_id,
  filter, before, limit)` (`commands/feeds.rs`) runs `Library::list_entries`
  (`crates/library/src/feeds.rs:238`), which already joins `feeds f`. `mark_all_read(feed_id)`
  runs `Library::mark_all_read` (`crates/library/src/feeds.rs:289`).
- `Feeds::import` (`core/feeds.rs:578`) skips feeds already subscribed and names new ones from
  the list until their first check; `export` writes every feed.
- `update_feed` stores every column but `added_at`. A check's `store` reads the row again under
  the library lock before writing it, so a column set meanwhile is not overwritten.
- The mock filters entries itself (`feedEntries` in `src/api/mock.ts`).

### Design

- **Data (next library version):** `ALTER TABLE feeds ADD COLUMN folder TEXT;` One level;
  nested OPML folders are joined with " / ". A folder exists while a feed is in it; there are
  no empty folders. `FeedRow` gains `folder`; `update_feed` leaves it alone, and
  `set_feed_folder(id, folder)` and `rename_feed_folder(from, to)` are their own library
  methods.
- **OPML:** `Outline` gains `folder: Option<String>`. `read` keeps a stack of the titles of the
  open outlines that have no `xmlUrl` (pushed on their start tag, popped on `</outline>`), and
  a feed's folder is that stack joined with " / ". `write` puts feeds with a folder under one
  outline per folder, named with the joined name (so a nested folder is written as one level
  and reads back the same). Reading a file with no folders is unchanged. On import, a feed
  already subscribed keeps its folder.
- **Views:** `FeedView.folder`; `feeds()` keeps returning a flat list and the interface groups
  it (grouping a list for display is presentation, not a rule). `EntryQuery` and the
  `feed_entries` command gain `folder: Option<String>` beside `feed_id`
  (`AND (?5 IS NULL OR f.folder = ?5)` in `list_entries`); `mark_all_read` likewise.
- **IPC:** `set_feed_folder(feed_id, folder: Option<String>)`, `rename_feed_folder(from, to)`,
  both followed by `FeedsChanged`.
- **Interface:** the feed list in `FeedsView.vue` grouped, folders collapsible (remembered
  locally), unread counts summed, "Move to folder" in a feed's menu with type-to-create, drag a
  feed onto a folder.

### Slices

1. OPML read with nested outlines; write and read back round-trips, nested folders included.
   Seam: `cargo test -p aoide-feeds`.
2. Column, `set_feed_folder`, rename; import keeps the list's folders.
3. `feed_entries` and `mark_all_read` by folder.
4. Interface; a mock scenario in `scenarios_for_the_ui_mock` for a folder's entries and
   marking a folder read, since the mock filters entries itself.

### Logging

`feed.folder` (`feed`, `outcome`: `moved`, `removed`, `renamed`). Folder names are not logged.

### Done when

- Importing an OPML file from another reader keeps its folders.

---

## 5.4 Podcasts

**Not planned for now** (decided 2026-11-16).

**Size:** L. Two stages; the first is useful alone.

### Outcome

**Stage 1.** Subscribe to a podcast like any feed. Episodes show with their length; press play
and the episode downloads and plays in the player, with speed, sleep timer, bookmarks and your
place remembered.

**Stage 2.** An episode gets a transcript (the publisher's when the feed offers one, else made
on your computer) and plays with the same sentence and word highlighting as a book.

### What exists

- `aoide_feeds::Entry` has no enclosure. `feed_entries` has no audio columns.
- An audiobook with no text is an item (`ItemKind::Audiobook`) played through
  `core/timeline.rs`; its path is a file or a folder.
- `crates/fetch` fetches pages and feeds with size limits, redirects checked hop by hop.
- `crates/transcribe` (whisper.cpp) and `crates/sync` align a recording to an EPUB's text.
  There is no path from a transcript alone to a readable item.

### Stage 1 design

- **Parsing.** `Entry` gains `enclosure: Option<Enclosure { url, mime, bytes }>`,
  `duration_s`, and the episode image; the feed gains its image and `is_podcast` (most entries
  carry audio enclosures).
- **Data (next library version).** `feed_entries` gains `enclosure_url`, `enclosure_mime`,
  `enclosure_bytes`, `duration_s`.
- **Fetching media.** `aoide_fetch::fetch_media(url, to: &Path, max_bytes, progress)`: streams
  to a file, same reach rule on every redirect (podcast hosts redirect through several
  measurement services; each hop is checked, and the count of hops is capped), resumes with
  `Range` when the server allows, refuses content types that are not audio.
- **Component `core/episodes.rs`:** an episode's stage is `Listed`, `Downloading { done, of }`,
  `Ready`, `Failed { why }`. Opening an episode makes an audiobook item on the feed shelf whose
  file is the download; saving it moves it to the library, as for articles. Downloads are kept
  with the feed's retention rule (`FeedSettings.keep_*`) unless saved, and are counted in the
  storage report.
- **Views.** `EntryView` gains `audio: Option<EpisodeView { duration_s, stage, bytes }>`.
- **Interface.** A play button and length on `FeedEntry.vue`; download progress; the episode
  notes (the entry's content) on its item page.
- **What leaves the computer:** the episode is downloaded from the publisher's host, which
  sees the request as any podcast app's. Add the line to the README.

Streaming while downloading is not in stage 1: the episode plays when the download is done, or
(if `timeline` can play a growing file; check) once enough has arrived.

### Stage 2 design (outline)

- **Publisher transcripts.** The `podcast:transcript` tag gives SRT, VTT or JSON with times.
  Parse them purely (`crates/feeds::transcript`) into timed cues.
- **Local transcripts.** Run `crates/transcribe` on the episode, in the background queue that
  recording sync uses.
- **From cues to an item with text.** Build a document (paragraphs from speaker turns or
  pauses, chapters from the feed's chapter marks or none) and a sync map directly from the
  cue times: no matching against a book is needed, since the text *is* the transcript. The
  item is then an audiobook with text, and the read-along view works as for a paired book.
  This is new ground in `core/recordings/`: write its own phase file first, starting from how
  `sync_maps` are stored.

### Slices (stage 1)

1. Enclosures, durations and images parsed from RSS (with `itunes:` tags) and Atom fixtures.
2. Columns and `EntryView.audio`.
3. `fetch_media`: reach checked per hop, size cap, resume, a non-audio response refused. Seam:
   loopback HTTP tests as `crates/fetch` has.
4. Episode stages (table) and the download job; cancel; restart continues.
5. An opened episode is an audiobook item on the feed shelf and plays; its place is kept.
6. Retention removes unsaved downloads; saved ones stay.
7. Interface.

### Logging

`episode.download` (`book`, `bytes`, `took_ms`, `hops`, `outcome`), `episode.removed`.

### Done when (stage 1)

- A podcast subscribed by its feed address plays its newest episode, and stopping and coming
  back resumes it.

### Open questions

- Is this in scope for a reading app? It reuses most of what exists, and it is also a whole
  product category with expectations (automatic downloads, per-show settings).
- Automatic download of new episodes: not planned for stage 1.

---

## 5.5 Newsletters

**Size:** S

### Outcome

Drop a saved email (`.eml`) on Aoide and it becomes an article. For newsletters that keep
coming, the README explains how to get them as a feed.

### What exists

- Articles need a URL (`articles.url TEXT NOT NULL`, `canonical_url TEXT NOT NULL`,
  `crates/library/src/store.rs`), and an article's id is `canonical_id` of it.
- `aoide_article::extract(html, url, options)` works on stored HTML. An article is extracted
  again from its stored snapshot **every time it is opened** (`Articles::document`), so any
  clean-up must happen inside `extract`, chosen by `ExtractOptions`, not once when the email
  is added.
- Extraction reports left-out images, figures, tables, embeds and forms as
  `Warning::LeftOut` (`crates/article/src/blocks.rs`). It never fills `ParsedBook::excluded`
  (always empty, `crates/article/src/lib.rs`); `ExcludedSection` holds a `Chapter` and an
  `ExclusionReason` (`crates/document`), and `ExcludedView` sends the reason as a string, so a
  new reason does not change the contract.
- `create_from_feed` (`core/articles.rs:363`) already makes an article from HTML in hand with
  no request and no cover fetch.
- What assumes an article's address is a web page:
  - `refetch_article` (queues a fetch of `canonical_url`);
  - `open_original` (refuses anything but `http(s)` through `web_address`, with an error);
  - the sign-in window (`SignInHost::article_page` in `core/signin.rs` opens `canonical_url`);
  - `site_of` for Settings › Sites (`core/sites.rs`), which skips an address with no host;
  - `siteOf` in `src/articles.ts`, which shows the host when the article has no site name.

  `ArticleView` carries no actions today; `ItemView.vue`, `ArticleHeader.vue` and
  `NeedsYouNotice.vue` show **Fetch again**, **Open original** and sign-in for every article.
- Adding files: `Shelf::import_files` (`core/shelf.rs`) copies EPUBs and PDFs into the library
  folder (`is_book_file`) and rescans; the drop handler in `src/App.vue` picks files by
  `/\.(epub|pdf)$/`, and `choose_books` (`commands/discovery.rs`) filters by
  `BOOK_EXTENSIONS`.

### Design

- **Parsing.** A MIME parser crate (for example `mail-parser`, at a version that builds on
  Rust 1.77.2) behind a small pure function in `crates/article` (`from_email(bytes) ->
  EmailPage { html, subject, from, date, message_id, view_online_url }`): the `text/html`
  part, else `text/plain` wrapped in paragraphs.
- **As an article.** The page is the email's HTML; title from the subject, byline and site name
  from the sender's name (so `siteOf` has a name to show), date from the header. The address is
  the newsletter's "view online" link when the email has one (so **Fetch again** works), else
  `email:<message id>` (angle brackets dropped; the SHA-256 of the file when there is no
  Message-ID), which the reach rule never fetches. It is made through `create_from_feed`
  generalised to take its source (`create_from_html`, shared with 5.1), with a new
  `ArticleSource::Email` (stored as `"email"`). The `.eml` is not copied into the library
  folder: the stored snapshot is the email's HTML, as for any article.
- **Actions follow the address.** `ArticleView` gains `on_the_web: bool`, false for an
  `email:` address. When it is false, `refetch_article`, `open_original` and the sign-in window
  refuse in the core, and the interface leaves out **Fetch again**, **Open original** and the
  needs-you notice.
- **Nothing is fetched** when an email is added: remote images are not loaded (they are
  trackers as often as pictures), and no cover is fetched (`create_from_html` does not). Images
  are already reported as left out.
- **Clean-up rules** specific to email run in `extract` when `ExtractOptions` says the page is
  an email (`Articles::document` sets it for `via = "email"`): unsubscribe footers, "view in
  browser" headers and tracking pixels are removed, and each removed part goes into
  `ParsedBook::excluded` with a new `ExclusionReason::Mailing`. A copy fetched later from the
  "view online" link replaces the email's only when fuller (`better`), and is then read as a
  web page.
- **Import.** `.eml` joins the supported formats for drop, Add and the inbox folder (4.7): the
  core's file import sends `.eml` files to `Articles`, not to `Shelf::import_files`; the drop
  handler in `App.vue` and the Add chooser accept `.eml`.
- **The feed route** is documentation: services that turn a newsletter into an Atom feed work
  with Feeds as they are.
- Reading a mailbox (IMAP) is not planned: it means holding mail credentials.

### Slices

1. `from_email`: HTML part, plain part, encoded subject, a multipart with attachments (ignored
   and counted). Seam: `cargo test -p aoide-article`.
2. An `.eml` becomes an article with no request made (`pages.asked` empty), on drop and Add.
3. Footer and header clean-up on three synthetic newsletters, reported as excluded, the same on
   every open.
4. `on_the_web`: the actions that need a web address are refused in the core and absent in the
   interface.
5. README: both routes.

### Logging

`article.from_email` (`book`, `attachments`, `outcome`: `ok`, `not_an_email`).

### Done when

- A newsletter saved from a mail client reads and narrates without its footer.
