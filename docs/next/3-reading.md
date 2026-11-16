# 3. Reading: the page

Read [plan.md](plan.md) first.

The reader is `src/views/ReaderView.vue` with `src/components/reader/`: `ChapterText.vue` draws
a chapter, `ChapterSegment.vue` draws one sentence group (it carries `data-segment-id` and plays
on click), `ReaderLines.vue` is the Lines view, `ReaderTools.vue` the reader's controls. A
chapter arrives as `ChapterView` (`src-tauri/src/book.rs:79`) with `SegmentView`s.

---

## 3.1 A menu on selected text

**Size:** M. Several other features hang their entry point here.

### Outcome

Select text in the reader and a small menu appears: **Copy**, **Highlight**, **Look up**,
**Search the book**, **Listen from here**, **Say this differently…**. Right-click a sentence
without selecting for **Listen from here**, **Say this again**, **Bookmark here**.

### What exists

- Nothing handles selection or `contextmenu` in `src/`.
- A click on a sentence plays it (`ChapterSegment.vue:55`). A drag that selects text ends with
  a click on some browsers: the click handler must not play when a selection was just made.
- `UiMenu.vue` is the menu component; `BookSearch.vue` searches the book; `play(book, chapter,
  segment)` exists.

### Design

**Pure TypeScript: `src/components/reader/selection.ts`.**

```
export interface TextRange { chapter: number; start: number; end: number; text: string;
                             firstSegmentId: string; }
export function rangeOfSelection(selection: Selection, chapterRoot: HTMLElement): TextRange | null
```

`start` and `end` are character offsets into the chapter's display text, the same coordinates
the segmenter uses (`Segment::range`). To compute them, each drawn sentence needs its start
offset in the DOM (`data-start` beside `data-segment-id`; `SegmentView` must carry `range.start`
if it does not already; check). Note markers and other inserted elements are not part of the
display text and must be skipped when counting (mark them `data-not-text`).

Offsets are counted in Unicode scalar values, as in Rust, not UTF-16 units: a test with an
emoji and with a combining accent guards it.

**The menu: `ReaderSelectionMenu.vue`.** Appears above the selection when the pointer is
released or after a keyboard selection settles (300 ms); closes on scroll, Esc, or a click
elsewhere. Actions are props, so features add theirs without editing the menu: each action has
an id, a label, and "applies to" (a selection, one word, a sentence).

**What applies** comes from the core where it is a rule (`can_say_again` from 1.1) and from the
selection's shape otherwise (Look up applies to one to three words).

**Lines view.** The same menu; `ReaderLines.vue` draws one passage, so the same data attributes
apply.

**Copy.** Plain text of the selection, without note markers.

### Slices

1. `rangeOfSelection`: within one sentence, across sentences, across paragraphs, with a note
   marker inside, with astral characters. Seam: vitest with a DOM built from a fixture chapter.
2. A selection does not start playback; a plain click still does.
3. The menu with Copy, Search the book and Listen from here.
4. The right-click menu on a sentence.
5. Keyboard: Shift+arrows select, the menu key opens the menu, actions reachable by keyboard.

### Logging

`ui.selection.action` (`action`, `book`). Never the text.

### Done when

- Selecting never plays; clicking still plays.
- The offsets of a selection match the segmenter's for every fixture chapter.

---

## 3.2 Highlights, and exporting notes

**Size:** M. Needs 3.1.

### Outcome

Highlight a passage, add a note to it, see all of a book's highlights and bookmarks in one
list, and export them as Markdown. While listening, one key highlights the sentence just
spoken.

### What exists

- `core/bookmarks.rs` and `crates/library/src/bookmarks.rs`: a bookmark is a place (`chapter`,
  `char_offset`, `segment_id`), an `excerpt` and a `note`. `BookBookmarks.vue` lists them.
- `ChapterSegment.vue` redraws one sentence when its props change (review P6). Highlights must
  keep that: a new highlight redraws the sentences it covers and no others.

### Design

**Data (next library version).**

```
CREATE TABLE highlights (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  chapter INTEGER NOT NULL CHECK(chapter >= 0),
  start INTEGER NOT NULL CHECK(start >= 0),
  end INTEGER NOT NULL CHECK(end > start),
  text TEXT NOT NULL,                 -- the highlighted text, at most 2,000 characters
  note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE INDEX highlights_book ON highlights(book_id, chapter, start);
```

Highlights are the user's writing: unlike audio, they are worth keeping when a book is removed
and added again. **Decide** (open question) whether to drop the cascade and keep them by book
id, as voice settings are kept.

