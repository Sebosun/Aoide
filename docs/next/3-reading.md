# 3. Reading: the page

Read [plan.md](plan.md) first.

Checked against the code on 2026-11-16: the "Check" items are answered in place, and decided
questions are under **Settled**.

The reader is `src/views/ReaderView.vue` with `src/components/reader/`: `ChapterText.vue` draws
a chapter, `ChapterSegment.vue` draws one sentence group (it carries `data-segment-id` and plays
on click), `ReaderLines.vue` is the Lines view, `ReaderTools.vue` the reader's controls. A
chapter arrives as `ChapterView` (`src-tauri/src/book.rs:79`) with `SegmentView`s.

Two facts every plan here leans on:

- **The page is not the display text.** `SegmentView` (`book.rs:47`) carries `words`
  (`display.split_whitespace()`), `speakable`, `note`, `notes` and no offsets. `ChapterSegment`
  draws the words joined by single spaces, footnote markers as `<button>`s after the word they
  follow (holding the label and, without margin notes, the open popover's text), and a
  trailing space. Margin notes are an `<aside>` beside the block. Spoken footnotes
  (`read_notes`) are extra segments with ids `{chapter}:{block}:n{k}`, `note: true` and an
  empty range (`with_note_segments`, `book.rs`). `Segment::words`
  (`crates/segmenter/src/lib.rs:47`) are the chapter-text ranges of the same whitespace-split
  words, so word *i* of `SegmentView.words` is `Segment::words[i]`.
- **A chapter is plain data, replaced whole.** The reader store keeps each `ChapterView`
  `markRaw` (`src/stores/reader.ts:53`, test "keeps a loaded chapter as plain data"). A new
  `ChapterView` gives every `ChapterSegment` a new `segment` prop and redraws the whole chapter.
  Anything that changes while a chapter is open (highlights) travels beside it, not in it.

---

## 3.1 A menu on selected text

**Size:** M. Several other features hang their entry point here.
**Built:** 2026-11-17 (fb2a6af..daea241)

### Outcome

Select text in the reader and a small menu appears: **Copy**, **Highlight**, **Search the
book**, **Listen from here**, **Say this differently…**. Right-click a sentence without
selecting for **Listen from here**, **Say this again**, **Bookmark here**. (**Look up** belonged
to 3.4, which is not planned for now.)

### What exists

- Nothing handles `contextmenu` in `src/`. 1.2 added `SayDifferently.vue`, a button at the top
  of the reader that follows `selectionchange` while a few words are selected.
- A click on a sentence plays it: `ChapterSegment.vue` binds `@click`, Enter and Space to
  `emit('play')` when `clickable`. A drag that selects inside one sentence ends with a click on
  that span, so the handler must not play when the selection is not collapsed.
- `UiMenu.vue` opens from a button in its slot. Its placement, `usePopover`
  (`src/composables/usePopover.ts`), only calls `getBoundingClientRect()` on the anchor, so a
  selection's rectangle can anchor a popup once the anchor's type is widened to
  `{ getBoundingClientRect(): DOMRect }`. Entries are `MenuAction { id, label, icon, disabled,
  action }` (`src/components/ui/types.ts`).
- `BookSearch.vue` searches the book (`api.searchBook`); a picked hit **plays** from its
  segment. `navigation.findInBook()` (`src/stores/navigation.ts:75`) opens Book tools on Search
  with no query.
- `player.play(bookId, chapter, segmentId)` (`src/stores/player.ts`) exists.
- `useShortcuts.ts` ignores its plain keys while Shift is held, but 2.7 gave Shift ← / → to
  chapters and Shift ↑ / ↓ to the volume. So Shift+arrows are the selection's only on a
  sentence reached with the keyboard, or while text is selected in the chapter (the reader's
  listener stops them reaching the player's); a sentence that was clicked keeps them for the
  player. WebKit extends an existing selection with Shift+arrows but cannot start one on a
  page that is not editable (no caret browsing).
- **Bookmark here** has no command yet: `add_bookmark(book_id, note)` bookmarks the playhead or
  the saved place (`Core::add_bookmark`, `core/bookmarks.rs`), never a chosen sentence.

### Design

**Pure TypeScript: `src/components/reader/selection.ts`.**

```
export interface TextRange { chapter: number; start: number; end: number; text: string;
                             firstSegmentId: string; }
export function rangeOfSelection(selection: Selection, chapterRoot: HTMLElement,
                                 chapter: ChapterView): TextRange | null
```

`start` and `end` are character offsets into the chapter's display text, the same coordinates
the segmenter uses (`Segment::range`). One offset per sentence is not enough: the page rejoins
words with single spaces, and an article's preformatted block keeps its line breaks and
indentation in the display text (`preformatted` in `crates/article/src/blocks.rs`). So
**`SegmentView` gains `starts: Vec<usize>`**, the chapter offset of each word. A point in the
DOM becomes (sentence by `data-segment-id`, word index, character in the word) and then
`starts[word] + character`; in the space between words, a selection's end is the end of the
word before it and its start the start of the word after it, and an end outside any sentence
(a triple click ends at the next block) goes back to the last word before it, so a range starts
and ends on words. `rangeOfSelection` reads `starts` from the `ChapterView`, so the DOM needs no
offsets of its own. Its tests draw the core's chapters with their display text
(`src/tests/fixtures/chapter-texts.json`, written by `chapter_texts_for_the_ui_tests` in
`core/fixtures.rs`: the preview books and `book::tests::every_shape`).

Not part of the display text, and skipped when counting: footnote markers and margin notes
(mark them `data-not-text`), and spoken-footnote segments (`note: true`).

Offsets are counted in Unicode scalar values, as in Rust, not UTF-16 units (`Array.from` on the
word): a test with an emoji and with a combining accent guards it.

**The menu: `ReaderSelectionMenu.vue`.** Appears above the selection when the pointer is
released or after a keyboard selection settles (300 ms); closes on scroll, Esc, or a click
elsewhere. Actions are props, so features add theirs without editing the menu: each action has
an id, a label, and "applies to" (a selection, a few words, a sentence).

**What applies** comes from the core where it is a rule (`can_say_again` from 1.1) and from the
selection's shape otherwise (Say this differently applies to one to four words, `fewWords` in
`selection.ts`, as the button 1.2 put at the top of the reader does; both open the same dialog,
`navigation.sayDifferently`).

As built: `ReaderSelection.vue` watches the selection beside `ChapterText` and in Lines and opens
`ReaderSelectionMenu.vue`; the entries are `TextAction`s (`textActions.ts`: id, label,
`applies`, an optional `available` rule, `run`), and `useTextActions()` gives the reader's.

**Search the book.** `findInBook(query?)` carries the selection to `BookSearch.vue`, which
fills in the field and runs the search.

**Bookmark here.** `add_bookmark` gains an optional `segment_id`: when given, the bookmark is
on that sentence instead of the playhead (`Core::add_bookmark_at`; an unknown sentence is
refused, and so is any sentence of an audiobook without text).

**The right click.** On selected text it opens the selection's menu; on a sentence without a
selection, the sentence's (Listen from here, Say this again when `canSayAgain`, Bookmark here).
Both take the focus and put back the selection WebKit clears when a button is focused. WebKit
may follow a right click with a click: the next click in the text after it does not play.

**Lines view.** The same menu; `ReaderLines.vue` draws whole sentences with
`data-segment-id`, so the same rule applies. Its lines move while listening; a selection there
is short-lived, which is acceptable.

**Copy.** Plain text of the selection, without note markers.

### Slices

1. `starts` on `SegmentView`; contract and fixtures. Seam: `book.rs` test that `starts[i]`
   is `Segment::words[i].start` for every fixture chapter.
2. `rangeOfSelection`: within one sentence, across sentences, across paragraphs, with a note
   marker inside, with a spoken footnote between, with astral characters. Seam: vitest with a
   DOM drawn from a fixture chapter.
3. A selection does not start playback; a plain click still does.
4. The menu with Copy, Search the book and Listen from here (`usePopover` takes a rectangle).
5. The right-click menu on a sentence; `add_bookmark` with a `segment_id`.
6. Keyboard: Shift+arrows from the focused sentence start a selection there (set in script:
   → its first word, ← its last word, ↓ ↑ all of it), then extend it as WebKit does; the menu
   key (or Shift F10, listed with the shortcuts) opens the menu; actions reachable by keyboard.

### Logging

`ui.selection` through `log_ui`, with the action id in `command`. `UiLog`
(`src-tauri/src/logging.rs:138`) has no `book` or `action` field; the actions that change
something log in the core (`highlight.added`). Never the text.

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
  `char_offset`, `segment_id`), an `excerpt` and a `note`, or on an audiobook a time
  (`position_ms`, `add_time`). The table cascades with the book (library version 2).
  `BookBookmarks.vue` lists them; Book tools (`BookToolsSheet.vue`) has the tabs Bookmarks,
  Search and Details, and only Bookmarks and Details for an item without text.
