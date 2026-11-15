# Phase 4: articles by link, over plain HTTP

Read [plan.md](plan.md) first. Mockups: `design/Add.dc.html` (link field),
`design/ReaderText.dc.html` (article header), `design/Library.dc.html` (article cards and the
"needs you" marker), `design/NeedsYou.dc.html`.

## Outcome

Paste a link, get the article in the library: fetched once with a plain HTTP request, its
readable text extracted deterministically, read in the same reader as a book, narratable by a
speech engine like any text. When the fetch did not get the article, the item says why and what
can be done. No browser is involved in this phase.

## Architecture: two stages and a snapshot

```
url ──fetch──▶ Snapshot (stored HTML + how it was got) ──extract──▶ Document ──▶ BookModel
      aoide-fetch (network)                              aoide-article (pure)
```

- **Fetch** produces a snapshot and never interprets it.
- **Extract** is a pure function of the snapshot's HTML and URL. Same input, same output.
- The snapshot is stored, so opening an article re-extracts from disk with no network, and a
  better extractor improves old articles.
- Phase 6 adds a second fetcher (a browser) that produces the same `Snapshot`. Nothing after
  the snapshot changes.

## New crates

### `crates/document` (`aoide-document`)

The parsed-text model both sources produce. Move the model types from
`crates/epub-parser/src/model.rs` (`ParsedBook`, `Chapter`, blocks, `BlockKind`, `Note`,
`Warning`, `Cover`, `ExcludedSection`, ...) here unchanged, and have `aoide-epub` re-export
them (`pub use aoide_document::*`) so no caller changes. `src-tauri/src/book.rs`
(`BookModel::from_parsed`) keeps working as is. This is a move, done as its own slice with no
behaviour change.

### `crates/article` (`aoide-article`): pure, no network, no Tauri

```
pub struct ExtractOptions { /* limits: max blocks, max text bytes */ }
pub fn extract(html: &str, url: &Url, options: &ExtractOptions) -> Result<Extracted, ExtractError>

pub struct Extracted {
    pub document: ParsedBook,          // one chapter; headings kept as heading blocks
    pub meta: ArticleMeta,
    pub completeness: Completeness,
    pub feeds: Vec<FeedLink>,          // <link rel="alternate"> feeds found on the page
}
pub struct ArticleMeta {
    pub title: Option<String>, pub byline: Option<String>, pub site_name: Option<String>,
    pub published: Option<String>,     // as written, plus a parsed timestamp when it parses
    pub language: Option<String>, pub canonical_url: Option<Url>,
    pub lead_image: Option<Url>, pub word_count: usize, pub declared_words: Option<usize>,
}
pub enum Completeness {
    Complete,
    CutShort { captured: usize, declared: usize },
    Wall(WallKind),                    // SignIn | Paywall | Check
    Empty,                             // nothing readable was found
}
pub fn canonical_id(url: &Url) -> String     // the article's BookId
pub fn normalise(url: &Url) -> Url
```

Rules, each with fixtures in `crates/article/tests/`:

