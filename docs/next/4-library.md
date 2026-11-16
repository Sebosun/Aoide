# 4. Library: what you keep and how you find it

Read [plan.md](plan.md) first.

The library is `crates/library` (SQLite; schema in `src/store.rs`). In the core, `Shelf`
(`core/shelf.rs`) imports and removes, `core/item.rs` knows an item's kind, `core/facets.rs`
builds Home and the filters, `core/metadata.rs` looks details up on Open Library. A book's id
is the hash of its content.

---

## 4.1 Edit a book's details

**Size:** M

### Outcome

Fix a wrong title, author or cover by hand on the item page. The file is not changed, and a
rescan does not undo the edit.

### What exists

- `books.title`, `authors`, `language`, `cover_path` come from the file at scan time and are
  rewritten when the file changes (review P8: only changed EPUBs are read again).
- `book_details` holds what Open Library gave, as JSON, applied by `apply_metadata`. **Check**
  what `apply_metadata` writes and which of `books` and `book_details` wins in `BookSummary`:
  the edit must sit above both.
- One cover store (`core/item.rs`, review A5).
- `BookSettings.language` is the language sent to the speech engine. It is not the book's
  displayed language; leave it alone.

### Design

**Data.** A book value `BookKey::Details` holding `DetailsEdit { title, authors, cover, series,
series_index, published }`, each optional. Stored in `settings` (`Place::Settings`) so a book
removed and added again keeps its corrections, as it keeps its voice.

**Rule.** For each field: the edit, else Open Library's applied value, else the file's. One
function, `item::details(file, looked_up, edit) -> Details`, used everywhere a title or author
is shown, sorted by, searched, exported (M4B tags, read-along EPUB metadata) or sent to the
desktop's now-playing panel. `BookSummary` gains `edited: Vec<Field>` so the page can mark
edited fields and offer **Use the file's**.

**Cover.** A chosen image (JPEG, PNG, WebP) is decoded with the `image` crate, scaled, and
written through the cover store under the book's id with a marker that it is the user's, so a
rescan does not replace it. Dropping an image on the cover does the same.

**IPC.** `edit_details(book_id, patch: DetailsPatch) -> BookView` (a patch, like
`AppSettingsPatch`: absent keeps, `null` clears), `choose_cover(book_id)`.

**Interface.** **Edit details** on the item page (`BookInfo.vue`): a dialog with title, authors
(one per line), series and number (4.2), cover. Works for books, audiobooks and articles.

### Slices

1. `item::details`: precedence table.
2. The value: stored, cleared per field, kept across remove and add.
3. A rescan of a changed file keeps the edit.
4. Sort, search and Home use the edited values.
5. Cover: set, replace, clear back to the file's.
6. Exports and now-playing carry the edited values.
7. Interface; mock scenario.

### Logging

`details.edited` (`book`, `fields` as names).

### Done when

- A book titled "untitled_final(2)" can be given its name and keeps it.

---

## 4.2 Series

**Size:** M. Needs 4.1 for hand edits.

### Outcome

Books know their series and their number in it. The library can sort and group by series, the
item page says "Book 2 of *The Name*" with the ones before and after, and Home puts the next
book of a series you just finished first.

### What exists

- No series field anywhere in the core. EPUBs commonly carry it: EPUB 3
  `belongs-to-collection` with `group-position`, and Calibre's
  `<meta name="calibre:series">` and `calibre:series_index`.
- `facets::home` is a pure function over `BookSummary` (`core/facets.rs:62`).
- `LibrarySort` has recent, title, author, progress.

### Design

**Parser.** `aoide_epub` metadata gains `series: Option<Series { name, position: Option<f32>
}>`: `belongs-to-collection` of type `series` first, else the Calibre pair. Positions can be
fractional (2.5).

**Data (next library version).** `ALTER TABLE books ADD COLUMN series TEXT; ADD COLUMN
series_position REAL;` Books already scanned are not read again unless changed, so this
version also clears whatever the scan uses to skip unchanged files (see how library v14 and v15
did it in `store.rs`), making the next scan read every EPUB once.

**Rule.** The series comes through `item::details` (4.1), so a hand edit wins. Audiobooks get
it from an edit or from the book they are paired with.

**Views.** `BookSummary.series: Option<SeriesView { name, position }>`. The item page asks
`series(book_id) -> SeriesShelfView { name, items: Vec<{ book_id, position, state }> }`.

**Home.** In `facets::home`: when the most recently finished item has a series, the unstarted
library item with the next higher position in that series goes first in `up_next_ids`.

**Library.** `LibrarySort::Series` (series name, then position, then title; books without a
series after, by title). Cards in that sort show the position as a small badge.

**Series name matching** is exact after trimming and case folding. No fuzzy matching: a wrong
merge is worse than two shelves, and 4.1 lets the user fix a name.

### Slices

