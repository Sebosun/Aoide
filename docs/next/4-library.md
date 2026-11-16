# 4. Library: what you keep and how you find it

Read [plan.md](plan.md) first.

Checked against the code on 2026-11-16: the "Check" items are answered in place, and decided
questions are under **Settled**.

The library is `crates/library` (SQLite; schema in `src/store.rs`, at version 16). In the core,
`Shelf` (`core/shelf.rs`) imports, scans and removes, `core/item.rs` knows an item's kind and
keeps covers, `core/facets.rs` builds Home, `core/metadata.rs` looks details up on Open Library.
The library's filters, search and sort run in the interface (`src/stores/library.ts`), each
reading fields of `BookSummary` the core sends. A book's id is the hash of its content, so a
changed file is a new item: the old record goes missing and the new one is read from scratch,
without the old one's settings, place or edits.

Since this plan was written, PDFs joined the library (`crates/pdf`, commits 702969e and
ca27eff) and tags came (library v16, `book_tags`; commits 58be77b and fdaae68). 4.3 and 4.5 are
updated for both.

---

## 4.1 Edit a book's details

**Size:** M

### Outcome

Fix a wrong title, author or cover by hand on the item page. The file is not changed, and a
rescan does not undo the edit.

### What exists

- `books.title`, `authors`, `language`, `cover_path` are written from the file only when a
  record is described: a new id, or a record whose file could not be read (`Shelf::describe`,
  `core/shelf.rs`). A rescan copies an existing record's fields as they are and does not read the
  file again (`Shelf::rescan`, the `Some(record)` branch). Changing the file changes its hash,
  so a changed file is a new book, not a rewritten one.
- An article's `books.title` is rewritten on every fetch (`core/articles.rs`, the `upsert_book`
  in `receive`). An audiobook's comes from its tags, else its file or folder name, when it is added
  (`store_audiobook`, `core/audiobook.rs`).
