# 5. Articles and feeds

Read [plan.md](plan.md) first.

Articles are `core/articles.rs` (save, the fetch worker, stages) over `crates/fetch` (the only
network path, `Reach::Public`) and `crates/article` (extraction from a stored snapshot). Feeds
are `core/feeds.rs` over `crates/feeds` (parsing, OPML) and `crates/library/src/feeds.rs`.

---

## 5.1 Import from read-later apps

**Size:** M

### Outcome

Choose the export file from Pocket, Instapaper, Omnivore, Wallabag or Readwise Reader, or a
plain list of links, and the articles arrive in the library, fetched quietly in the background.

### What exists

- `save_article(url)` queues a fetch; the article has stages, and a page that gives only part
  of itself can be fetched again or from an archived copy.
- `import_opml` is the model for "choose a file, parse it purely, apply the result".
- **Check** how the fetch worker paces requests (one at a time? per host?) before sending it
  two thousand links.

### Design

**Pure parsing: `crates/feeds/src/links.rs`** (or a small crate `crates/imports` if it grows).

```
pub struct SavedLink { pub url: Url, pub title: Option<String>, pub saved_at: Option<i64>,
                       pub labels: Vec<String>, pub archived: bool,
                       pub content_html: Option<String> }
pub enum Source { Pocket, Instapaper, Omnivore, Wallabag, Readwise, Bookmarks, PlainList }
pub fn detect(name: &str, head: &[u8]) -> Option<Source>
pub fn read(source: Source, bytes: &[u8]) -> Result<Vec<SavedLink>, ImportError>
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

Only `http` and `https` addresses are kept; duplicates (after the canonical form
`crates/article` uses) are dropped; everything else is counted and reported.

**Where an export carries the page's text** (Omnivore, Wallabag), the article is built from
that HTML through `aoide_article::extract` with no request at all, marked as coming from the
import. Its original address is kept so "Fetch again" works.

**The job: `core/link_import.rs`.**

```
enum Stage { Idle, Reading, Choosing { found }, Fetching { done, of, failed }, Paused, Done }
```

- After reading, the user chooses: all, only unread (not archived in the old app), or the most
  recent N. Nothing is fetched before that.
- Fetching goes through the existing article queue at a polite pace: one request at a time per
  host, a short delay between hosts' requests, stepping aside for anything the user asks for
  directly. It survives a restart (the links are already items in a "waiting" stage).
- Old links are often dead. A failed link stays as an item with its reason and the existing
  **Try an archived copy** offer. A summary at the end lists counts by outcome.
- Labels become collections (created as needed). Items archived in the old app are archived
  here (4.3, if built; else they are imported as finished).
- `saved_at` becomes the item's added date, so the library's order matches the old app's.

**IPC.** `choose_links_file()`, `read_links(path) -> LinkImportView`, `start_link_import(choice)`,
`pause_link_import`, `cancel_link_import`, progress events.

**Interface.** In the Add dialog: **Import from another app…**. A progress row in the sidebar
card that already shows background work.

### Slices

1. `read` for each source on small synthetic files; `detect`.
2. Non-web addresses and duplicates dropped and counted.
3. Stages (table).
4. An export with content makes articles without a request. Seam: core test with the scripted
   fetcher (`Pages`), asserting zero fetches.
5. Pacing: per-host order, stepping aside for a user's save.
6. Restart during an import continues it.
7. Labels to collections, dates, archived state.
8. Interface and the end summary.

### Logging

`link_import.read` (`source`, `found`, `dropped`), `link_import.progress` at stage changes,
`link_import.done` (`fetched`, `partial`, `failed`, `took_ms`).

### Done when

- A 1,000-link Pocket export imports overnight without the app feeling slower.
- No site receives more than one request at a time.

### Open questions

- Fetch everything, or keep links as "not fetched yet" and fetch on open? The plan fetches what
  the user chose, since offline reading is the point.

---

## 5.2 Send a page from the browser

**Size:** S. Needs 6.1 (one running copy that receives addresses).

### Outcome

A bookmark in your browser's toolbar saves the page you are on to Aoide.

### Design

- 6.1 gives Aoide the `aoide:` address scheme and a single running copy. This feature is the
  `aoide://save?url=…` request and a bookmarklet:

  ```
  javascript:location.href='aoide://save?url='+encodeURIComponent(location.href)
  ```

  Settings › Sites shows it as a link to drag to the bookmarks bar, with two sentences on how.