1. Parser: both sources, fractional position, neither. Synthetic EPUB fixtures.
2. Migration and one full rescan; the columns filled.
3. `details` carries the series; edit wins.
4. `facets::home`: next in series first; none when the next is already started or absent.
5. Sort by series.
6. `series` view and the item page strip.

### Logging

`library.series` on scan only as a count (`with_series`).

### Done when

- A Calibre-managed trilogy shows as 1, 2, 3 without any editing.

---

## 4.3 Archive, and quicker collections

**Size:** S

### Outcome

Finished with an article? Archive it: it leaves Home and the library's default view but is
still there under **Archive**. Putting something in a collection takes one keystroke and a few
letters.

### Why not tags

Collections (`collections`, `book_collections`; `set_book_collections`) already are many-to-many
labels. A second, parallel system would confuse more than it helps. What is missing is speed
and an out-of-the-way place for read things.

### Design

**Archive.** `ALTER TABLE books ADD COLUMN archived_at INTEGER;` (next library version).

- Archived items are left out of Home, of the default library filters and of "up next". A
  built-in filter **Archive** shows them. Search finds them, marked.
- `archive(book_id, on: bool)`. Finishing an article offers **Archive** in the finished toast;
  a setting "Archive articles when I finish them" (off by default) does it without asking.
- Archiving never deletes anything: text, audio and notes stay. The storage tab can offer
  "Delete audio of archived items".
- `BookSummary.archived: bool`; the rule for what each filter shows stays in `core/facets.rs`.

**Collections, faster.**

- A "Add to collection" picker with type-to-filter and "Create *typed name*" as the last row,
  opened with `C` on the item page and from every item menu.
- The Add dialog and the article header show collections as chips.
- Collections are remembered per feed: "Save entries from this feed into …" (optional column
  on `feeds`), applied when an entry is saved.

### Slices

1. Column, `archive`, the filter rules in `facets` (table-driven).
2. Search includes archived items, marked.
3. The finished toast and the setting.
4. The picker.
5. A feed's default collection.

### Logging

`item.archived` (`book`, `on`, `from` = `user` or `finished`).

### Done when

- A reader with 300 saved articles sees only the unread ones without deleting anything.

---

## 4.4 Goals

**Size:** S

### Outcome

Set a daily goal in minutes and, if you like, a number of books for the year. Home and Activity
show how today and the year are going.

### What exists

- `ActivityStats` (`crates/library/src/activity.rs`): reading and listening per day and per
  book. Streaks are computed in the interface (`src/stats.ts`, tested in
  `src/tests/stats.test.ts`).
- `books.finished_at`.

### Design

- `AppSettings.goals { daily_minutes: Option<u32>, yearly_items: Option<u32>, counts: Both |
  Reading | Listening }`.
- Move the streak rule into the core while here: `core/goals.rs`, pure,
  `progress(stats, finished, goals, today) -> GoalsView { today_ms, daily_target_ms, streak_days,
  goal_streak_days, year_finished, yearly_target, on_pace: Option<bool> }`. `src/stats.ts` then
  only formats. (AGENTS.md: let the core decide.)
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
3. Streaks served by the core; `stats.ts` trimmed; its tests moved to Rust where they tested
   the rule.
4. Interface.

### Done when

- The streak the interface shows is the core's, and one test owns the rule.

---

## 4.5 More formats

**Size:** M each. Build after PDF support lands: it decides how a book that is not an EPUB
enters the library, and these follow the same path.

### What exists

- Import refuses anything but `.epub` (`core/shelf.rs:288`); discovery looks for `.epub`
  (`crates/discovery/src/lib.rs:104`, `names.rs`).
- Everything downstream works on `aoide_document`'s parsed model, which articles already
  produce without being EPUBs.
- The read-along EPUB export needs an EPUB source.

### Shared groundwork (once, with or after PDF)

- `books.format` (or whatever PDF introduces) and one function, `item::format_of(path, head
  bytes)`, that recognises a file by its content first and its extension second.
- Import, discovery and the inbox folder (4.7) accept the list of supported formats from one
  place.
- Each format's parser is a pure crate or module returning the document model, with warnings
  for what it could not carry, tested with tiny synthetic fixtures.
- Views carry the actions that apply (review A3): no read-along EPUB export for a text file.

### The formats, easiest first

| Format | Where | Notes |
|--------|-------|-------|
| Plain text (`.txt`) | `crates/plaintext` | Encoding: UTF-8, else detect with BOM and a fallback to Windows-1252, reporting a warning. Paragraphs: blank lines, or re-joined hard-wrapped lines (a line ending without punctuation followed by a lowercase start). Chapters: lines matching "Chapter N", "PART", roman numerals alone, or form feeds; else parts of about 5,000 words. Project Gutenberg's header and licence are detected and reported as excluded sections, not silently dropped. |
| Markdown (`.md`) | same crate | Headings are chapters (level 1, or level 2 when there is one level 1). Emphasis and links become text; code blocks are `Preformatted`; footnotes (`[^1]`) map to notes. Front matter gives title and author. |
| HTML file (`.html`) | `crates/article` | A saved page is an article with no fetch: run `extract` on the file, source "file". |
| FB2 (`.fb2`, `.fb2.zip`) | `crates/fb2` | XML: `<section>` with `<title>` and `<p>`; `<body name="notes">` gives notes; the cover is a base64 `<binary>`. Common in Russian-language libraries. |
| DOCX (`.docx`) | `crates/docx` | A zip: `word/document.xml`, paragraph styles `Heading1..` for chapters, `footnotes.xml` for notes, `docProps/core.xml` for title and author. Tracked changes: read the accepted text. |
| MOBI, AZW3 | not planned | No dependable pure parser, and most such files carry DRM. Detect them and say "Convert this to EPUB with Calibre". |