**Component `core/highlights.rs`** (model: `core/bookmarks.rs`).

- `add(book, chapter, start, end)`: the core cuts `text` from the chapter itself (the interface
  sends a range, not text), joins a highlight that overlaps or touches an existing one, and
  refuses a range outside the chapter.
- `add_spoken(book)`: highlights the sentence at the playhead, or the one before it when the
  playhead is in its first second (you press the key after hearing it).
- **Anchoring.** When a chapter is opened, each highlight's stored `text` is compared with the
  chapter text at its range. If it differs (the article was fetched again, the book file was
  replaced by a corrected edition with a new hash; check whether that can happen for a kept
  id), the text is searched for in the chapter and the range moved; if it is not found the
  highlight is `detached`: listed, not drawn.
- `ChapterView` gains `highlights: Vec<HighlightView { id, start, end, has_note }>`; a separate
  `highlights(book_id)` lists all with text, note, chapter title and `detached`.

**IPC.** `highlights(book_id)`, `add_highlight(book_id, chapter, start, end)`,
`highlight_spoken(book_id)`, `update_highlight_note(id, note)`, `delete_highlight(id)`,
`export_notes(book_id, path)`, `notes_markdown(book_id) -> String` (for Copy).

**Export format.** One Markdown file per book:

```markdown
# Title
*Author* · exported 2026-11-16 from Aoide

## Chapter title

> The highlighted text.

A note, if there is one.

- 🔖 A bookmark's excerpt · its note
```

Front matter for other tools is a flag away (`---` block with title, author, url for
articles); keep the default plain. "Export all notes" writes one file per book into a chosen
folder.

**Interface.**

- Drawing: `ChapterText` hands each `ChapterSegment` the parts of highlights that fall inside
  it (`[start, end, id]` relative to the sentence). Colour from one new theme token (`mark`),
  added to `aoide_config::TOKENS` and the Default theme in light and dark.
- Clicking a highlight opens a small card: the note (editable), Remove, Copy.
- The Bookmarks tab of Book tools becomes **Notes**: highlights and bookmarks in reading
  order, a filter for each, **Export…** and **Copy as Markdown**.
- The spoken-sentence key is `H` (2.7), with a toast "Highlighted" and Undo.

### Slices

1. Table and library functions.
2. `add`: text cut by the core, overlap joined, out-of-range refused.
3. Anchoring: unchanged, moved, detached. Seam: component test with two versions of a chapter.
4. `add_spoken` at the playhead and just after a sentence ends.
5. `notes_markdown`: a golden file for a small synthetic book with highlights, notes and
   bookmarks across two chapters.
6. `ChapterView.highlights`, contract, fixtures.
7. Drawing without redrawing other sentences (the P6 test pattern: count renders).
8. The Notes tab, the card, export. Mock scenario "highlight, then it is listed".

### Logging

`highlight.added` (`book`, `chapter`, `chars`, `from` = `selection` or `spoken`),
`highlight.removed`, `highlight.detached`, `notes.exported` (`book`, `highlights`, `bookmarks`).

### Done when

- A highlight made in Page view shows in Lines view and after a restart.
- The exported file opens cleanly in a Markdown editor.

### Open questions

- Keep highlights when the book is removed?
- Colours (several highlight colours)? The plan says one.
- Should bookmarks and highlights merge into one thing? The plan keeps both: a bookmark is a
  place, a highlight is a passage.

---

## 3.3 Reader typography

**Size:** S

### Outcome

In the reader's tools: the font, the line spacing, the width of the text column, justified or
ragged text, and hyphenation. Today there is one font and five text sizes.

### What exists

- `aoide_config::Appearance` (`crates/config/src/lib.rs:206`): `text_size` 0–4, `reader_mode`,
  `theme`, `mode`, `zoom`. Saved with `save_appearance`; hand edits to `settings.toml` apply
  (review A4).
- The reading font is Newsreader, bundled, loaded ahead in `src/fonts.ts`. The CSP allows no
  outside fonts, so every font is bundled.

### Design