- **Main content**: found by a readability-style extractor (evaluate the candidates in
  plan.md; wrap the chosen one behind this crate's API). Then converted to blocks by this
  crate: `h1`–`h6` → heading with level, `p` and `blockquote` → paragraph, `li` → list item,
  `pre` → preformatted. Inline markup is flattened to text; links keep their text.
- **Left out, with a warning each**: images, figures, tables, embeds, forms. "3 images were
  left out." Preserve uncertain text and warn, as the EPUB parser does.
- **Metadata precedence**: JSON-LD `Article`/`NewsArticle` → Open Graph and `article:*` meta →
  `<title>`, `<meta name="author">`, `<html lang>`, `<link rel="canonical">`. The title block
  is not repeated at the top of the body when the body starts with the same heading.
- **JSON-LD body**: when JSON-LD carries `articleBody` and it is longer than the extracted
  body, the body is built from it (paragraphs split on blank lines) and a warning says so.
- **Completeness**:
  - `declared_words` comes from JSON-LD `wordCount`, else from `articleBody` length;
  - `CutShort` when declared is at least 300 and captured is under 60 % of it;
  - `Wall(Paywall)` when JSON-LD says `isAccessibleForFree: false` and the body is short, or a
    known paywall container is present;
  - `Wall(SignIn)` when the page is a login form with almost no text;
  - `Wall(Check)` for challenge pages (a short page whose title or body matches the known
    "checking your browser" patterns);
  - `Empty` when fewer than 40 words were found.
  The patterns are data in one file of the crate, each with a fixture that triggers it and one
  that looks similar and must not.
- **URL normalisation**: lower-case scheme and host, drop the fragment, drop tracking
  parameters (`utm_*`, `fbclid`, `gclid`, ... from a list in the crate), keep other query
  parameters in their original order, drop a trailing slash only on an empty path.
  `canonical_id` is the SHA-256 hex of the normalised URL.
- **Safety**: input size is bounded by the caller; extraction is bounded by the options;
  no URL is fetched and no script is run. A test feeds deeply nested and huge inputs.

### `crates/fetch` (`aoide-fetch`): network, no parsing

```
pub struct FetchOptions {
    pub timeout: Duration,            // default 20 s
    pub max_bytes: usize,             // default 8 MiB
    pub max_redirects: usize,         // default 5
    pub user_agent: String,
    pub reach: Reach,                 // Public (default) | AnyAddress (tests)
}
pub enum Reach { Public, AnyAddress }
pub struct Snapshot {
    pub requested: Url, pub final_url: Url, pub status: u16,
    pub html: String, pub fetched_at_ms: i64, pub via: Via,   // Http | Archive (| Browser, phase 6)
}
pub enum FetchError { Refused(String), Timeout, TooLarge, NotHtml(String), Status(u16), Transport(String) }
pub struct HttpFetcher { .. }
impl HttpFetcher { pub async fn fetch(&self, url: &Url) -> Result<Snapshot, FetchError> }
```

Rules, tested over loopback HTTP with synthetic responses (the pattern in
`crates/metadata/tests/` and `crates/chatterbox-client/tests/support`), with
`Reach::AnyAddress`:

- `http` and `https` only. Redirects are followed by this crate, one hop at a time, each hop
  re-checked; more than `max_redirects` is `Refused`.
- **`Reach::Public`** resolves the host and refuses loopback, private, link-local and
  unspecified addresses, on the first request and on every redirect. Links arrive from feeds
  and pasted text, so a link must not be able to reach the local network. Tested with the
  option off: a loopback URL is refused before any connection.
- Body read up to `max_bytes`, then `TooLarge`. `Content-Type` must be HTML or XHTML, else
  `NotHtml`. Charset from the header, else `<meta charset>`, else UTF-8 with replacement.
- No automatic retries, no cookies kept between fetches, system proxy ignored
  (`reqwest` with `rustls-tls`, as elsewhere in the workspace).
- Non-2xx is `Status(code)`, with the body discarded.

## Data (library v9)

```
CREATE TABLE articles (
  book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  url TEXT NOT NULL,              -- as given
  canonical_url TEXT NOT NULL,    -- normalised; the id is its hash
  site_name TEXT, byline TEXT, published_at INTEGER,
  stage TEXT NOT NULL, reason TEXT, error TEXT,
  via TEXT, fetched_at INTEGER,
  word_count INTEGER, declared_words INTEGER
);
```

Files: `<data>/articles/<id>/snapshot.html` and, when there is a lead image,
`<data>/articles/<id>/cover.<ext>`. `books.path` points at the snapshot. Removing the item
removes the folder. `core/storage.rs` counts the folder in the storage report and its cleanup
knows about it.

## State: an article's fetch (`src-tauri/src/core/fetching.rs`, new)

```
enum Stage { Queued, Fetching, Ready, NeedsYou(Reason), Failed }
enum Reason { SignIn, Paywall, Check, CutShort, Empty }
enum Event { Started, Got(Completeness), FetchFailed, Retried, Removed }
fn next(stage: Stage, event: Event) -> Stage
```

- `Got(Complete)` → `Ready`. `Got(anything else)` → `NeedsYou(reason)`. A snapshot is stored
  in both cases, so whatever text was captured is readable.
- A later fetch replaces the stored snapshot only when it captured more words than the stored
  one, or the stored one was not `Ready`. A good copy is never replaced by a worse one.
- `FetchFailed` (network error, non-HTML, status) → `Failed` with the message; no snapshot.
- One fetch at a time, on a worker owned by `core/articles.rs`; queued fetches survive a
  restart (`Queued` and `Fetching` rows are re-queued at start).

## Core (`src-tauri/src/core/articles.rs`, new)

- `Core::save_article(url) -> Result<String, String>` (command `save_article`): validates and
  normalises the URL, inserts the item (kind `article`, title = the host until fetched) and
  its `articles` row in `Queued`, emits `library://changed`, starts the worker. Saving a URL
  that is already an item returns its id and re-queues it only if it is not `Ready`.
- `Core::refetch_article(id, source)` with `source: Http | Archive` (command
  `refetch_article`).
- `Core::article(id) -> ArticleView` (command `article`): url, canonical url, site name,
  byline, published, stage, reason, via, captured and declared words, discovered feeds.
- Event `CoreEvent::ArticleState(ArticleView)` (`article://state`).
- `BookSummary` gains `article: Option<ArticleSummary { site_name, stage }>`.
- `load_book` for kind `article`: read the snapshot, `aoide_article::extract`, then
  `BookModel::from_parsed`. Everything built on `BookModel` then works: reading place,
  bookmarks, search, speech narration, preparation, M4B export.
- Not offered for articles: pairing a recording, Open Library lookup. Guard them as phase 3
  guarded text-only methods.
- Cover: the lead image is downloaded with `aoide-fetch` rules (image content type, 2 MiB cap)
  after a successful fetch; failure is silent and leaves the generated cover.
- **Archived copy**: `refetch_article(id, Archive)` asks the Internet Archive availability
  endpoint for the closest snapshot of the URL and fetches that. Check the endpoint's current
  form and terms before building it; if it is unsuitable, drop this action from the notice and
  record that in "Notes from implementation".

## Interface

- **Add dialog**: the link half of `Add.dc.html`. Field, "Save article". On open, a link on
  the clipboard is filled in (needs the Tauri clipboard plugin and a capability; if adding it
  is not wanted, skip the prefill and say so in the notes). Pasting a link anywhere outside a
  text field, or dropping link text on the window, opens the dialog with it filled in.
- **Library**: article cards as in `Library.dc.html`: a text tile (site name, headline,
  first line) in place of a cover, then title, byline, status line. Kind switch gains
  Articles. Stages on the card: "Fetching…" with a skeleton tile; "Needs you · <reason>" in
  the warning colour; "Could not be fetched" with the message in the tooltip.
- **Home**: article cards and rows as in `Main.dc.html`.
- **Reader** (shape `Read` or `ReadAlong`): article header from `ReaderText.dc.html` (site,
  date, read time, "Open original" through the existing opener plugin, title, byline). The
  chapter menu becomes an Outline of the article's headings; it is hidden when there are none.
- **Needs you**: `src/components/article/NeedsYouNotice.vue` from `NeedsYou.dc.html`, shown
  in the reader after the captured text when the stage is `NeedsYou`. Heading and body per
  reason; actions in this phase: "Fetch again", "Try an archived copy", "Open original".
  "Kept so far: N words."
- **Item page**: Text card shows the source site, fetch state and words; Audio card offers
  only "Narrate with a voice". Actions: Read, Listen, Open original, Fetch again.
- Preview params: `article=ready|fetching|needsyou-signin|needsyou-cutshort|failed` gives the
  mock an article in that stage.

## Slices

### 4.1 Move the model to `aoide-document`
Done: `cargo test --workspace` passes with no test changed; `aoide-epub`'s public API is
unchanged.

### 4.2 Extraction: body and blocks
Test (`crates/article/tests/extract.rs`): a synthetic article page with navigation, sidebar
and footer yields exactly its headings, paragraphs, list items and code block, in order, as
one chapter; images and a table produce warnings and no blocks.
Done: output is byte-identical across two runs (a determinism test).

### 4.3 Extraction: metadata and normalisation
Test: each precedence level wins over the next; a duplicated title heading is dropped;
`normalise` and `canonical_id` cases from the rules above.

### 4.4 Extraction: completeness
Test: one fixture per `Completeness` variant and per `WallKind`, each with its near-miss.

### 4.5 `HttpFetcher`
Test (loopback server): success; each `FetchError`; redirect chain; charset from header and
from meta; `Reach::Public` refusal without a connection.

### 4.6 Library v9 and the fetch stage
Test (`library_v9.rs`; `core/fetching.rs` transition table).

### 4.7 Save, fetch, read
Test (core, with a fetcher trait object scripted in the test, as transcribers are scripted in
`recording_tests.rs`): `save_article` creates the item in `Queued`; the worker moves it to
`Ready`; `list_books` shows title, byline and site from the page; `open_book` and `chapter`
return its text; saving the same link again returns the same id; a restart re-queues an
unfinished fetch.
Done: `Core` takes its fetcher as a dependency through `Environment`, so tests never touch the
network.

### 4.8 Not ready: needs you and failed
Test (core): a cut-short page ends in `NeedsYou(CutShort)` with its text readable; a failed
fetch ends in `Failed` with no snapshot; a refetch that captures more replaces the snapshot; a
refetch that captures less keeps the old one.

### 4.9 Add dialog and library
Test (`add.test.ts`, `library.test.ts`): saving a link calls `save_article` and shows the
card in "Fetching…", then ready on the event; an invalid link shows the field error and makes
no call; the Articles kind filter; each stage's card.

### 4.10 Reader and the notice
Test (`reader.test.ts`): article header fields; outline lists headings and jumps; the notice
renders each reason and its buttons call `refetch_article`.

### 4.11 Narration and the guards
Test (core): an article plays through the fake engine, prepares, and exports; pairing a
recording and metadata lookup return the clear error.

### 4.12 Archived copy and cover
Test (core, scripted fetcher): `Archive` fetches the snapshot URL the availability answer
names and stores `via: Archive`; no answer leaves the stage unchanged with a message. A lead
image is stored as the cover; a failing image changes nothing.

### 4.13 Conventions and docs
Add to `AGENTS.md`: where article extraction and fetching live, that extraction is pure and
snapshot-based, the `Reach` rule, and "tests script the fetcher; real-network tests are opt-in"
(add one opt-in test behind an env var that fetches a URL given in another env var).
Update `README.md` and `docs/roadmap.md`.

## Done when

- All slices committed, all checks pass.
- Native app: paste links to a handful of ordinary server-rendered articles; each appears,
  reads cleanly, shows site and byline, resumes where it was left, and can be narrated.
- A link to a page that needs scripts or a login ends in "needs you" with whatever was
  captured, never in an item that looks complete but is not.
- With the network unplugged, saved articles open and a new link ends in "Could not be
  fetched".

## Notes from implementation

- The migration is library v10 (phase 3 used v9). The `articles` table also keeps `lead`: the
  page's description, else its first paragraph, cut at about 200 characters, for cards.
- 4.2–4.4: `extract()` is infallible and returns `Extracted { document, meta, completeness,
  feeds }`; an empty page is `Completeness::Empty`, not an error. dom_smoothie is pinned at
  0.18 (0.19 needs Rust 1.85; the MSRV is 1.77). html5ever and dom_smoothie are super-linear
  in nesting depth, so a page nested deeper than `ExtractOptions.max_depth` (256, measured by a
  linear lexical scan first) is read as plain text with its chrome skipped. A JSON-LD
  `headline` is used only when it shares most words with `og:title` or `<title>` (an
  encyclopedia puts a description there); else its `name`, else the page's title.
- 4.6: `fetching::Event` has `Started`, `Got`, `FetchFailed`, `Retried` and `Interrupted`
  (a fetch cut off by quitting is queued again on start); there is no `Removed` event, since
  removing an article deletes its row.
- 4.7–4.8: a refetch that fails keeps the stored copy and returns to that copy's stage, with
  "Could not fetch it again. …" in `error` (the reader's notice and the item page show it). A
  new copy replaces the stored one only when it is fuller: more words, or complete where the
  stored one is not.
- 4.9: the Add dialog is not prefilled from the clipboard (reading it needs a permission
  prompt); a link pasted outside a text field, or dropped, opens Add with it filled in. A link
  dropped on the native window may not arrive (WebKitGTK passes only files). The UI checks a
  link with `asLink` before asking the core, which decides.
- 4.10: `open_original` opens the canonical address through an `Environment.open_link` hook
  (tests record it). The needs-you notice stays through a fetch again, its actions disabled,
  and says when that fetch failed. In the reader's margin an article has no chapter menu.
- 4.12: the archived copy is fetched from `https://web.archive.org/web/{timestamp}id_/{url}`
  (the `id_` form is the page as archived, without the archive's toolbar).
- An article's word count is stored when its copy is kept, so lists show its reading time
  before it is first opened.
- 4.13: the opt-in real-network test is `crates/fetch/tests/live_page.rs`, `#[ignore]`d like
  the other local-only tests, with the address in `AOIDE_TEST_ARTICLE_URL`.