- **Any web page can make this request**, so it is treated as untrusted input:
  - only `http` and `https` addresses, and only `Reach::Public` as for every fetch;
  - it saves an article and nothing else: it never subscribes to a feed, never opens the
    sign-in window, never changes a setting;
  - at most one save a second and twenty waiting; the rest are dropped and logged;
  - a toast names what was saved with **Undo**, so a save the user did not ask for is visible.
- The browser asks the user before opening `aoide:` links; that prompt is the browser's.

**A browser extension** could also send the page's HTML, which would save signed-in pages
without the optional browser. It needs a native-messaging host or a local listening port, and
a local port any page can post to is an attack surface. Not planned; note it as the next step
if the bookmarklet proves popular.

### Slices

1. `aoide://save?url=…` parsed and validated (in 6.1's pure `requests` function): bad schemes,
   missing address, an address inside the address.
2. A save request saves an article and shows the toast; a feed address is saved as an article
   page, not subscribed.
3. The rate limit.
4. Settings › Sites: the bookmarklet.

### Logging

`launch.save_link` (`outcome`, `why`). The address is not logged (the article's own events
carry the book id).

### Done when

- One click in Firefox or Chromium saves the page into a running Aoide, or starts Aoide and
  saves it.

---

## 5.3 Feed folders

**Size:** S

### Outcome

Group feeds into folders. A folder shows its unread count and can be read as one stream.
Folders in an OPML file are kept on import and written on export.

### What exists

- `opml::read` flattens nested outlines (`crates/feeds/src/opml.rs`); `Outline { title,
  xml_url, html_url }`.
- `FeedView` (`core/feeds.rs:104`), `feed_entries(feed_id, filter, before, limit)`.

### Design

- **Data (next library version):** `ALTER TABLE feeds ADD COLUMN folder TEXT;` One level;
  nested OPML folders are joined with " / ".
- **OPML:** `Outline` gains `folder: Option<String>` (the titles of its parent outlines);
  `write` nests feeds under folder outlines. Reading a file with no folders is unchanged.
- **Views:** `FeedView.folder`; `feeds()` keeps returning a flat list and the interface groups
  it (grouping a list for display is presentation, not a rule). `feed_entries` gains
  `folder: Option<String>` beside `feed_id`; `mark_all_read` likewise.
- **IPC:** `set_feed_folder(feed_id, folder: Option<String>)`, `rename_feed_folder(from, to)`.
- **Interface:** the feed list in `FeedsView.vue` grouped, folders collapsible (remembered
  locally), unread counts summed, "Move to folder" in a feed's menu with type-to-create, drag a
  feed onto a folder.

### Slices

1. OPML read with nested outlines; write and read back round-trips.
2. Column, `set_feed_folder`, rename.
3. `feed_entries` and `mark_all_read` by folder.
4. Interface; mock scenario.

### Logging

`feeds.folder` (`feed`, `outcome`).

### Done when

- Importing an OPML file from another reader keeps its folders.

---

## 5.4 Podcasts

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

- Articles need a URL (`articles.url TEXT NOT NULL`, `canonical_url NOT NULL`).
- `aoide_article::extract` works on stored HTML.

### Design

- **Parsing.** A MIME parser crate (for example `mail-parser`) behind a small pure function in
  `crates/article` (`from_email(bytes) -> EmailPage { html, subject, from, date,
  view_online_url }`): the `text/html` part, else `text/plain` wrapped in paragraphs.
- **As an article.** The page is the email's HTML; title from the subject, byline from the
  sender's name, date from the header. The address is the newsletter's "view online" link when
  the email has one (so **Fetch again** works), else a `mailto:`-free placeholder of the form
  `email:<message id>` that the reach rule never fetches. Check what else assumes an article
  URL is fetchable (the site rules, **Open the page**) and make those actions absent for an
  email (views carry the actions that apply).
- **Nothing is fetched** when an email is added: remote images are not loaded (they are
  trackers as often as pictures). Extraction already reports left-out images.
- **Clean-up rules** specific to email: unsubscribe footers, "view in browser" headers and
  tracking pixels are excluded and reported as excluded parts.
- **Import.** `.eml` joins the supported formats for drop, Add and the inbox folder (4.7).
- **The feed route** is documentation: services that turn a newsletter into an Atom feed work
  with Feeds as they are.
- Reading a mailbox (IMAP) is not planned: it means holding mail credentials.

### Slices

1. `from_email`: HTML part, plain part, encoded subject, a multipart with attachments (ignored
   and counted).
2. An `.eml` becomes an article with no request made.
3. Footer and header clean-up on three synthetic newsletters.
4. Actions that need a web address are absent.
5. README: both routes.

### Logging

`article.from_email` (`book`, `parts`, `outcome`).

### Done when

- A newsletter saved from a mail client reads and narrates without its footer.