**`Appearance` gains** (each with a default equal to today's look):

| Field | Values |
|-------|--------|
| `reader_font` | `serif` (Newsreader), `sans` (the interface font), `legible` (Atkinson Hyperlegible, OFL), `system` |
| `line_spacing` | 0–4, 2 is today's |
| `text_width` | 0–4, 2 is today's |
| `justify` | bool, false |
| `hyphenate` | bool, false |

Steps, not raw numbers, as `text_size` does: the step-to-value tables live in the reader's CSS
as custom properties, so sizes stay within the token rule and a style test can cover them.

**Hyphenation** uses the browser (`hyphens: auto`), which needs the text's language: the
chapter root gets `lang` from the book's language. Check that WebKitGTK on Linux hyphenates;
if it has no dictionary for a language the switch does nothing, which is acceptable.

**Fonts.** Add the font files under `src/assets/`, their faces in `fonts.css`, and make
`readingFont()` load the chosen face. A font is loaded only when chosen.

**Lines view** follows the font and spacing; width and justification do not apply to it.

**Interface.** `ReaderTools.vue`: a "Text" popover with the size (already there), font
(`UiSelect`), spacing and width (`UiSegmented` or steps), two switches. Settings › Appearance
shows the same controls.

### Slices

1. `Appearance` fields with defaults; old `settings.toml` files load unchanged; values out of
   range are clamped and reported as other settings problems are. Seam: `cargo test -p
   aoide-config`.
2. Contract and fixtures.
3. CSS properties from the steps; a test that every step maps to a token-based value.
4. Fonts loaded on choice; the page is not laid out twice (the reason `fonts.ts` exists).
5. The popover; screenshots in light and dark at the extremes.

### Done when

- Every combination keeps the sentence shading and the word mark aligned.
- Scrolling stays as smooth as the `ui.frames` log showed before (compare it).

---

## 3.4 Dictionary

**Size:** L. Needs 3.1. The data is the hard part, not the code.

### Outcome

Select a word, choose **Look up**, and its meaning appears in a small card, from a dictionary
on your computer. Nothing is sent anywhere.

### Design

**Where definitions come from.** Aoide ships no dictionary. Two sources:

1. **Bring your own:** StarDict files (`.ifo`, `.idx`, `.dict` or `.dict.dz`). Many free
   dictionaries exist in this format, including bilingual ones (FreeDict), which gives word
   translation for free. The user puts them in `~/.local/share/com.aoide.audiobooks/dictionaries`
   or picks a folder.
2. **A download offered in Settings:** WordNet for English (small, free licence), fetched once
   through `crates/fetch`, checked against a pinned checksum, as the speech-recognition model
   and Chrome for Testing are (`core/chrome-for-testing.toml` is the model for the pin). It is
   converted or read in place by the same crate.

Wiktionary extracts would cover more languages but are large and need building and hosting a
file; leave that until someone wants it.

**New crate `crates/dictionary`**, pure, no network:

```
pub struct Dictionary { .. }                       // one opened dictionary
pub fn open(dir: &Path) -> Result<Dictionary, DictionaryError>
impl Dictionary {
    pub fn info(&self) -> Info { name, from_language, to_language, entries }
    pub fn lookup(&self, word: &str) -> Vec<Entry { headword, html_or_text, kind }>
}
pub fn candidates(word: &str, language: &str) -> Vec<String>   // "running" → running, run
```

- `.idx` is read into a sorted index once (memory-map or load; a large dictionary's index is
  tens of megabytes) and searched by binary search with the format's comparison rule.
- `.dict.dz` is gzip with random access; support plain `.dict` first.
- `candidates` does case folding and a few suffix rules per language; it does not need to be
  clever, since it only proposes headwords to try.
- Definitions can contain HTML: it is sanitised to a small set of tags in the core before it
  crosses to the interface. Never render dictionary HTML as it comes.

**Component `core/dictionaries.rs`:** finds dictionaries in the folder, opens them lazily,
keeps the order the user chose, and answers `look_up(word, book_language) ->
Vec<DefinitionView { dictionary, headword, html }>`, choosing dictionaries whose source
language matches the book's. The download has stages like the model download in
`core/recording.rs`.

**IPC.** `dictionaries() -> Vec<DictionaryView>`, `look_up(book_id, word)`,
`download_dictionary(id)`, `cancel_dictionary_download`, `set_dictionary_order`.

**Interface.** A card anchored to the selection: headword, definitions, a tab per dictionary
when several answer. Settings › Library (or a new Reading tab) lists dictionaries, the folder,
and the WordNet download with its size.

### Slices

1. `crates/dictionary`: open a tiny synthetic StarDict dictionary written for the test (three
   entries), look up exact and folded-case words, missing word, corrupt `.ifo`.
2. `candidates` for English.
3. Sanitising: scripts, event attributes, links and images are removed.
4. The component: several dictionaries, language choice, order.
5. `.dict.dz`.
6. The WordNet download: pin, checksum, stages, cancel. Seam: scripted fetcher, as in
   `core/browser_download.rs` tests.
7. The card and Settings. README "What leaves your computer" gets its line.

### Logging

`dictionary.opened` (`name` as file name, `entries`, `took_ms`), `dictionary.lookup`
(`outcome`, `took_ms`; never the word), `dictionary.download` stages.

### Done when

- A lookup answers in under 50 ms with a 100,000-entry dictionary.
- With no dictionary installed, Look up says how to get one instead of failing.

### Open questions

- Is WordNet's dry style good enough as the offered default, or is bring-your-own enough for a
  first version?
- Sentence translation needs a translation model or a network service. Not planned.

---

## 3.5 Search the whole library's text

**Size:** M

### Outcome

Type a few words in the library's search and get the passages that contain them across every
book and saved article, grouped by item. Click one to open the reader there.

### What exists

- `search_book` (`commands/library.rs:132`, `BookModel::search` in `book.rs:328`) searches the
  open book in memory. The library's search field filters by title and author.
- A book's text exists only after parsing (`BookModel::parse`); the scan stores metadata, not
  text. Articles keep their snapshot and are extracted on open.
- `rusqlite` is built `bundled`. **Check** that FTS5 is compiled in (it is in the bundled build
  by default; a one-line test that creates an FTS5 table settles it).

### Design

**Data (next library version).**

```
CREATE VIRTUAL TABLE passages USING fts5(
  text,
  book_id UNINDEXED, chapter UNINDEXED, block UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TABLE indexed (
  book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,          -- of the indexing rule; a bump re-indexes
  blocks INTEGER NOT NULL
);
```

One row per block (paragraph). A row's place is `chapter` and `block`, which the reader can
open (`block_offsets` gives the character offset). FTS tables do not cascade: removing a book
deletes its rows in the same transaction (in `Shelf`'s removal path).

Size: the index is roughly the size of the text again, a few megabytes for a long novel. Add it
to the storage report (`core/storage.rs`) and offer "Search inside books" as a switch in
Settings › Library for people who do not want it.

**Component `core/indexing.rs`,** a background job like the others:

```
enum Stage { Idle, Indexing { book: BookId, done: usize, of: usize }, Paused }
```

- Works through items with no `indexed` row or an old version, oldest opened last, one at a
  time, on a blocking thread; steps aside while speech is being made for listening (ask the
  host `busy()`).
- An item is also indexed when it is opened (its model is parsed anyway) and an article when
  its fetch completes.
- Parsing a book for the index must not hold the library lock (it is the leaf lock).

**Query.** `search_library(query, limit) -> Vec<LibraryHits { book_id, hits: Vec<PassageHit {
chapter, block, snippet, char_offset }>, more: u32 }>`: the user's words become an FTS query of
quoted terms joined by AND, with a prefix match on the last term; operators typed by the user
are treated as text. Ranking by `bm25`, three passages per item, items ordered by their best
passage. `snippet()` gives the excerpt with match markers, which the core converts to ranges so
the interface draws the marks (no HTML crosses).

**Interface.** The library search shows title matches as now and, below, **In the text**:
items with their passages. Enter on a passage opens the reader at that block with the words
marked (reuse `BookSearch.vue`'s marking). Shortcut Ctrl+Shift+F.

### Slices

1. FTS5 is available; the migration; rows removed with the book.
2. Index one synthetic book; a query finds a passage with the right chapter and block;
   diacritics and case fold.
3. Query building: quotes, operators as text, prefix on the last word, empty query.
4. The job: stages, steps aside, resumes after restart, re-indexes on a version bump.
5. Articles indexed on fetch and re-fetch.
6. Opening the reader at a passage.
7. Interface, storage report, the switch (turning it off drops the tables' rows).

### Logging

`index.item` (`book`, `blocks`, `took_ms`, `outcome`), `index.search` (`hits`, `took_ms`; never
the query).

### Done when

- A phrase remembered from any book is found in under 100 ms in a 500-book library.
- Indexing a large library never makes listening stutter.

### Open questions

- Index feed entries that were never saved? The plan says no: only the library shelf.
- Stemming (finding "running" for "run")? `porter` exists for English only; the plan says no.

---

## 3.6 Pictures, tables and code

**Size:** L, with a decision to make first.

### Outcome

A book's pictures show in the reader. The voice can say a picture's description. Tables are
shown as tables and announced instead of being read as a stream of cells. Code is announced or
skipped, as you choose.

### What exists

- A block is text: `BlockKind { Heading, Paragraph, ListItem, Preformatted }`
  (`crates/document/src/lib.rs:146`). There is no picture block and the reader draws no
  pictures.
- For articles the extractor counts what it left out (`LeftOut::Image`, `Figure`, `Table`,
  `Embed`, `Form`) and the reader says so.
- **Not checked:** what the EPUB parser does with `<img>`, `<figure>`, `<figcaption>` and
  `<table>`. The first slice finds out.

### The decision

A chapter's display text is its blocks' texts joined, and stored places are offsets into it.
Adding blocks for pictures and tables changes that text for books people already have places
in. Two ways:

- **A. Blocks that take no text.** A picture or table is attached to the block before it
  (`Block.after: Vec<Insert>`), like `break_before` in 1.4. No offset moves, no segment id
  changes. The reader draws inserts between paragraphs. Speech for an insert is an extra,
  separately addressed entry. More awkward in the segmenter, safe for stored places.
- **B. Real blocks**, with a one-time re-anchoring of every stored place by its excerpt.
  Cleaner model, riskier migration.

The plan assumes **A**.

### Design (with A)

**Document model.** `Insert::Picture { resource, alt, caption }`, `Insert::Table { rows:
Vec<Vec<String>>, header_rows, caption }`. `Preformatted` already exists as a block.

**Pictures.**

- EPUB: `resource` is a path inside the archive. The core extracts a picture when its chapter
  is opened, into `…/com.aoide.audiobooks/pictures/<book id>/`, scaled down if huge, and the
  asset protocol scope in `tauri.conf.json` grows by that folder. The source book is only read.
  Covers and tiny decorative images (under about 32 px, or with `role="presentation"`) are left
  out.
- Articles: only the lead image is fetched today. Fetching every picture means more requests to
  the site and its CDNs when saving. Make it a setting ("Keep an article's pictures"), off by
  default, through `crates/fetch` with the same reach rule and a size cap.

**Speech.** `AppSettings.describe { pictures: Off | Caption | AltAndCaption, tables: Announce |
Read | Skip, code: Read | Announce | Skip }`.

- A picture's caption is real text and is read as a paragraph by default. Alt text is read only
  when chosen ("Picture: …").
- A table is announced ("A table of 4 columns and 12 rows: *caption*") by default; `Read`
  speaks it row by row with the header before each cell.
- Code (`Preformatted`): `Announce` says "A code sample" and moves on.
- These spoken extras have their own entry ids (`"{chapter}:{block}:i{n}"`), so they do not
  disturb sentence ids.

**Reader.** Pictures between paragraphs, sized to the column, click to enlarge. Tables as
tables with horizontal scroll. In Lines view an insert shows as a single line ("Picture:
*caption*") that opens it.

### Slices

1. **Audit.** A synthetic EPUB with a picture, a figure with a caption, a table and a code
   block: a test that records what the parser produces today. Decide A or B with the result.
2. `Insert` in the document model; EPUB pictures and captions parsed; nothing else changes
   (display text identical: assert it on the existing fixtures).
3. Extraction to the pictures folder, removal with the book, the storage report.
4. The reader draws pictures.
5. Tables: parsed, drawn.
6. Speech choices for pictures, tables and code, with entry ids that leave the others alone.
7. Articles: the setting, the fetches, the snapshot.

### Logging

`pictures.extracted` (`book`, `count`, `bytes`, `took_ms`), `article.pictures` (`book`,
`fetched`, `skipped`, `bytes`).

### Done when

- An illustrated book looks right, and every place stored before the change still points at
  the same sentence.
- A table is never read as a run of numbers unless asked for.

### Open questions

- A or B.
- Formulas (MathML): show the picture fallback and say "a formula"? Not planned beyond that.

---

## 3.7 Fullscreen and a quiet reader

**Size:** S

### Outcome

F11 makes the reader fill the screen. When reading silently, the controls fade after a few
seconds without the pointer moving and return when it moves.

### What exists

- The reader already takes the whole window. `src-tauri/capabilities/default.json` grants
  `core:default` and webview zoom only.

### Design

- Fullscreen through a Rust command (`set_fullscreen(on: bool)`, `tauri::WebviewWindow::
  set_fullscreen`), which keeps the capability list small and is covered by the contract. The
  window reports its state back with an event so Esc and the window manager's own shortcut stay
  in step.
- Esc leaves fullscreen first, then the reader (the existing Esc rule gets one more step).
- Quiet controls are interface only: a `reader--idle` class after 3 s without pointer or key
  activity while not hovering a control; never while a menu is open; respects
  `prefers-reduced-motion`.
- The choice is not saved: Aoide starts windowed.

### Slices

1. The command and event; contract.
2. F11 and Esc order. Seam: shortcut tests.
3. Idle fade. Seam: vitest with fake timers.

### Done when

- Fullscreen works on Wayland and X11 (check both by hand; the window manager decides).