- `apply_metadata` (`Metadata::apply`, `core/metadata.rs`) never writes a title or authors. It
  stores `BookDetails { description, publisher, isbn, published, subjects, open_library_key }`
  as JSON in `book_details`, adds the subjects as tags, and, only when the book has no cover,
  writes Open Library's cover to `covers/{id}-online.{ext}` and sets `books.cover_path`.
  `BookSummary` reads `books` only; `book_details` is served apart by the `book_details` command
  (`Metadata::details`: stored values over the file's description, publisher and ISBN). So for
  title and authors there are two layers (the file's record, the edit); for `published` the
  looked-up value is the only one below the edit; for the cover, `books.cover_path` holds the
  file's or Open Library's.
- Title and authors are read in two places: `BookSummary` from `books` (`Shelf::summary`), and
  `BookModel.title`/`authors` from the parse (`book.rs`, `BookModel::from_parsed`). The model
  feeds `BookView` (the reader's tools, `BookInfo.vue`), the now-playing panel
  (`core/controls.rs`), the M4B export (`core/export.rs`), the read-along EPUB
  (`core/read_along.rs`) and the Open Library query. Audiobooks read `books.title` in
  `Core::timeline_of` and in their `BookView` (`core/timeline.rs`). The two sources already
  disagree for a PDF with no title: the record falls back to the file name, the model to
  "Untitled".
- Sort and search are in `src/stores/library.ts` (`visible`, the `order` table) over
  `BookSummary.title` and `authors`. Home (`facets::home`) does not look at titles.
- One cover store (`core/item.rs`, `store_cover`, `{id}.{ext}`, removing the cover it had in
  another type). Thumbnails are named after the cover's file (`shown_cover`). Open Library's
  cover does not go through it.
- `Library::set_cover` sets or clears `books.cover_path` alone.
- The item page is `src/views/ItemView.vue` (title from the library store's summary).
  `BookInfo.vue` is the reader's tools sheet.
- `BookSettings.language` is the language sent to the speech engine. It is not the book's
  displayed language; leave it alone.

### Design

**Data.** A book value `BookKey::Details` holding `DetailsEdit { title, authors, cover, series,
series_index, published }`, each optional. Stored in `settings` (`Place::Settings`, as
`book.{id}.details`) so a book removed and added again keeps its corrections, as it keeps its
voice. `Stored::read` reads every book's with `book_values(BookKey::Details)`, as it reads
settings.

**Rule.** For each field: the edit, else what is stored for the item (`books` for title,
authors and cover; `book_details.published`; the series columns of 4.2). One pure function,
`item::details(record, looked_up, edit) -> Details`, called by:

- `Shelf::summary`, so the library list, its sort and search (in `src/stores/library.ts`) and
  Home use the edited values with no change in the interface;
- `Shelf::load`, which sets the loaded model's `title` and `authors` from it instead of from the
  parse, so `BookView`, now-playing, both exports and the lookup query follow (and a PDF's title
  is the same everywhere);
- `Core::timeline_of` and the audiobook `BookView` in `core/timeline.rs`;
- `Core::book_details`, for `published`.

`BookSummary` gains `edited: Vec<Field>` so the page can mark edited fields and offer **Use the
file's**.

**Cover.** A chosen image (JPEG, PNG, WebP, GIF, BMP: the `image` features in
`src-tauri/Cargo.toml`) is checked by reading its dimensions with the `image` crate and kept as
`covers/{id}-own.{ext}`: a stem of its own, so the file's `{id}.{ext}` and Open Library's
`{id}-online.{ext}` stay, and its thumbnail is made by `shown_cover` as for any cover (no
scaling on write). `DetailsEdit.cover` names it. **Use the file's** clears the field and
deletes the file; the summary then shows `books.cover_path` again, which was never overwritten.
Dropping an image on the cover does the same as choosing one.

**IPC.** `edit_details(book_id, patch: DetailsPatch) -> ()` (a patch, like
`AppSettingsPatch`: absent keeps, `null` clears; the cover as a path). It unloads the parsed
book (`Shelf::unload`) and emits `LibraryChanged`, as `apply_metadata` does. `choose_image() ->
Option<String>` opens the file chooser, beside `choose_books` in `commands/discovery.rs`.

**Interface.** **Edit details** on the item page (`ItemView.vue`): a dialog with title, authors
(one per line), series and number (4.2), cover. Works for books, audiobooks and articles: unlike
`apply_metadata`, it does not `require` `Needs::Book`.

### Slices

1. `item::details`: precedence table (edit over record for title, authors, cover; edit over
   looked-up for `published`; nothing edited gives the record's).
2. The value: stored, cleared per field, kept across remove and add.
3. A rescan keeps the edit; for an article, a refetch that rewrites `books.title` does too.
4. Every reader uses it: `BookSummary`, the loaded model (`BookView`, now-playing, M4B tags,
   read-along metadata), an audiobook's timeline. Seam: core test reading each after an edit.
5. Cover: set, replace, clear back to the file's (and to Open Library's when that was the one).
6. Interface; mock scenario.

### Logging

`details.edited` (`book`, `fields` as names). A row in `docs/logging.md`.

### Done when

- A book titled "untitled_final(2)" can be given its name and keeps it.

### Settled

- A changed file is a new item and does not carry the edit over, as it does not carry voice
  settings or the place (decided 2026-11-16: the book's id is its content hash).

---

## 4.2 Series

**Size:** M. Needs 4.1 for hand edits.

### Outcome

Books know their series and their number in it. The library can sort and group by series, the
item page says "Book 2 of *The Name*" with the ones before and after, and Home puts the next
book of a series you just finished first.

### What exists

- No series field anywhere: not in `aoide_document::ParsedBook` (`crates/document/src/lib.rs`,
  shared by EPUB, PDF and articles), not in `aoide_epub::BookMetadata` (the package-only read,
  `crates/epub-parser/src/model.rs`), not in `books`. EPUBs commonly carry it: EPUB 3
  `belongs-to-collection` refined by `collection-type` `series` and `group-position`, and
  Calibre's `<meta name="calibre:series">` and `calibre:series_index`. The package is read with
  `roxmltree` in `crates/epub-parser/src/package.rs`. A PDF's Info dictionary has no series.
- Books already listed are not read again: an existing record keeps its fields on every scan
  (see 4.1). `library_files` (v14) only saves hashing; clearing it would make the scan hash
  every file again but still not read their metadata. Neither v14 nor v15 cleared anything.
- The model for reading something once for books already listed is tags: `GlobalKey::SubjectsRead`
  (`crates/library/src/values.rs`), checked in `Shelf::rescan`, which reads each existing
  book's subjects with `subjects_of` (`aoide_epub::read_metadata`, the package only, or
  `aoide_pdf::read_pdf_metadata`) and then sets the flag.
- `facets::home` is a pure function over `BookSummary` (`core/facets.rs:62`). `BookSummary`
  carries `read_state` but not when an item was finished; `books.finished_at` has it.
- `LibrarySort` (`core/settings.rs:170`) has title, author, progress and recent (`#[serde(other)]`
  falls back to recent). The comparators are in `src/stores/library.ts` (`order`).
- An audiobook paired with a book is merged into it (`Library::merge_audiobook`,
  `crates/library/src/merge.rs`): the book is the item. An audiobook on its own has no series in
  its tags that Aoide reads.

### Design

**Parser.** `aoide_document` gains `Series { name, position: Option<f32> }`; `ParsedBook` and
`aoide_epub::BookMetadata` gain `series: Option<Series>`: `belongs-to-collection` of type
`series` first, else the Calibre pair. Positions can be fractional (2.5). PDFs and articles
give `None`.

**Data (next library version).** `ALTER TABLE books ADD COLUMN series TEXT; ALTER TABLE books
ADD COLUMN series_position REAL;` `NewBook` gains `series`; `upsert_book` writes it; the rescan's
branch for an existing record copies it. Books already listed get theirs once, the way subjects
did: a `GlobalKey::SeriesRead` flag, and `subjects_of` becomes one package read returning
subjects and series, so the first scan after the update reads each EPUB's package (not the
whole book) once. No clearing of `library_files`.

**Rule.** The series comes through `item::details` (4.1), so a hand edit wins. An audiobook on
its own gets one only from an edit.

**Views.** `BookSummary.series: Option<SeriesView { name, position }>` and
`BookSummary.finished_at: Option<i64>`. The item page asks `series(book_id) -> SeriesShelfView
{ name, items: Vec<{ book_id, position, state }> }`, built in the core from the library list.

**Home.** In `facets::home`: when the most recently finished item (by `finished_at`) has a
series, the unstarted library item with the next higher position in that series goes first in
`up_next_ids`. With 4.3, an archived item is not offered.

**Library.** `LibrarySort::Series` (series name, then position, then title; books without a
series after, by title), with its comparator in `src/stores/library.ts`. Cards in that sort show
the position as a small badge (`BookCard.vue`).

**Series name matching** is exact after trimming and case folding. No fuzzy matching: a wrong
merge is worse than two shelves, and 4.1 lets the user fix a name.

### Slices

1. Parser: both sources, fractional position, neither. Synthetic EPUB fixtures. Seam:
   `cargo test -p aoide-epub`.
2. Migration; new books get the columns; books already listed get them on the next scan, once
   (the flag). Seam: shelf test with a library made before the flag.
3. `details` carries the series; edit wins.
4. `facets::home`: next in series first; none when the next is already started or absent.
5. Sort by series.
6. `series` view and the item page strip.

### Logging

`library.scan` gains `with_series` (a count). The row in `docs/logging.md` is updated.

### Done when

- A Calibre-managed trilogy shows as 1, 2, 3 without any editing.

---

## 4.3 Archive, and quicker collections

**Size:** S

### Outcome

Finished with an article? Archive it: it leaves Home and the library's default view but is
still there under **Archive**. Putting something in a collection takes one keystroke and a few
letters.

### What exists

- Collections (`collections`, `book_collections`; `set_book_collections`), shown in the
  sidebar, toggled from checkbox rows in the item menus (`ItemView.vue`, `BookCard.vue`) or by
  dragging an item to the sidebar.
- Tags came after this plan was written: library v16 (`tags`, `book_tags` with a source of
  `user`, `metadata`, `classifier` or `removed`), set in `TagsDialog.vue` (which already
  offers the tags in use while typing), and the library's include/exclude tag filter
  (`AppSettings.library_tags`).
- The library's built-in filters are in `src/stores/library.ts` (`filters`: all, unread,
  progress, finished, listen, preparing), each reading one `BookSummary` field. There are no
  filter rules in `core/facets.rs`; it builds Home and `AudioSource`.
- Read state is an enum with a transition function (`next_read`, `core/reading.rs`);
  `Core::read_event` applies it and emits only `LibraryChanged` when it changes. Nothing tells
  the user an item was finished: there is no "finished" toast. Toasts take text only
  (`ui.toast`, `src/stores/ui.ts:68`), with no action button.
- `books.shelf` is `library` or `feed` with a `CHECK`, moved by `next_shelf` (`core/feeds.rs`).
  Changing that `CHECK` would mean rebuilding `books`, so the archive is a column of its own.
- `Storage::delete_book_audio` (`core/storage.rs`) deletes one item's generated audio.

### Collections and tags

Collections are the user's shelves, tags are labels from the item, the user or a model. A third
system is not added: the archive is a state of the item (like finished), and "quicker" is about
collections.

### Design

**Archive.** `ALTER TABLE books ADD COLUMN archived_at INTEGER;` (next library version).

- `BookSummary.archived: bool`. `facets::home` leaves archived items out of every list (so out
  of "up next"). In `src/stores/library.ts`, archived items are left out of every built-in filter
  and collection, and a new built-in filter **Archive** shows them. A search finds them too,
  marked on the card.
- `archive(book_id, on: bool)`.
- Finishing an article: in `Core::read_event`, when an article goes to `Finished`, either the
  setting "Archive articles when I finish them" (`AppSettings`, off by default) archives it, or
  the core emits `CoreEvent::Finished { book_id }` and the window shows a toast with an
  **Archive** action. Toasts gain an optional action for this.
- 2.5 (decided 2026-11-16): a queued feed article that finishes playing is marked read and
  archived through the same `archive`, with `from` = `queue`. The column is on `books`, so an
  item still on the feed shelf can carry it too.
- 5.1 sets it for items archived in the old app.
- Archiving never deletes anything: text, audio and notes stay. The storage tab can offer
  "Delete audio of archived items" (`delete_book_audio` for each).

**Collections, faster.**

- A "Add to collection" picker with type-to-filter and "Create *typed name*" as the last row,
  opened with `C` on the item page and from every item menu, in place of the checkbox rows.
- The Add dialog and the article header show collections as chips.
- Collections are remembered per feed: "Save entries from this feed into …" (optional column
  `feeds.collection_id`, next library version), applied when an entry moves to the library
  shelf (`Core::move_shelf` with the save event).

### Slices

1. Column, `archive`, `BookSummary.archived`, Home leaving them out (table-driven in
   `facets`); the store's filters and the Archive filter (`src/tests/library.test.ts`).
2. Search includes archived items, marked.
3. The finished event, the toast's action and the setting.
4. The picker.
5. A feed's default collection.

### Logging

`item.archived` (`book`, `on`, `from` = `user`, `finished` or `queue`). A row in
`docs/logging.md`.

### Done when

- A reader with 300 saved articles sees only the unread ones without deleting anything.

### Settled

- The quick picker is for collections; tags keep `TagsDialog.vue` (decided 2026-11-16; tags came
  after the plan, and their dialog already filters as you type).

---

## 4.4 Goals

**Size:** S

### Outcome

Set a daily goal in minutes and, if you like, a number of books for the year. Home and Activity
show how today and the year are going.

### What exists

- `ActivityStats` (`crates/library/src/activity.rs`): reading and listening per local day
  (`YYYY-MM-DD`) and per book. Streaks (`current`, `best`) are computed in the interface
  (`streaks` in `src/stats.ts`, tested in `src/tests/stats.test.ts`) and shown only in
  `ActivityView.vue`, over reading and listening added up. `WeekCard.vue` (on Home) shows the
  last seven days and no streak.
- The rest of `src/stats.ts` (`lastDays`, `lastMonths`, `summary`, `weekdays`, `heatmap`,
  `series`) arranges the core's per-day totals for charts.
- `books.finished_at` (v8), set and cleared by the read-state transitions (`Reading::apply`,
  `core/reading.rs`). Marking an item unread clears it, so a book read twice counts in the year
  it was last finished.
- Local days in the core come from `chrono` local time, as `ReadingClock` does.
- Settings › Library is `LibrarySettings.vue`; `AppSettings` and `AppSettingsPatch` in
  `core/settings.rs`.

### Design

- `AppSettings.goals { daily_minutes: Option<u32>, yearly_items: Option<u32>, counts: Both |
  Reading | Listening }`.
- Move the streak rule into the core while here: `core/goals.rs`, pure,
  `progress(stats, finished, goals, today) -> GoalsView { today_ms, daily_target_ms, streak_days,
  best_streak_days, goal_streak_days, year_finished, year_articles, yearly_target, on_pace:
  Option<bool> }`. `streaks` leaves `src/stats.ts`; the chart helpers stay, since they arrange
  numbers rather than decide anything. (AGENTS.md: let the core decide.)
- A "streak" stays "days with any activity"; a "goal streak" is days that met the goal. Both
  use local days, as the tables do.
- What counts as a finished item for the year: books and audiobooks; articles are counted
  apart ("42 articles").
- No reminders, no notifications. Goals are shown, never pushed.

**Interface.** A ring around today's figure in `WeekCard.vue`, a Goals card in Activity, the
two numbers in Settings › Library.

### Slices

1. `progress`: a day at, under and over the goal; streaks across a gap; year boundaries; pace.
2. Settings and patch.
3. Streaks served by the core; `streaks` removed from `stats.ts`; its tests moved to Rust.
4. Interface.

### Done when

- The streak the interface shows is the core's, and one test owns the rule.

---

## 4.5 More formats

**Size:** M each. PDF support has landed (702969e, ca27eff); these follow the path it made.

### What exists

- One list of book file extensions: `aoide_library::BOOK_EXTENSIONS` (`["epub", "pdf"]`) and
  `is_book_file` (`crates/library/src/lib.rs`), used by import (`Shelf::import_files`, which
  refuses anything else with "not an .epub or .pdf file", `core/shelf.rs:333`), the file chooser
  (`choose_books`, `commands/discovery.rs`) and the library folder scan (`scan_folder`). Drops
  go through import.
- The parser is chosen by content, not by a stored format: `book::parse_document`
  (`src-tauri/src/book.rs`) runs `aoide_pdf::parse_pdf` when `aoide_pdf::is_pdf` sees the PDF
  header, else `aoide_epub::parse_epub`. `Shelf::describe` and `BookModel::parse` both call it.
  There is no `books.format` column, and a PDF's `ItemKind` is `Book`.
- Subjects for tags are read without a full parse by `subjects_of` (`core/shelf.rs`), which
  branches on `is_pdf` too.
- A file with no text: import refuses a PDF before copying it (`aoide_pdf::check_text`); the
  scan passes over a PDF that cannot be read (`is_pdf_file`, by extension) instead of listing
  it with an error. An EPUB that cannot be read is listed with its error.
- Find books (`crates/discovery`: `find_epubs`, `names.rs` `is_epub`) still looks for EPUBs
  only; PDF support left it so.
- Everything downstream works on `aoide_document::ParsedBook`, which articles already produce
  through `aoide_article::extract(html, url, options).document` (as `Articles::document` does).
- The read-along EPUB export (`core/read_along.rs`) is built from the parsed model and the
  synced recording, not from a source EPUB, so it is not limited to EPUBs. The M4B export also
  works from the model.
- Test PDFs are written by hand in `aoide_pdf::testing`; each format can do the same.
- Already dependencies: `roxmltree` and `zip` (in `aoide-epub`), `encoding_rs` (in
  `aoide-fetch`). Markdown needs a parser crate (for example `pulldown-cmark`), a new
  dependency.

### Shared groundwork (once, with the first format)

- `book::Format` and `format_of(bytes, extension)`, recognising a file by its content first and
  its extension second: `%PDF-`; a zip whose `mimetype` is `application/epub+zip`; a zip with
  `word/document.xml` (DOCX); an XML file whose root is `FictionBook`, or a zip holding one
  `.fb2`; then `.txt`, `.md`, `.html` by extension. `parse_document` and `subjects_of` dispatch
  on it. No column: the check is cheap and is made on each parse, as for PDF.
- `BOOK_EXTENSIONS` grows with each format, so import, the chooser, the scan and the inbox
  folder (4.7) follow from one place. The chooser's filter is labelled from it.
- The scan's rule for a file that cannot be read becomes the format's: passed over when the
  format may hold no text at all (a scanned PDF), listed with its error otherwise. `is_pdf_file`
  goes.
- Each format's parser is a pure crate or module returning `ParsedBook`, with warnings for what
  it could not carry, tested with tiny synthetic fixtures.

### The formats, easiest first

| Format | Where | Notes |
|--------|-------|-------|
| Plain text (`.txt`) | `crates/plaintext` | Encoding: UTF-8, else detect with BOM and a fallback to Windows-1252 (`encoding_rs`), reporting a warning. Paragraphs: blank lines, or re-joined hard-wrapped lines (a line ending without punctuation followed by a lowercase start). Chapters: lines matching "Chapter N", "PART", roman numerals alone, or form feeds; else parts of about 5,000 words. Project Gutenberg's header and licence are detected and reported as excluded sections, not silently dropped. |
| Markdown (`.md`) | same crate | Headings are chapters (level 1, or level 2 when there is one level 1). Emphasis and links become text; code blocks are `Preformatted`; footnotes (`[^1]`) map to notes. Front matter gives title and author. |
| HTML file (`.html`) | `crates/article` | A saved page read with `extract` and a `file:` URL. It is a book in the library folder (`ItemKind::Book`), not an article: articles need a row in `articles` and a stored snapshot. |
| FB2 (`.fb2`, `.fb2.zip`) | `crates/fb2` | XML: `<section>` with `<title>` and `<p>`; `<body name="notes">` gives notes; the cover is a base64 `<binary>`; `<sequence name number>` gives the series (4.2). Common in Russian-language libraries. |
| DOCX (`.docx`) | `crates/docx` | A zip: `word/document.xml`, paragraph styles `Heading1..` for chapters, `footnotes.xml` for notes, `docProps/core.xml` for title and author. Tracked changes: read the accepted text. |
| MOBI, AZW3 | not planned | No dependable pure parser, and most such files carry DRM. Detect them and say "Convert this to EPUB with Calibre". |

### Slices (per format)

1. The parser on synthetic fixtures: structure, notes, warnings, a malformed file gives an
   error and no panic.
2. `format_of` recognises it (the first format also brings `Format` and the dispatch).
3. Import and the scan accept it; the item opens, reads and narrates. Seam: shelf test, as
   `a_pdf_is_imported_and_scanned_like_an_epub_and_read_as_reflowed_text`.

### Logging

No new event. `library.import`'s per-failure lines gain `format`.

### Done when

- A Gutenberg text file and a Markdown essay read and narrate like books.

### Settled

- Find books keeps looking for EPUBs only, as it did when PDFs came (plan's default: text,
  Markdown and HTML files on a disk are mostly not books).

---

## 4.6 Calibre libraries and OPDS catalogues

**Not planned for now** (decided 2026-11-16).

**Size:** Calibre M; OPDS L. Needs 4.2 for series.

### Calibre

**Outcome.** Find books notices a Calibre library and imports from it with Calibre's own
details (title, authors, series, cover), which are usually better than the file's.

**Design.**

- A Calibre library is a folder with `metadata.db` (SQLite) and one folder per book. A pure
  reader in `crates/discovery` (`calibre.rs`) opens the database **read-only and immutable**
  (never lock or write someone else's library) and lists `{ epub path, title, authors, series,
  series_index, cover path, calibre id }` from `books`, `authors`, `series`, `data` and their
  link tables.
- Find books shows "A Calibre library with 412 books" as a source. Importing copies the EPUBs
  as any import does and stores Calibre's details as the *looked-up* layer of `item::details`
  (4.1), below hand edits.
- A later scan picks up books added in Calibre since. Books removed in Calibre are not removed
  in Aoide.
- Formats other than those Aoide reads are counted and reported, not imported.

**Slices.** (1) Reader on a synthetic `metadata.db` built in the test. (2) Detection in the
scan. (3) Import with details. (4) Incremental scan.

### OPDS

**Outcome.** Add a catalogue address (a Calibre content server, Kavita, Standard Ebooks,
Project Gutenberg) and browse and download books from it.

**The conflict to settle first.** `crates/fetch` reaches public addresses only
(`Reach::Public`). Most personal OPDS servers are on the home network. The speech engine has
the same need and solves it with an explicit switch (*Allow addresses on my home network*,
`NetworkScope` in `crates/tts`). An OPDS catalogue needs the same: a per-catalogue, explicit
"this address is on my network" that the user turns on, with the reach checked on every hop.
Do not widen `Reach` globally.

**Design sketch.**

- OPDS 1.2 is Atom: parse navigation and acquisition feeds in `crates/feeds` (new module
  `opds`), pure. OPDS 2 (JSON) later.
- Credentials (HTTP Basic) are stored with the catalogue; say where and how they are kept, and
  never log them.
- Component `core/catalogues.rs`: list, browse (a page at a time), search (OpenSearch
  template), download an acquisition link of a supported format into the library through the
  normal import.

This is a plan outline only. Write its own phase file before building it.

### Logging

`calibre.found` (`books`, `took_ms`), `calibre.imported` (`count`), `opds.*` to be listed with
its own plan.

---

## 4.7 An inbox folder

**Size:** S

### Outcome

Choose a folder (Downloads, or one a sync tool fills). Any book that appears in it is added to
the library within seconds.

### What exists

- `core/watching.rs::watch_folder(dir, settle, on_change)` wakes on real changes only (not on
  opens or reads), once the folder has been quiet for `settle`. It watches recursively
  (`RecursiveMode::Recursive`), blocks its thread, and returns only when it cannot watch or
  `on_change` breaks. It does not notice the folder coming back after it went.
- The library folder itself is watched (`Shelf::watch`, settling 1.2 s) and scanned
  (`Shelf::rescan`).
- `Shelf::import_files(paths)` checks the extension (`is_book_file`), refuses a PDF with no text,
  copies a file from outside into the library folder (a numbered name when another file has the
  name; nothing written when the same content is there), then rescans. It identifies by content
  hash. It also **takes the id off the removed list** (`ignored.remove`): importing is how a
  removed book is brought back on purpose.
- Removed books are kept out of scans by `GlobalKey::IgnoredBooks` (`Shelf::ignored`,
  `Shelf::remove`).
- `discovered_files` is Find books' cache of files found on the disk, by path. It is not a
  record of what was imported and is not needed here.
- Book extensions come from `aoide_library::BOOK_EXTENSIONS`. A download in progress
  (`x.epub.part`, `x.epub.crdownload`) has another extension and is ignored by that alone.

### Design

- `AppSettings.inbox: Option<PathBuf>`; only the files directly in it; book extensions only. The
  library folder, or a folder inside it, is refused as an inbox.
- Component `core/inbox.rs`, with a host trait for: the ids in the library, the removed ids, and
  importing (`Shelf::import_files`). It runs `watch_folder` on its own thread and lists the
  folder's top level itself on each wake (`watch_folder` gains the recursive mode as an
  argument, or the component ignores what is below the top level).
- A file is taken when its size has not changed for 2 s, on top of the watcher's settling.
- A file is skipped when its hash is already in the library or **on the removed list**: the
  inbox checks this before calling `import_files`, which would otherwise bring a removed book
  back. Hashes are kept in memory per (path, size, modification time), so a file is read once a
  session.
- It is **copied** into the library and the original is left where it is. Aoide never deletes
  from the inbox.
- State, one enum: `Idle`, `Settling { since }`, `Importing`, `Unavailable`. One import at a
  time.
- A toast names what was added; nothing opens by itself.
- The folder disappearing (an unmounted drive) moves to `Unavailable`, says so in Settings, and
  checks every 30 s for its return; watching starts again when it is back.

### Slices

1. The settle rule: a growing file waits; a `.part` file is ignored. Seam: component test with
   a temporary folder and an injected clock.
2. A new EPUB is imported once; the same content under another name is not imported twice.
3. Removed from the library, still in the inbox: not re-imported.
4. The folder goes and comes back.
5. Settings.

### Logging

`inbox.imported` (`book`, `file` name), `inbox.skipped` (`file`, `why`), `inbox.unavailable`.
Rows in `docs/logging.md`.

### Done when

- Saving an EPUB from the browser to Downloads puts it in the library without touching Aoide.

---

## 4.8 Backup and moving to another computer

**Size:** M

### Outcome

**Back up** writes one file with your library: progress, bookmarks, highlights, collections,
settings, saved articles, and (if you choose) the books themselves. **Restore** on this or
another computer brings it back. An automatic backup can be kept in a folder you choose.

### What exists

- The README's "Where your files live" lists everything: `settings.toml`, `themes/` and
  `site-rules.toml` in `~/.config/aoide`; the library database, covers, articles, models and the
  browser profile in `~/.local/share/com.aoide.audiobooks`; generated audio in the cache; books
  in `~/Documents/Aoide/Books`.
- The library is in WAL mode with `synchronous = NORMAL` (`Library::initialize`, `store.rs`).
  The app holds one connection, shared as `Arc<Mutex<Library>>` (opened in `Core::build`,
  `core.rs`). `rusqlite` is bundled (0.32), so `VACUUM INTO` is available.
- `Library::initialize` refuses a database whose `user_version` is newer than its migrations
  ("unsupported schema version"), but opening runs pending migrations, which writes.
- `zip` 2.4 (deflate only) is a dependency of `src-tauri`, used by `core/browser_download.rs`.
- `Library::move_paths(from, to)` rewrites `books.path`, `books.cover_path` and the
  `discovered_files` paths under a moved folder. `rename.rs` uses it, and runs before
  `Core::start` (`lib.rs`), moving folders while nothing has the library open. It does not touch
  `recordings.files` (a JSON list of the recording's paths) or `library_files` (the scan's
  cache).
- A missing book comes back when its file is anywhere in the library folder: the scan finds it
  by hash and sets its path and `present`. Find books does **not** reconnect one: it lists a
  missing book as "In library" (`CandidateStatus::InLibrary`, `core/discovery.rs`) and importing
  it copies nothing.

### Design

**What goes in.**

| Part | In a backup |
|------|-------------|
| Library database | Always. A consistent copy made with `VACUUM INTO` a temporary file, never by copying the live file. |
| Covers, saved articles | Always. |
| Settings, themes, site rules | Always. |
| Books | Optional (they can be large). |
| Recordings (your audiobooks) | Never by default: they are large and are the user's own files elsewhere. Their paths are kept. |
| Generated audio, models, a downloaded browser | Never. They are made or fetched again. |
| The browser profile (sign-ins) | Never. It holds session cookies. Say so in the dialog. |

**The copy.** `VACUUM INTO` runs on a second, read-only connection to the library file. In WAL
mode it reads one snapshot while the app's connection keeps writing, so nothing is paused and the
shared connection's lock is not held for the copy.

**Format.** A zip with `manifest.json` (`format: 1`, Aoide version, library schema version,
date, what is included, checksums). The file can hold reading history: tell the user it is not
encrypted.

**Restore.**

- `inspect_backup` reads the copied database's `user_version` without migrating. A backup
  whose schema is newer than this Aoide is refused; an older one is migrated when it is opened,
  as any database is.
- After a confirmation that names what will be replaced, the backup is unpacked into a staging
  folder, an automatic backup of the current state is written, and Aoide restarts itself. At
  start, before the core opens the library (where `rename.rs` runs), the staged files take the
  place of the current ones; the current ones are moved aside first and put back if any step
  fails, then removed.
- Books included in the backup are written to the library folder. Books not included keep their
  stored paths and show as *missing* until their files are in the library folder, where the scan
  finds them by hash.
- Absolute paths under the old home directory are rewritten to this computer's with
  `move_paths`, extended to `recordings.files`. `library_files` is cleared (the next scan
  hashes every file once).

**Automatic backups.** `AppSettings.backup { folder, every_days, keep }`: a background job (a
stage enum like preparation's) writes a dated file and removes the oldest beyond `keep`. Pointed
at a synced folder, this is the simple answer to "use my library on two computers", one at a
time.

**Component `core/backup.rs`:** `Stage { Idle, Copying, Packing { done, of }, Done, Failed }`,
host trait for paths.

**IPC.** `back_up(path, options)`, `inspect_backup(path) -> BackupInfoView`, `restore(path)`,
progress events.

### Slices

1. `VACUUM INTO` from a second connection is consistent while the first writes. Seam: library
   test.
2. Pack and manifest; unpack into a temporary home and open the library there.
3. Inspect refuses a newer schema without writing; an older one migrates on open.
4. The staged restore at start: applied; a failure part way leaves the previous library in use.
5. Path rewriting (books, covers, recordings); a missing book comes back when its file is put in
   the library folder.
6. With books included.
7. Automatic backups: schedule, retention, a failing folder is reported once.
8. Interface in Settings › Storage; README section.

### Logging

`backup.written` (`bytes`, `books_included`, `took_ms`, `outcome`), `backup.restored`
(`from_version`, `took_ms`, `outcome`), `backup.pruned` (`removed`). Rows in
`docs/logging.md`.

### Done when

- A library restored on a clean machine shows the same Home, the same places and the same
  notes.
- A restore that fails half-way leaves the previous library in use.

### Settled

- Restore restarts Aoide and applies the backup at start, before the library is opened, rather
  than rebuilding the core in place (plan's default, "decide which is simpler": the core is
  built once in `Core::start`, and `rename.rs` already works this way).
- Backups are not encrypted; a passphrase can be added to the format later (plan's default).
- Live sync between two running computers is a different feature and is not planned (plan's
  default).