### Slices (per format)

1. The parser on synthetic fixtures: structure, notes, warnings, a malformed file gives an
   error and no panic.
2. `format_of` recognises it.
3. Import and discovery accept it; the item opens, reads and narrates.
4. The item page offers only what applies.

### Logging

`import.parsed` gains `format`.

### Done when

- A Gutenberg text file and a Markdown essay read and narrate like books.

---

## 4.6 Calibre libraries and OPDS catalogues

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

- `core/watching.rs::watch_folder` wakes on real changes only (not on reads).
- The library folder itself is watched and scanned (`rescan_library`).
- Import copies a file into the library and identifies it by content hash; `discovered_files`
  remembers hashes seen by Find books.

### Design

- `AppSettings.inbox: Option<PathBuf>`; not recursive; supported formats only.
- A file is taken when its size has not changed for 2 s (a download in progress, a `.part` or
  `.crdownload` file, is left alone).
- It is **copied** into the library and the original is left where it is. Aoide never deletes
  from the inbox. Its hash is remembered so it is not offered again after the user removes the
  book from the library.
- State, one enum: `Idle`, `Settling { since }`, `Importing`. One import at a time.
- A toast names what was added; nothing opens by itself.
- The folder disappearing (an unmounted drive) pauses watching and says so in Settings; it
  resumes when the folder is back.

### Slices

1. The settle rule: a growing file waits; a `.part` file is ignored. Seam: component test with
   a temporary folder and an injected clock.
2. A new EPUB is imported once; the same content under another name is not imported twice.
3. Removed from the library, still in the inbox: not re-imported.
4. The folder goes and comes back.
5. Settings.

### Logging

`inbox.imported` (`book`, `file` name), `inbox.skipped` (`file`, `why`), `inbox.unavailable`.

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

The README's "Where your files live" lists everything: `settings.toml`, `themes/` and
`site-rules.toml` in `~/.config/aoide`; the library database, covers, articles, models and the
browser profile in `~/.local/share/com.aoide.audiobooks`; generated audio in the cache; books
in `~/Documents/Aoide/Books`. The library is in WAL mode (review P1). `zip` is already a
dependency.

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

**Format.** A zip with `manifest.json` (`format: 1`, Aoide version, library schema version,
date, what is included, checksums). The file can hold reading history: tell the user it is not
encrypted.

**Restore.**

- Refuses a backup whose schema is newer than this Aoide; migrates an older one on open, as
  any database is.
- Replaces the current library after a confirmation that names what will be replaced, and
  first writes an automatic backup of the current state. Then Aoide restarts its core (or asks
  to be restarted; decide which is simpler).
- Books included in the backup are written to the library folder. Books not included keep
  their stored paths and show as *missing* until found: Find books already reconnects files by
  content hash.
- Absolute paths under the old home directory are rewritten to this computer's.

**Automatic backups.** `AppSettings.backup { folder, every_days, keep }`: a background job (a
stage enum like preparation's) writes a dated file and removes the oldest beyond `keep`. Pointed
at a synced folder, this is the simple answer to "use my library on two computers", one at a
time.

**Component `core/backup.rs`:** `Stage { Idle, Copying, Packing { done, of }, Done, Failed }`,
host trait for paths and for pausing writers while `VACUUM INTO` runs.

**IPC.** `back_up(path, options)`, `inspect_backup(path) -> BackupInfoView`, `restore(path)`,
progress events.

### Slices

1. `VACUUM INTO` copy is consistent while another connection writes. Seam: library test.
2. Pack and manifest; unpack into a temporary home and open the library there.
3. Restore refuses a newer schema; migrates an older one.
4. Path rewriting; missing books reconnect by hash.
5. With books included.
6. Automatic backups: schedule, retention, a failing folder is reported once.
7. Interface in Settings › Storage; README section.

### Logging

`backup.written` (`bytes`, `books_included`, `took_ms`, `outcome`), `backup.restored`
(`from_version`, `took_ms`, `outcome`), `backup.pruned` (`removed`).

### Done when

- A library restored on a clean machine shows the same Home, the same places and the same
  notes.
- A restore that fails half-way leaves the previous library in use.

### Open questions

- Encrypt backups with a passphrase? Not planned; it can be added to the format later.
- Live sync between two running computers is a different feature and is not planned.