- The `Bookmarks` component holds the library and no host trait; `Core::add_bookmark` passes it
  the playhead's entry from `playback.position()`. `Position { index, offset_ms }`
  (`crates/audio/src/player.rs:31`) also says how far into that entry's audio playback is,
  which is what "the sentence just spoken" needs.
- `ChapterSegment.vue` redraws one sentence when its props change (review P6; test "does not
  draw the chapter again…" in `src/tests/reader.test.ts`). Highlights must keep that, and a
  `ChapterView` is replaced whole (see the top of this file), so highlights cannot travel in it.
- Book ids are `content_hash` of the file's bytes (`crates/library/src/lib.rs:473`). A corrected
  edition is a new id; the same file added again is the same id. Article ids are
  `canonical_id(url)` (`core/articles.rs`). The text under a kept id changes in two ways: an
  article fetched again (`refetch_article`), and a change to the extraction rules in a newer
  Aoide (1.4 and 3.6 are candidates). Anchoring is for those.
- Values that outlive a book: `BookKey::Settings` and `BookKey::ProgressFraction` live in
  `settings` with no foreign key (`crates/library/src/values.rs:45`). Tables that `REFERENCES
  books(id) ON DELETE CASCADE` go in `Library::remove_book` (`lib.rs:339`).
- Colours: the Default palette is `src/styles/theme.css` (light on `:root`, dark under the
  media query and `data-theme`); `aoide_config::TOKENS` (`crates/config/src/lib.rs:28`) lists
  what a theme may override. The built-in themes are `crates/config/themes/*.toml`; a theme
  without a token keeps the Default value.
- `tauri_plugin_dialog` is registered (`src-tauri/src/lib.rs`), for choosing where to export.

### Design

**Data (next library version).**

```
CREATE TABLE highlights (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  book_id TEXT NOT NULL,              -- no foreign key: kept when the book is removed
  chapter INTEGER NOT NULL CHECK(chapter >= 0),
  start INTEGER NOT NULL CHECK(start >= 0),
  end INTEGER NOT NULL CHECK(end > start),
  text TEXT NOT NULL,                 -- the highlighted text, at most 2,000 characters
  note TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL
);
CREATE INDEX highlights_book ON highlights(book_id, chapter, start);
```

Highlights are the user's writing: like `BookSettings`, they outlive the book, and adding the
same file (or saving the same address) again brings them back (decided 2026-11-16). One colour.

**Component `core/highlights.rs`** (model: `core/bookmarks.rs`).

- `add(book, chapter, start, end)`: the core cuts `text` from the chapter itself (the interface
  sends a range, not text), joins a highlight that overlaps or touches an existing one, and
  refuses a range outside the chapter or longer than 2,000 characters.
- `add_spoken(book)`: highlights the sentence at the playhead (`Position.index`), or the
  speakable one before it in the same chapter when `Position.offset_ms` is under 1,000 (you
  press the key after hearing it). Refused for an item without text.
- **Anchoring**, when a book's highlights are listed (its model is loaded for that). Each
  highlight's stored `text` is compared with the chapter text at its range. If it differs, the
  text is searched for in the chapter, the range moved and written back; if it is not found the
  highlight is `detached`: listed, not drawn.

**Views.** Highlights stay out of `ChapterView`. `highlights(book_id) -> Vec<HighlightView {
id, chapter, chapter_title, start, end, text, note, detached }>`, in reading order. The reader
store keeps the open book's list reactive, beside the raw chapters; `add_highlight`,
`update_highlight_note` and `delete_highlight` return the new list.

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

- Drawing: `ChapterText` turns the list into the parts that fall inside each sentence
  (`[start, end, id]` relative to the sentence, using `SegmentView.starts` from 3.1) and hands
  each `ChapterSegment` its parts; a sentence with none gets the same empty array every time, so
  a new highlight redraws only the sentences it covers. Colour from one new theme token
  (`mark`), added to `aoide_config::TOKENS` and to `theme.css` in light and dark.
- Clicking a highlight opens a small card: the note (editable), Remove, Copy.
- The Bookmarks tab of Book tools becomes **Notes**: highlights and bookmarks in reading
  order, a filter for each, **Export…** and **Copy as Markdown**. An audiobook's Notes tab
  lists bookmarks only.
- The spoken-sentence key is `H` (2.7), with a toast "Highlighted" and Undo.

### Slices

1. Table and library functions; a highlight survives `remove_book`.
2. `add`: text cut by the core, overlap joined, out-of-range and too-long refused.
3. Anchoring: unchanged, moved (and written back), detached. Seam: component test with two
   versions of a chapter.
4. `add_spoken` at the playhead and in the first second of the next sentence.
5. `notes_markdown`: a golden file for a small synthetic book with highlights, notes and
   bookmarks across two chapters.
6. `HighlightView`, the commands, contract, fixtures.
7. Drawing without redrawing other sentences (the P6 test pattern: count renders).
8. The Notes tab, the card, export. Mock scenario "highlight, then it is listed".

### Logging

`highlight.added` (`book`, `chapter`, `chars`, `from` = `selection` or `spoken`),
`highlight.removed`, `highlight.detached`, `notes.exported` (`book`, `highlights`, `bookmarks`).

### Done when

- A highlight made in Page view shows in Lines view and after a restart.
- A book removed and added again has its highlights.
- The exported file opens cleanly in a Markdown editor.

### Settled

- Highlights and notes are kept when the book is removed, like `BookSettings`; adding the book
  again brings them back (decided 2026-11-16).
- One highlight colour (decided 2026-11-16).
- Bookmarks and highlights stay two things: a bookmark is a place, a highlight is a passage
  (plan's default).

---

## 3.3 Reader typography

**Size:** S
**Built:** 2026-11-17 (3cb8937..74fcee1)

### Outcome

In the reader's tools: the font, the line spacing, the width of the text column, justified or
ragged text, and hyphenation. Today there is one font and five text sizes.

### What exists

- `aoide_config::Appearance` (`crates/config/src/lib.rs:206`): `mode`, `theme`, `text_size`
  0–4, `reader_mode`, `zoom`, with `#[serde(default)]` on the struct, so a file without the new
  fields loads with their defaults. Saved with `save_appearance`; hand edits to `settings.toml`
  apply (review A4).
- Nothing in Rust checks `text_size`; `applyAppearance` (`src/theme.ts:52`) clamps it to
  `TEXT_SCALES` and sets `--read-scale`. A value of the wrong type (an unknown name) fails the
  whole `[appearance]` section: the defaults are used and `Config::problems`
  (`src-tauri/src/core/config.rs:209`) lists the section's error.
- Where the look comes from: the line height is `1.68` on `.chapter-text`
  (`ChapterText.vue`); the column is a grid track, `minmax(0, 640px)`, in `ReaderSheet.vue`
  (wide and narrow layouts), between margins of 150–204 px that hold the book and margin
  notes; the font is `--font-read` (`src/styles/theme.css`).
- The style test (`src/tests/styles.test.ts`) checks gaps, paddings, margins and radii only.
  Widths and line heights are outside the token rule.
- The reading font is Newsreader, bundled in `src/assets/fonts/` (Latin and extended Latin,
  with its OFL), declared in `src/styles/fonts.css`, loaded ahead by `readingFont()` in
  `src/fonts.ts`. The CSP allows no outside fonts, so every font is bundled.
- Nothing sets `lang` in the reader; `BookView.language` has the book's language.
- `ReaderTools.vue` is an "Aa" popover with the size steps. `AppearanceSettings.vue` has a text
  size slider. `UiSelect` and `UiSegmented` exist.

### Design

**`Appearance` gains** (each with a default equal to today's look):

| Field | Values |
|-------|--------|
| `reader_font` | `serif` (Newsreader), `sans` (the interface font), `legible` (Atkinson Hyperlegible Next, OFL), `system` (`system-ui`) |
| `line_spacing` | 0–4, 2 is today's (1.68) |
| `text_width` | 0–4, 2 is today's (640 px) |
| `justify` | bool, false |
| `hyphenate` | bool, false |

Steps, not raw numbers, as `text_size` does. The step-to-value tables sit beside `TEXT_SCALES`
in `src/theme.ts`; `applyAppearance` clamps each step as it does the text size and sets
`--read-line` and `--read-width` on the root, which `ChapterText.vue` and `ReaderSheet.vue`
use. An unknown `reader_font` in a hand edit is a section problem like any other malformed
value. As built, the font, justification and hyphenation are root properties too
(`--read-font` from `READER_FONTS`, `--read-align`, `--read-hyphens`), with today's values as
defaults in `theme.css`; `--font-read` stays Newsreader for the rest of the interface.

**Hyphenation** uses the browser (`hyphens: auto`), which needs the text's language: the
chapter root gets `lang` from `BookView.language`. WebKitGTK hyphenates with the system's
libhyphen dictionaries (`/usr/share/hyphen`; Fedora's `hyphen-en` is installed on the
development machine, WebKitGTK 2.54). A language without a dictionary is not hyphenated, which
is acceptable. Check by hand in slice 5.

**Fonts.** Add the font files under `src/assets/fonts/` with their licences, their faces in
`fonts.css`, and make `readingFont()` load the chosen face. A font is loaded only when chosen.
As built: Atkinson Hyperlegible *Next* (the variable successor, weights 200–800 with italics,
so headings at 600 are not faked; from Fontsource 5.3.0, Latin and extended Latin).
`applyAppearance` starts loading the chosen font, and `readingFont()` with no argument waits
for the font chosen last (the saved place is shown once it is in).

**Lines view** follows the font and spacing; width and justification do not apply to it.

**Interface.** `ReaderTools.vue`: a "Text" popover with the size (already there), font
(`UiSelect`), spacing and width (`UiSegmented` or steps), two switches. Settings › Appearance
(`AppearanceSettings.vue`) shows the same controls. As built: one component,
`ReaderTypography.vue`, in both places; the font is a `UiSegmented` of four (a `UiSelect`'s
list is teleported, a popover inside a popover), size, spacing and width are `ReaderSteps.vue`,
and the switches are checkboxes, as elsewhere in the app.

### Slices

1. `Appearance` fields with defaults; old `settings.toml` files load unchanged; an unknown font
   name is reported as a section problem. Seam: `cargo test -p aoide-config`.
2. Contract and fixtures.
3. The step tables and properties; the default steps give 1.68 and 640 px; out-of-range steps
   are clamped. Seam: vitest on `applyAppearance`.
4. Fonts loaded on choice; the page is not laid out twice (the reason `fonts.ts` exists).
5. The popover; screenshots in light and dark at the extremes; hyphenation checked by hand.

### Done when

- Every combination keeps the sentence shading and the word mark aligned.
- Scrolling stays as smooth as the `ui.frames` log showed before (compare it).

---

## 3.4 Dictionary

**Not planned for now** (decided 2026-11-16).

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

- `search_book` (`commands/library.rs:168`, `Core::search_book` in `core/playback.rs:936`,
  `BookModel::search` in `book.rs`) searches the open book in memory, segment by segment, at
  most 200 hits; a picked hit plays from its segment. The library's search field filters by
  title and author.
- A book's text exists only after parsing; the scan stores metadata, not text. Articles keep
  their snapshot and are extracted on open (`ShelfHost::article_document`).
- `Shelf::load` (`core/shelf.rs:550`) parses outside the library lock, but keeps at most three
  models and, when full, drops all but the one playing. Indexing through it would push the
  open book's model out each time; the index job parses on its own (`parse_document` in
  `book.rs`, `article_document` for articles) and keeps nothing.
- FTS5 is compiled in: `rusqlite` 0.32 with `bundled` builds `libsqlite3-sys` 0.30.1, whose
  build script passes `-DSQLITE_ENABLE_FTS5` (and `-DSQLITE_ENABLE_DBSTAT_VTAB`). Slice 1 still
  guards it with a test.
- Items: `books.kind` (book, article, audiobook; audiobooks have no text) and `books.shelf`
  (`library` or `feed`, library version 11).
- Book rows are deleted in two places: `Library::remove_book` (`crates/library/src/lib.rs:339`)
  and `merge_audiobook` (`merge.rs`).
- `Generation::busy()` (`core/generation.rs:225`) is true while the worker makes any segment,
  for listening or for preparing.
- `aoide_text::block_offsets` (`crates/segmenter/src/lib.rs:95`) gives each block's offset.

### Design

**Data (next library version).**

```
CREATE TABLE passage_rows (
  id INTEGER PRIMARY KEY,
  book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  chapter INTEGER NOT NULL, block INTEGER NOT NULL,
  text TEXT NOT NULL
);
CREATE INDEX passage_rows_book ON passage_rows(book_id);
CREATE VIRTUAL TABLE passages USING fts5(
  text, content = 'passage_rows', content_rowid = 'id',
  tokenize = 'unicode61 remove_diacritics 2'
);
-- and the three triggers SQLite's FTS5 documentation gives for an external-content table,
-- so inserts and deletes on passage_rows reach passages.
CREATE TABLE indexed (
  book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  version INTEGER NOT NULL,          -- of the indexing rule; a bump re-indexes
  blocks INTEGER NOT NULL
);
```

One row per block (paragraph). A row's place is `chapter` and `block`. The text lives in an
ordinary table indexed by book, because an FTS5 table with an `UNINDEXED book_id` column has
no index on it: removing or re-indexing one book would read the whole library's text. Removing
a book removes its rows in the same transaction through the cascade, on both deletion paths;
slice 1 tests that the FTS rows go too.

Size: the index is roughly the size of the text again, a few megabytes for a long novel. Add it
to the storage report (`core/storage.rs`; `dbstat` measures it) and offer "Search inside
books" as a switch in Settings › Library for people who do not want it.

**Component `core/indexing.rs`,** a background job like the others:

```
enum Stage { Idle, Indexing { book: BookId, done: usize, of: usize }, Paused }
```

- Works through items on the library shelf, with text, with no `indexed` row or an old
  version, oldest opened last, one at a time, on a blocking thread; steps aside while the
  worker makes speech (the host's `busy()` is `Generation::busy()`).
- An item is also indexed when it is opened (its model is parsed anyway) and an article when
  its fetch completes.
- Parsing for the index happens outside the library lock (it is the leaf lock).

**Query.** `search_library(query, limit) -> Vec<LibraryHits { book_id, hits: Vec<PassageHit {
chapter, block, segment_id, snippet, marks }>, more: u32 }>`: the user's words become an FTS
query of quoted terms joined by AND, with a prefix match on the last term; operators typed by
the user are treated as text. Ranking by `bm25`, three passages per item, items ordered by
their best passage. `snippet()` gives the excerpt with match markers, which the core converts
to ranges (`marks`) so the interface draws them (no HTML crosses). `segment_id` is the
sentence of the block that holds the first match, found by the core.

**Interface.** The library search shows title matches as now and, below, **In the text**:
items with their passages. Enter on a passage opens the reader at that sentence with the words
marked, without playing (unlike `BookSearch.vue`'s pick, which plays). Shortcut Ctrl+Shift+F.

### Slices

1. FTS5 is available; the migration; rows (and FTS rows) removed with the book on both paths.
2. Index one synthetic book; a query finds a passage with the right chapter, block and
   sentence; diacritics and case fold.
3. Query building: quotes, operators as text, prefix on the last word, empty query.
4. The job: stages, steps aside, resumes after restart, re-indexes on a version bump, does not
   touch the shelf's kept models.
5. Articles indexed on fetch and re-fetch; feed-shelf items are not.
6. Opening the reader at a passage.
7. Interface, storage report, the switch (turning it off drops the tables' rows).

### Logging

`index.item` (`book`, `blocks`, `took_ms`, `outcome`), `index.search` (`hits`, `took_ms`; never
the query).

### Done when

- A phrase remembered from any book is found in under 100 ms in a 500-book library.
- Indexing a large library never makes listening stutter.

### Settled

- Feed entries that were never saved are not indexed: only the library shelf (plan's default).
- No stemming; `porter` exists for English only (plan's default).

---

## 3.6 Pictures, tables and code

**Size:** L

### Outcome

A book's pictures show in the reader. The voice can say a picture's description. Tables are
shown as tables and announced instead of being read as a stream of cells. Code keeps its
lines, and is announced or skipped, as you choose.

### What exists

- A block is text: `BlockKind { Heading, Paragraph, ListItem, Preformatted }`
  (`crates/document/src/lib.rs:146`). There is no picture block and the reader draws no
  pictures. `Chapter.text` is always the blocks' texts joined by blank lines.
- **What the EPUB parser does today** (`read` in `crates/epub-parser/src/content.rs`). Block
  elements are `h1`–`h6`, `p`, `li`, `pre`, `div`, `section`, `br` and `tr`; only `p`, `li`,
  `pre` and headings set a kind, the rest inherit it.
  - `<img>` (and `<svg><image>`) has no text and is not a block: nothing is kept, not its
    `alt`, and no warning. The EPUB parser never emits `Warning::LeftOut`.
  - `<figure>` and `<figcaption>` are not block elements: the caption's text runs into the
    line that is open. Figures sit between paragraphs, so the caption is already a paragraph
    of its own, in the display text, and read aloud.
  - `<table>`: each `<tr>` is a block, and `<td>`/`<th>` add a space, so a row is one
    paragraph of its cells' texts joined by single spaces; where one cell ends is lost. A
    `<caption>` is a paragraph before the first row; a `<p>` inside a cell starts a block of
    its own. This is the stream of cells, and it is spoken today.
  - `<pre>` gives `Preformatted`, but every whitespace character becomes a space and runs
    collapse, so line breaks and indentation are gone; `white-space: pre-wrap` in
    `ChapterText.vue` has nothing left to keep.
  - MathML's text nodes are added with nothing between them.
  - A section with no text (a full-page illustration) is dropped with
    `Warning::EmptySection` (`filter::partition`).
  - There is no public way to read a resource from the archive (`Archive` is private), and
    `Cover` carries the cover's bytes, not its path.
- **What the article extractor does** (`crates/article/src/blocks.rs`): `img`/`picture`,
  `figure` (caption included), `table`, embeds and forms are left out whole and counted
  (`LeftOut`), and the reader says so. `pre` keeps its text as written (line breaks,
  indentation; `br` as a newline). `math` is read by `formula`, symbols spaced. In the reader
  the line breaks are still lost: `SegmentView.words` is `split_whitespace`.
- The lead image is fetched as the article's cover into the covers folder (`keep_cover`,
  `core/articles.rs`); the asset protocol's scope is `$APPDATA/covers/**`
  (`src-tauri/tauri.conf.json`). `image` 0.25 (jpeg, png, webp, gif, bmp) is a dependency of
  `src-tauri`.
- What depends on the display text: progress and bookmarks resolve by `segment_id` first, then
  `char_offset` (`Bookmarks::add`, `BookModel::index_of`, `index_at`), and segment ids contain
  the block index; 3.2's highlights are offsets; a paired recording's sync map is stored with
  a `text_hash` over every chapter's text (`core/recordings/layout.rs:194`), and a different
  hash makes the recording be matched again (`SyncEvent::TextChanged`).

### The decision: A, blocks that take no text

A chapter's display text is its blocks' texts joined, and stored places are offsets into it.
The two ways were:

- **A. Nothing enters or leaves the display text.** Pictures and article tables are inserts
  between blocks; no offset moves, no segment id changes. Speech for an insert is an extra,
  separately addressed entry.
- **B. Real blocks**, with a one-time re-anchoring of every stored place by its excerpt.

**A** (settled by the code check, 2026-11-16). The audit answers it: in EPUBs the captions and
the table rows are already text and pictures have none, so A needs no change to an EPUB's
display text at all; only the drawing and the speech change. B would gain nothing for EPUBs
and would cost a re-anchoring of progress, bookmarks and highlights, new segment ids, and a
new match of every paired recording whose book has a picture or a table. For articles,
whatever is left out today stays out of the text: picture captions and table cells travel in
the insert.

### Design (A)

**Where inserts live.** Not on the block before (a picture can come before the first block):
`Chapter.inserts: Vec<Insert>`, each with `before_block: usize` (`blocks.len()` at the end).
The EPUB parser places them as it places note references (`Content.refs` and
`blocks_and_refs`: a token index becomes a block and offset).

**Document model.**

- `Insert::Picture { resource, alt, caption }`, where `caption` is `Block(usize)` for an EPUB
  (the caption paragraph that is already in the text) or `Text(String)` for an article.
- `Insert::Table { rows: Vec<Vec<String>>, header_rows, caption }`, articles only.
- EPUB tables stay blocks: `Chapter.tables: Vec<TableShape { blocks: Range<usize>,
  caption_block: Option<usize>, header_rows, cells: Vec<Vec<Range<usize>>> }>`, the cell
  ranges inside each row's text recorded where `<td>`/`<th>` open and close.
- Code: `Block.layout: Option<String>` on a `Preformatted` block from an EPUB, the text with its
  whitespace kept (the same words in the same order as the collapsed text). An article's
  preformatted text already is its layout. (`Block` also gains `break_before` in 1.4.)

**Views.** `BlockView` gains `layout` for preformatted blocks; `ChapterView` gains the
chapter's inserts (picture URL, alt, caption) and table shapes. The reader draws a
preformatted block with the layout's whitespace and the segments' words, so selection and
highlights still address words.

**Pictures.**

- EPUB: `resource` is a path inside the archive, resolved against the chapter's source
  document. `crates/epub-parser` gains a pure `read_resource(bytes, path)`. The core extracts a
  picture when its chapter is opened, into `…/com.aoide.audiobooks/pictures/<book id>/`,
  scaled down if huge (`image`), and the asset protocol scope grows by `$APPDATA/pictures/**`.
  The source book is only read. The cover (compared by the package's cover path, which the
  parser has in `package.rs`) and tiny decorative images (under about 32 px once decoded, or
  `role="presentation"`) are left out.
- Pictures in sections with no text stay out: keeping such a section would add a chapter and
  move every later chapter's index. They remain in the warnings, as today.
- Articles: only the lead image is fetched today. Fetching every picture means more requests to
  the site and its CDNs when saving. Make it a setting ("Keep an article's pictures"), off by
  default, through `crates/fetch` with the same reach rule and a size cap.

**Speech.** `AppSettings.describe { pictures: Off | Caption | AltAndCaption, tables: Announce |
Read | Skip, code: Read | Announce | Skip }`.

- An EPUB picture's caption is a paragraph and is read as one, as today. An article's caption
  is read as an extra entry when `Caption` is chosen. Alt text is read only when chosen
  ("Picture: …").
- A table is announced ("A table of 4 columns and 12 rows: *caption*") by default. For an
  EPUB table its row segments become silent (empty speech, same ids). `Read` speaks it row by
  row with the header before each cell, which changes the rows' speech, not their display.
- Code (`Preformatted`): `Announce` says "A code sample" and silences the block's segments.
- The spoken extras have their own entry ids (`"{chapter}:{block}:i{n}"`, beside the
  `:n{k}` ids of spoken footnotes), so they do not disturb sentence ids.
- Changing `describe` unloads the shelf's models and re-plans, as 1.4's pauses do.

**Reader.** Pictures between paragraphs, sized to the column, click to enlarge. Tables as
tables with horizontal scroll. In Lines view an insert shows as a single line ("Picture:
*caption*") that opens it.

### Slices

1. **Audit.** A synthetic EPUB with a picture, a figure with a caption, a table with a
   caption and a code block: a test that records what the parser produces today (the list
   above). It stays as the guard that later slices leave the display text alone.
2. `Insert` and `Chapter.inserts`; EPUB pictures and their caption block parsed; display text
   identical on every fixture.
3. Extraction to the pictures folder, removal with the book, the storage report.
4. The reader draws pictures.
5. Tables: EPUB shapes with cell ranges, drawn; article tables as inserts.
6. Code: EPUB `layout`, drawn with its lines for EPUBs and articles.
7. Speech choices for pictures, tables and code, with entry ids that leave the others alone.
8. Articles: the setting, the fetches, the snapshot.

### Logging

`pictures.extracted` (`book`, `count`, `bytes`, `took_ms`), `article.pictures` (`book`,
`fetched`, `skipped`, `bytes`).

### Done when

- An illustrated book looks right, and every place stored before the change still points at
  the same sentence; a paired recording is not matched again.
- A table is never read as a run of numbers unless asked for.

### Settled

- A, blocks that take no text (settled by the code check, 2026-11-16; reasons above).
- Formulas (MathML): nothing beyond today's reading (plan's default).

---

## 3.7 Fullscreen and a quiet reader

**Size:** S

### Outcome

F11 makes the reader fill the screen. When reading silently, the controls fade after a few
seconds without the pointer moving and return when it moves.

### What exists

- The reader already takes the whole window. `src-tauri/capabilities/default.json` grants
  `core:default` and webview zoom only.
- Nothing in `src-tauri/src` listens to window events (no `on_window_event`), and Tauri 2 has
  no fullscreen-changed event: a change made by the window manager shows as a resize, after
  which `is_fullscreen()` tells.
- The contract skips `WebviewWindow` parameters (`core/contract.rs:316`), so a command can take
  the window.
- Esc: `plain()` in `src/composables/useShortcuts.ts` leaves the reader when nothing in
  `OVERLAYS` is open. F11 is unused; 2.7 lists it.
- "Reading silently" is `reader.shape === "read"` (`Shape::Read`, `core/reading.rs`).

### Design

- Fullscreen through a Rust command (`set_fullscreen(window: WebviewWindow, on: bool)`),
  which keeps the capability list small and is covered by the contract. The window reports its
  state back so Esc and the window manager's own shortcut stay in step: `lib.rs` reads
  `is_fullscreen()` on `WindowEvent::Resized` and, when it changed, emits
  `CoreEvent::Fullscreen(bool)` through the core's emitter, so the contract types it.
- Esc leaves fullscreen first, then the reader (`plain()` gets one more step before
  `leaveReader`).
- Quiet controls are interface only: a `reader--idle` class after 3 s without pointer or key
  activity while not hovering a control and while `shape` is `read`; never while anything in
  `OVERLAYS` is open; respects `prefers-reduced-motion`.
- The choice is not saved: Aoide starts windowed.

### Slices

1. The command and event; contract.
2. F11 and Esc order. Seam: shortcut tests.
3. Idle fade. Seam: vitest with fake timers.

### Done when

- Fullscreen works on Wayland and X11 (check both by hand; the window manager decides).
