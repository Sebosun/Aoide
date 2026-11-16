# What comes next: feature plans

**Status:** proposed 2026-11-16. Nothing here is built. Written for an implementing agent and for
choosing what to build. Read this file fully, then only the feature you are working on.

These plans came out of a review of what a reading and listening app is expected to do and
Aoide does not do yet. Each feature is planned on its own, so they can be built in any order
unless "Needs" says otherwise. PDF support is being built separately and is not planned here;
[4-library.md](#) (More formats) builds on it.

## The features

Size is a rough guess: **S** is a day or two, **M** about a week, **L** more than that or with
an open design question.

| # | Feature | File | Size | Needs |
|---|---------|------|------|-------|
| 1.1 | Say one sentence again | [1-narration.md](#11-say-one-sentence-again) | S | |
| 1.2 | Pronunciation list | [1-narration.md](#12-pronunciation-list) | M | |
| 1.3 | Leave chapters out of the narration | [1-narration.md](#13-leave-chapters-out-of-the-narration) | M | |
| 1.4 | Pauses between paragraphs and at scene breaks | [1-narration.md](#14-pauses-between-paragraphs-and-at-scene-breaks) | S | |
| 1.5 | A second voice for dialogue | [1-narration.md](#15-a-second-voice-for-dialogue) | L | |
| 2.1 | Step back a little on resume | [2-listening.md](#21-step-back-a-little-on-resume) | S | |
| 2.2 | Time left in the book | [2-listening.md](#22-time-left-in-the-book) | S | |
| 2.3 | A speed for each book | [2-listening.md](#23-a-speed-for-each-book) | S | |
| 2.4 | A gentler sleep timer | [2-listening.md](#24-a-gentler-sleep-timer) | S | |
| 2.5 | Listen next: a queue | [2-listening.md](#25-listen-next-a-queue) | M | |
| 2.6 | Shorter silences in recordings | [2-listening.md](#26-shorter-silences-in-recordings) | M | |
| 2.7 | More keyboard shortcuts | [2-listening.md](#27-more-keyboard-shortcuts) | S | |
| 3.1 | A menu on selected text | [3-reading.md](#31-a-menu-on-selected-text) | M | |
| 3.2 | Highlights, and exporting notes | [3-reading.md](#32-highlights-and-exporting-notes) | M | 3.1 |
| 3.3 | Reader typography | [3-reading.md](#33-reader-typography) | S | |
| 3.4 | Dictionary | [3-reading.md](#34-dictionary) | L | 3.1 |
| 3.5 | Search the whole library's text | [3-reading.md](#35-search-the-whole-librarys-text) | M | |
| 3.6 | Pictures, tables and code | [3-reading.md](#36-pictures-tables-and-code) | L | |
| 3.7 | Fullscreen and a quiet reader | [3-reading.md](#37-fullscreen-and-a-quiet-reader) | S | |
| 4.1 | Edit a book's details | [4-library.md](#41-edit-a-books-details) | M | |
| 4.2 | Series | [4-library.md](#42-series) | M | 4.1 |
| 4.3 | Archive, and quicker collections | [4-library.md](#43-archive-and-quicker-collections) | S | |
| 4.4 | Goals | [4-library.md](#44-goals) | S | |
| 4.5 | More formats | [4-library.md](#45-more-formats) | M each | PDF |
| 4.6 | Calibre libraries and OPDS catalogues | [4-library.md](#46-calibre-libraries-and-opds-catalogues) | M, L | 4.2 |
| 4.7 | An inbox folder | [4-library.md](#47-an-inbox-folder) | S | |
| 4.8 | Backup and moving to another computer | [4-library.md](#48-backup-and-moving-to-another-computer) | M | |
| 5.1 | Import from read-later apps | [5-articles-feeds.md](#51-import-from-read-later-apps) | M | |
| 5.2 | Send a page from the browser | [5-articles-feeds.md](#52-send-a-page-from-the-browser) | S | 6.1 |
| 5.3 | Feed folders | [5-articles-feeds.md](#53-feed-folders) | S | |
| 5.4 | Podcasts | [5-articles-feeds.md](#54-podcasts) | L | |
| 5.5 | Newsletters | [5-articles-feeds.md](#55-newsletters) | S | |
| 6.1 | Open with Aoide, one running copy | [6-desktop.md](#61-open-with-aoide-one-running-copy) | M | |
| 6.2 | Notifications | [6-desktop.md](#62-notifications) | S | |
| 6.3 | Keep working in the background | [6-desktop.md](#63-keep-working-in-the-background) | M | |
| 6.4 | Stop an idle speech server | [6-desktop.md](#64-stop-an-idle-speech-server) | S | |
| 6.5 | Releases, Windows and macOS | [6-desktop.md](#65-releases-windows-and-macos) | L | |
| 6.6 | Listening on a phone | [6-desktop.md](#66-listening-on-a-phone) | direction only | |

## Suggested order

1. **The first week's annoyances:** 1.1, 1.4, 2.1, 2.2, 2.7, 6.2. All small, all noticed by
   anyone who listens for an hour.
2. **What local speech needs:** 1.2, then 1.3.
3. **The reader:** 3.3, 3.1, 3.2.
4. **The desktop:** 6.1, then 5.2 and 4.7, then 6.3 and 6.4.
5. **The library:** 4.1, 4.2, 4.3, 4.8.
6. **Everything else**, by appetite. 1.5 and 5.4 are the two that would make Aoide stand out and
   the two that cost the most.

## Corrections to the review

Two things the review called missing are partly there:

- **Time left.** The player shows the time left in the *chapter* (`chapter_left_ms` in
  `core/wire.rs`, estimated in `src/composables/usePlayerControls.ts` for speech). What is
  missing is the time left in the *book* (2.2).
- **Tags.** Collections already do what tags do. 4.3 makes collections quick to use and adds an
  archive, instead of adding a second system.

## How to work

The same way as [../reading-app/plan.md](../reading-app/plan.md): one slice at a time, a failing
test at the slice's seam first, the checks of that plan after every slice, one commit per slice.
`AGENTS.md` applies. What these plans lean on:

- **A component per feature.** New state lives in its own module under `src-tauri/src/core/`,
  owns its locks, reaches the rest through a host trait `Core` implements, and is tested beside
  it without a `Core` (see [../core-refactor.md](../core-refactor.md), section Component).
  Models to copy are named in each plan.
- **Explicit state.** Anything with states is one enum changed by one transition function over
  named events, with a table-driven test.
- **The core decides.** A rule the interface needs ("can this sentence be said again", "is this
  chapter left out", "what plays next") is a field of a core view, tested in Rust. The mock
  serves the core's fixtures and does not re-derive rules.
- **Pure crates.** Parsing and text rules go in `crates/*` with no Tauri, interface or network.
- **Contract.** After changing a type that crosses IPC or a command:
  `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`. After changing a view the mock serves:
  `AOIDE_WRITE_FIXTURES=1 cargo test -p aoide fixture`. A rule the mock plays out gets a
  scenario in `scenarios_for_the_ui_mock`.
- **Logging.** Each plan lists its events. Every new event gets a row in
  [../logging.md](../logging.md). Fields carry book ids and file names only: never book text,
  never a pronunciation, a highlight or a search query.
- **Vue.** BEM, `<style scoped>`, sizes from `src/styles/tokens.css`, `@/` imports.

## Rules shared by every plan

- **Library schema.** A feature that needs a table or a column adds *one new step at the end*
  of `MIGRATIONS` in `crates/library/src/store.rs`. Shipped steps are never edited. The plans
  say "the next library version" because the number depends on merge order.
- **Segment ids and offsets are addresses.** Progress, bookmarks and (after 3.2) highlights
  point into a chapter's display text by `chapter` and `char_offset`, and by `segment_id`
  (`"{chapter}:{block}:{n}"`). A change that moves either for an existing book must say how
  stored places are carried over. 1.5 and 3.6 are the plans this applies to.
- **Audio is addressed by what was asked for.** A segment's audio is stored under
  `aoide_tts::cache_key` (engine kind, adapter version, model, text, voice, language,
  parameters). A feature that changes the text or the voice of a sentence gets new audio for
  that sentence and only that sentence, without any invalidation code. 1.2 and 1.5 rely on this.
- **Nothing new leaves the computer by default.** A feature that fetches something (a
  dictionary, a podcast episode, a catalogue) says so in the interface, goes through
  `crates/fetch` with `Reach::Public`, and gets a line in the README's "What leaves your
  computer".
- **Source files are never changed.** Edits to details, series, pronunciations and highlights
  live in the library database.

## What each plan contains

- **Outcome**: what the user can do afterwards.
- **What exists**: the code the feature starts from, with paths. Read those files first; where
  a plan says "check", the review did not read far enough to be sure.
- **Design**: the rule, where it lives, the data, the commands and views, the interface.
- **Slices**: the order to build in, each with the seam its test drives.
- **Logging**, **Done when**, and **Open questions** for the user to answer before or during
  the work.


---

# 1. Narration: what the voice says and how

Read [plan.md](plan.md) first.

What these five features share: `aoide_text::Segment` (`crates/segmenter/src/lib.rs`) already
separates `display` (what the page shows) from `speech` (what is sent to the engine) and carries
`pause_after_ms`. `Generation::speech_plan` (`src-tauri/src/core/generation.rs:148`) turns each
segment's `speech` into a `SynthesisRequest` through `Voicing::request` and addresses its audio
by `aoide_tts::cache_key`. Almost everything below is a change to what goes into that request.

---

## 1.1 Say one sentence again

**Size:** S

### Outcome

Right-click a sentence (or use the reader's menu on the sentence being spoken) and choose **Say
this again**. Aoide throws that sentence's audio away and makes it again. Local engines
sometimes garble, repeat or cut off a sentence; today the only cure is `regenerate_book`, which
deletes the whole book's audio.

### What exists

- `regenerate_book` (`src-tauri/src/commands/speech.rs:117`) and the core method behind it: it
  asks the worker to forget the book's keys (`Generation::forget`, which returns a `Forgotten`
  to await), deletes the stored segments, and tells the session the audio is gone
  (`Session::audio_lost`, `NarrationPlan::audio_lost`). Read that path first: this feature is
  the same path for one key.
- `NarrationPlan::request(index)` and the plan's items give the key of an entry.
- The cache key holds the request only. The same request gives the same key.

### Design

**Rule (core).** A sentence can be said again when its entry is spoken by an engine (`Fate`
says so in `core/narration.rs`; not a recording clip, not a silent entry) and its audio exists.
The chapter view's `SegmentView` gains `can_say_again: bool`, so the interface never works this
out.

**Two steps, because engines differ.**

1. *Make it again.* Forget the key, await `Forgotten`, delete the segment row and file, report
   the loss to the session, and let the normal plan make it: it is at or near the playhead, so
   it is first in line. If it is the sentence playing now, playback restarts that sentence once
   the new audio is in.
2. *Make it differently.* An engine that is deterministic for a fixed seed returns the same
   audio. When the engine's descriptor has an integer parameter named `seed`, saying a sentence
   again must vary it. Store a **take** per sentence:

   ```
   CREATE TABLE segment_takes (
     book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
     segment_id TEXT NOT NULL,
     speech_hash TEXT NOT NULL,     -- of Segment::speech; a take for other text is ignored
     take INTEGER NOT NULL CHECK(take > 0),
     PRIMARY KEY (book_id, segment_id)
   );
   ```

   `Voicing::request` takes the take and, when the descriptor has `seed`, adds it to the seed
   parameter. The request differs, so the key differs, and nothing else needs to know. For an
   engine with no `seed` the take changes nothing and step 1 alone applies (a sampling engine
   gives a different result anyway).

**Component.** A small `core/takes.rs` owning the table access, or methods on `Generation` if it
stays under about 60 lines. Host needs: none beyond the library.

**IPC.** `say_again(book_id, segment_id) -> ()`. The existing generation and player events
report the rest.

**Interface.** In the sentence's context menu (3.1 builds the menu; until then, a button in
`ReaderTools.vue` acting on the current sentence). While the sentence is being made, it shows
the same "being made" state the reader already shows ahead of the playhead.

### Slices

1. `say_again` on a cached, not-playing sentence: the segment row is gone, the worker is asked
   for the key again, the scripted engine receives one more request. Seam: `Generation` with
   `aoide_tts::testing`, beside `core/generation_tests.rs`.
2. On the playing sentence: playback pauses on it and replays it from its start when the audio
   arrives. Seam: core test with the scripted engine.
3. `can_say_again` on `SegmentView`: false for recording clips, silent entries and uncached
   sentences. Fixture and mock scenario.
4. Takes: with a `seed` parameter in the descriptor, the second request's seed differs and its
   key differs; a take whose `speech_hash` no longer matches is ignored. Seam: `Voicing` unit
   test and a library test for the table.
5. Interface: the menu entry, its busy state, preview screenshot.

### Logging

`speech.say_again` (`book`, `segment`, `take`, `outcome`).

### Done when

- A garbled sentence can be replaced without touching any other audio.
- With Chatterbox's seed set to a fixed value, the replacement differs from the original.
- A book's `prepared` mark and its M4B export still hold after a sentence is replaced.

### Open questions

- The same text with the same voice in two books shares a key. Deleting it for one book makes
  the other book make it again too. Acceptable, or should deletion wait until no book's plan
  holds the key?
- Keep the previous take for an "undo"? The plan says no: the old file is deleted.

---

## 1.2 Pronunciation list

**Size:** M

### Outcome

Tell Aoide how to say a word: "Hermione → her-MY-oh-nee", "SQL → sequel", "Dr. Xu → doctor
shoe". A list for each book and one for all books. The page keeps the book's spelling; only the
voice changes. Sentences that contain the word are made again; nothing else is.

### What exists

- `Segment::speech`, built by `speech_text` (`crates/segmenter/src/lib.rs:480`), which already
  removes note markers.
- `Voicing` (`src-tauri/src/core/voicing.rs`): "how a book is voiced". `Voicing::request(text)`
  is the one place a request is made from a segment's text.
- Word timings are "per whitespace-separated word of the request text"
  (`aoide_tts::Synthesis::word_timings`), and the estimate in the segmenter works per word of
  the segment. **Check** how `Session::provided` and `book::word_index` map timings to
  `Segment::words`: this is where a replacement that changes the number of words would break
  the word highlight.

### Design

**Pure rule: `aoide_text::pronounce`.**

```
pub struct Rule { pub written: String, pub spoken: String, pub match_case: bool }
pub struct Spoken {
    pub text: String,
    /// For each whitespace-separated word of `text`, the index of the word of the input it
    /// stands for.
    pub word_of: Vec<usize>,
}
pub fn pronounce(speech: &str, rules: &[Rule]) -> Spoken
```

- Matches whole words only (word boundaries by `char::is_alphanumeric`, so "art" does not match
  inside "start"), a rule's `written` may be several words ("Dr. Xu").
- Longest `written` first, then book rules before global rules, then in list order. The same
  input and rules always give the same output.
- Possessives and plurals: `written` "Xu" also matches "Xu's" and keeps the "'s". No other
  inflection.
- `word_of` lets the caller fold the engine's timings back: three spoken words standing for one
  written word become one timing from the first start to the last end.

**Data (next library version).**

```
CREATE TABLE pronunciations (
  id INTEGER PRIMARY KEY,
  book_id TEXT REFERENCES books(id) ON DELETE CASCADE,   -- NULL: every book
  written TEXT NOT NULL, spoken TEXT NOT NULL,
  match_case INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  UNIQUE(book_id, written)
);
```

The global list also has to survive a lost database and be shareable. **Decide** (open
question) whether it lives in `~/.config/aoide/pronunciations.toml` instead, like site rules.

**Where it is applied.** `Voicing` gains `rules: Arc<[Rule]>` (book rules then global), and
`Voicing::request` runs `pronounce` on the segment's speech. The cache key then differs for the
sentences a rule touches and for no others. Voice preview and export go through the same
request, so they follow without more work. A paired recording is not affected.

**Timings.** Wherever engine timings or the estimate are attached to an entry, fold them with
`word_of` first so they are again one per word of `Segment::words`. The local aligner aligns
audio to text: it must be given the *spoken* text and its result folded the same way (check
`core/align.rs`).

**Component.** `core/pronunciation.rs` (model: `core/bookmarks.rs`): list, add, change, remove,
and "what does this change": `affected(book_id) -> AffectedView { sentences, cached }`, so the
interface can say "12 sentences will be made again" before saving.

**After a change.** The plan of the book being listened to is rebuilt (the path
`save_book_settings` takes when the voice changes; check that it calls `follow`). A prepared
book loses its `prepared` mark for the affected sentences and the item page offers to prepare
the rest.

**IPC.** `pronunciations(book_id: Option) -> Vec<PronunciationView>`,
`save_pronunciation(book_id: Option, written, spoken, match_case) -> PronunciationView`,
`delete_pronunciation(id)`, `preview_pronunciation(book_id, spoken)` (through the existing
`preview_voice` path, with the book's voice).

**Interface.** Book tools › Voice gets a **Pronunciation** list: written, spoken, a play button
to hear the spoken form, remove. Settings › Speech engine gets the same list for all books. In
the reader, selecting a word offers **Say this differently…** (3.1), which opens a small dialog
with the word filled in and a **Hear it** button.

### Slices

1. `pronounce`: whole words, several-word rules, longest first, case, possessive, `word_of`.
   Seam: `cargo test -p aoide-text`.
2. Table and component: add, list, change, remove; book rules before global. Seam: component
   test with a temporary library.
3. `Voicing::request` applies the rules: a plan built with a rule has a different key for the
   sentence with the word and the same keys elsewhere. Seam: `Generation::speech_plan` test.
4. Timings folded: a rule turning one word into three still highlights one word per written
   word. Seam: session test with scripted timings.
5. Saving a rule while listening re-plans; the affected count is right. Seam: core test.
6. Interface: the two lists, the dialog, preview. Mock scenario for "save a rule, the sentence
   is made again".

### Logging

`pronunciation.saved`, `pronunciation.removed` (`book` or `scope="all"`, `affected`). Never the
words.

### Done when

- A name said wrong is fixed once and stays fixed for the whole book.
- Changing a rule makes only the sentences that contain the word again.
- The word highlight stays on the written word whatever the rule says.

### Open questions

- Global list in the database or in a hand-editable file?
- Engines that accept phonemes (Kokoro takes `[word](/phonemes/)`): allow raw engine markup in
  `spoken`? It would not carry over to another engine. The plan says plain respelling only.

---

## 1.3 Leave chapters out of the narration

**Size:** M

### Outcome

On a book's contents, untick the chapters you do not want to hear: the copyright page, the
table of contents, the index, forty pages of endnotes. Listening skips them, preparing the
whole book leaves them out, and the time estimates stop counting them. They are still there to
read.

### What exists

- `BookContents.vue` lists chapters; `prepare_chapter` and `PrepareView.chapters` already allow
  preparing some chapters.
- `NarrationPlan` (`core/narration.rs`) holds a `Fate` per entry, changed by `next(fate,
  event)`. Read it first: "left out" is most likely a new fate.
- `skip_chapter` moves the playhead by chapters.
- The parser already excludes some sections and reports them (`ExcludedView`); that is
  extraction, and this is a listening choice. Keep them apart.

### Design

**Data.** A book value `BookKey::LeftOut` (in `crates/library/src/values.rs`), a JSON list of
chapter indexes. It should outlive the book being removed and added again, like the voice
settings, so it is stored in `settings` (`Place::Settings`).

**Rule (core), in a new `core/leaving_out.rs`.**

- Playback that reaches the start of a left-out chapter continues at the start of the next
  chapter that is not left out; at the end of the book it finishes as a book does.
- Starting playback *inside* a left-out chapter (the user clicked a sentence there) plays it:
  an explicit request wins. Leaving out only applies to arriving from the chapter before.
- Whole-book preparation and M4B export leave the chapters out. `prepare_chapter` on a left-out
  chapter still works.
- Listening estimates (item page, time left in book) leave them out. Reading progress and the
  reading-time estimate do not: you may still read them.
- A paired recording: the same rule on its sync map. Narration that is "off text" already has
  its own handling (`off_text`); do not touch it.

**Suggestions, not automation.** A pure function proposes chapters by title:
`aoide_text::front_and_back_matter(titles, language) -> Vec<usize>` with a short list per
language ("Copyright", "Contents", "Acknowledgements", "Index", "Notes", "Bibliography", "About
the author", "Also by"). The interface offers **Leave out front and back matter**, which ticks
those and shows which. Nothing is left out until the user agrees: the project rule is to keep
uncertain content.

**Views.** `ChapterSummary` gains `left_out: bool` and `suggested: bool`. `BookView` already
carries the actions that apply (review A3); add the count of left-out chapters for the item
page's Audio card ("3 chapters left out").

**IPC.** `set_left_out(book_id, chapters: Vec<usize>) -> BookView`.

**Interface.** A checkbox per chapter in `BookContents.vue` while its "choose what is narrated"
mode is on; left-out chapters are dimmed with a small "not narrated" label. In the reader's
chapter menu they carry the same label.

### Slices

1. `front_and_back_matter`: English titles, case and punctuation, nothing proposed for a novel
   whose chapters are all numbered. Seam: `cargo test -p aoide-text`.
2. The value is stored, listed in `ChapterSummary`, and kept across remove and add.
3. Playback arrives at a left-out chapter and continues after it; the last chapter left out
   ends the book. Seam: core test with the scripted engine (see how `tests.rs` drives chapter
   ends).
4. Clicking a sentence in a left-out chapter plays it.
5. Preparation and its progress count leave the chapters out; the `prepared` mark is reached.
6. Export leaves them out, with chapter markers still right.
7. Recording: the same for a paired audiobook.
8. Interface and mock scenario.

### Logging

`narration.left_out` (`book`, `chapters` as a count), `playback.skipped_left_out` (`book`,
`from`, `to`).

### Done when

- A book with front matter starts speaking at chapter one after one click.
- No audio is made for a left-out chapter unless the user asks for that chapter.

### Open questions

- Should M4B export have its own switch ("export everything")?
- Per-book only, or also a global "always leave out copyright pages" setting? The plan says
  per-book, with the suggestion one click away.

---

## 1.4 Pauses between paragraphs and at scene breaks

**Size:** S

### Outcome

The voice breathes: a short pause between paragraphs, a longer one at a scene break, both
adjustable. Today only headings are followed by a pause (700 ms).

### What exists

- `SegmentOptions::heading_pause_ms` and `Segment::pause_after_ms`
  (`crates/segmenter/src/lib.rs:20`). A segment with empty `speech` plays its pause only.
- Silent entries are planned as `silent:{index}` with `duration_ms = pause_after_ms`
  (`core/generation.rs:171`). Pauses are not part of any cache key, so changing them makes no
  audio again.
- **Check** `NarrationPlan::player_entries`: how a spoken segment's `pause_after_ms` reaches
  the player, and whether export (`core/export.rs`) writes the same silence.
- **Check** what the EPUB parser does with `<hr>` and with paragraphs such as `* * *` or `⁂`.
  A paragraph with no letter or digit already becomes a segment with empty speech. An `<hr>`
  is probably dropped.

### Design

**Segmenter.** `SegmentOptions` gains `paragraph_pause_ms` and `break_pause_ms`. The last
segment of a block gets `paragraph_pause_ms`; a segment before a scene break gets
`break_pause_ms`; headings keep `heading_pause_ms`. The largest applies, they do not add up.

**Scene breaks.** Two sources:

- A block whose text has no alphanumeric character (`* * *`): it is the break. Its own segment
  stays silent and the pause is attached to it.
- An `<hr>` or an element styled as a break: the document model must carry it without adding a
  block, because block indexes are part of segment ids. Add `break_before: bool` to
  `aoide_document::Block`, set by `crates/epub-parser` and `crates/article`. No offsets move.

**Settings.** `AppSettings.pauses { paragraph_ms, break_ms, heading_ms }`, defaults 250, 1200
and 700, each 0–3000. Changing them rebuilds the open book's segments and re-plans. Settings ›
Speech engine: three sliders and a **Hear it** button that plays two short paragraphs with the
current values through the preview path.

**Durations.** Estimates of listening time include the pauses (they already include silent
entries).

### Slices

1. Segmenter: paragraph and break pauses, the largest wins, the last block of a chapter gets
   the heading-sized pause before the next chapter title. Seam: `cargo test -p aoide-text`.
2. `break_before` from `<hr>` in a synthetic EPUB and in an article. Seam: each parser's tests.
3. Settings carried into `SegmentOptions`; a change re-plans without new requests to the
   engine. Seam: core test counting scripted-engine requests.
4. Export writes the same silences.
5. Interface.

### Logging

None new (a settings change is already logged).

### Done when

- Paragraphs are audibly separated at the default settings.
- Changing a pause makes no audio again.

### Open questions

- Per-book pauses (poetry wants more)? The plan says global only.

---

## 1.5 A second voice for dialogue

**Size:** L. Build 1.1 to 1.4 first; this one changes how books are segmented.

### Outcome

In Book tools › Voice, choose a second voice for speech in quotation marks. The narrator reads
the narration; the second voice reads what characters say. Off unless chosen.

### What exists

- `segment_chapter` groups sentences into segments of 40 to 250 characters within a block; one
  segment makes one request with one voice.
- `BookSettings` (`core/wire.rs:201`) holds the per-book voice; `Voicing` resolves it.
- Segment ids are addresses (see plan.md). Places are also stored with `char_offset`, and
  `BookModel::index_at(chapter, char_offset)` finds the segment at an offset.

### Design

**Pure rule: `aoide_text::dialogue`.** `quoted_spans(block_text, language) -> Vec<Range>`:

- Pairs `“ ”`, `" "`, `« »`, `„ “`, and `‘ ’` only when the block has no double quotes (to
  avoid apostrophes). A quote left open at the end of a paragraph continues into the next one
  when that one opens with a quote mark (the usual convention for long speeches).
- Dash dialogue (`— Hello, said …`) is not detected in the first version.
- A span shorter than `min_chars` or of one word ("yes") is still a span; the grouping rule
  below decides whether it gets its own request.

**Segmenter.** `SegmentOptions.split_dialogue: bool`. When on, a segment never mixes roles: it
is cut at span boundaries, except that a span under about 25 characters next to narration in
the same sentence stays with the narration (too short to switch voice without sounding
chopped). `Segment` gains `role: Role { Narration, Dialogue }`.

**Segment ids change for a book when this is turned on or off.** Stored places survive because
they carry `char_offset`: on open, progress and bookmarks whose `segment_id` is not found are
looked up with `index_at`. **Check** that every reader of a stored `segment_id` falls back this
way before building anything else; add the fallback where it is missing, as its own slice.

**Voicing.** `BookSettings.dialogue_voice_id: Option<String>`; `Voicing::request(text, role)`
picks the voice. Narration sentences keep their keys (same text, same voice) unless the cut
changed their text. Turning the feature on therefore makes again only the segments that held a
quote.

**Engines.** Cloned-voice engines (Chatterbox) switch voice per request at no cost. For an
engine that loads a model per voice, switching may be slow: `EngineDescriptor` gains
`cheap_voice_switch: bool`, and the interface warns when it is false.

**Interface.** A second voice picker under the first, with **Preview** playing a narration
sentence and a quoted sentence from the book. Dialogue is not marked on the page.

### Slices

1. Stored places fall back to `char_offset` when their `segment_id` is unknown. Seam: core test
   that re-segments a book with different options and reopens it.
2. `quoted_spans`: each quote style, apostrophes, a speech over two paragraphs, an unclosed
   quote at the end of a chapter (treated as closed).
3. Segmenter with `split_dialogue`: roles, the short-span rule, `display` still equal to the
   chapter text in `range`, sentences and words still covering the same text.
4. `Voicing::request` by role; keys of untouched narration are unchanged.
5. Settings, preview, the warning for slow voice switches.
6. Export: M4B carries both voices; nothing else to do if it reads the same plan.

### Logging

`voicing.dialogue` (`book`, `outcome`, `segments`, `dialogue_segments`).

### Done when

- A chapter of ordinary fiction switches voices at the right places, with no audible clicks at
  the cuts (pauses between entries apply as for any two segments).
- Turning it off restores the old segments and their cached audio is used again.

### Not in this plan

A voice per character. It needs speaker attribution, which no deterministic rule does well. If
wanted later, it builds on `Role` becoming `Role::Speaker(id)` and a per-book cast list filled
in by hand.

### Open questions

- Inner thoughts in italics: a third role? Not planned.
- Should the short-span threshold be a setting? The plan says a constant.


---

# 2. Listening: the player

Read [plan.md](plan.md) first.

The player's state is `PlayerStateView` (`src-tauri/src/core/wire.rs:108`), built in
`core/playback.rs`. What sits around playback (the sleep timer, media keys) is
`core/controls.rs`. The interface side is `src/composables/usePlayerControls.ts` and
`src/components/player/`.

---

## 2.1 Step back a little on resume

**Size:** S

### Outcome

Come back after a break and playback starts a little earlier, so you catch the thread: from
the start of the sentence after a short pause, a sentence or two earlier after a long one.

### What exists

- `pause`, `resume` and `play` in `commands/playback.rs`; `Playback` (`core/playback.rs`) owns
  the player. Review A1 made every playhead move say what moved it (`Core::follow_through`):
  read that first, a resume that moves the playhead is one more named move.
- A saved place is a segment and a character offset; opening a book and pressing play starts
  from it.

### Design

**Pure rule, in a new `core/resuming.rs`.**

```
pub struct Away { pub paused_for: Duration, pub cold_start: bool }
pub enum StepBack { None, SentenceStart, Sentences(u8) }     // speech and synced recordings
pub fn step_back(away: Away) -> StepBack
pub fn step_back_ms(away: Away) -> u64                        // an audiobook with no text
```

| Away | With text | Audiobook alone |
|------|-----------|-----------------|
| under 10 s | nothing | nothing |
| 10 s to 5 min | start of the current sentence | 3 s |
| 5 min to 1 h | one sentence earlier | 10 s |
| over 1 h, or the first play after starting Aoide | two sentences earlier | 20 s |

It never crosses the start of the chapter, and never applies after a seek, a click on a
sentence or a chapter skip (the user chose that place).

**State.** `Playback` records when it paused. A cold start is "no session has played since
launch".

**Setting.** `AppSettings.step_back_on_resume: bool`, on by default, in Settings › Library or
wherever playback preferences end up (there is no playback tab yet; Speech engine is not the
place).

### Slices

1. `step_back` and `step_back_ms`: the table above as a table-driven test.
2. Resume after a scripted 10 minutes (inject the clock; see how `controls.rs` and the tests
   deal with `Instant`) starts one sentence earlier and reports the move with its cause.
3. Not across a chapter start; not after a seek.
4. A recording alone steps back in milliseconds.
5. The setting, off means nothing moves.

### Logging

`playback.resumed` gains `stepped_back` (entries or ms) and `away_s`.

### Done when

- Resuming after lunch replays the last sentence or two.
- Pause and play in quick succession changes nothing.

### Open questions

- Should the thresholds be settings? The plan says one switch.

---

## 2.2 Time left in the book

**Size:** S

### Outcome

The player shows time left in the book as well as in the chapter, at the current speed, and
when the book would end ("ends 23:40"). Click the time to switch between chapter and book.

### What exists

- `chapter_left_ms` in `PlayerStateView`: exact for a recording, `None` for speech, where
  `usePlayerControls.ts:44` estimates it from the words left.
- `NarrationPlan::playing_ms(range)` (`core/narration.rs:270`) sums the durations of a range of
  entries: real where audio is cached, estimated elsewhere.
- For an audiobook alone, `position_ms` and `duration_ms` already give it.

### Design

**The core decides.** `PlayerStateView` gains `book_left_ms: Option<u64>`: from the playhead to
the end, at the current speed, leaving out chapters left out by 1.3. For speech it is
`playing_ms(playhead..end)` scaled by speed. It changes with every sentence, so compute it when
the state is built (the plan's sums can be kept as a suffix-sum array rebuilt when the plan
changes; do not sum a whole book 30 times a second).

While here, move the chapter estimate for speech into the core as well, so `chapter_left_ms` is
always `Some` and `usePlayerControls.ts` stops estimating. That removes a rule from the
interface (AGENTS.md: let the core decide).

**Interface.** `PlayerDock` and `MiniPlayer`: the time label toggles chapter and book; the
choice is kept in `Appearance` or local storage (it is a view preference, not a core one). The
tooltip gives the end time by the wall clock.

### Slices

1. `book_left_ms` for speech: cached and estimated entries, speed 1.5. Seam: playback test.
2. For a paired recording and for an audiobook alone.
3. `chapter_left_ms` for speech from the core; delete the estimate in the composable; fixtures
   regenerated.
4. Interface toggle and end time.

### Logging

None.

### Done when

- The figure falls steadily while playing and changes at once when the speed changes.
- The interface holds no listening-time arithmetic for the playing book.

---

## 2.3 A speed for each book

**Size:** S

### Outcome

A book can keep its own playback speed. A slow narrator stays at 1.4×, poetry at 0.9×, and
everything else follows the usual speed.

### What exists

- Speed is one value in `settings.toml` (`[player]`, `core/config.rs:148`), applied by
  `Playback::set_speed`. A hand edit of the file changes it (review A4).
- `BookSettings` holds per-book overrides. Its `params` may contain an engine parameter also
  called `speed` (the engine's speaking rate, part of the request and the cache key). That is a
  different thing: name the new field `playback_speed`.

### Design

- `BookSettings.playback_speed: Option<f32>`.
- When a session starts, the core applies the book's speed if it has one, else the usual one.
- `set_speed(speed)` while a book with its own speed plays changes that book's speed; otherwise
  it changes the usual one. The command does not grow a flag: the core knows which applies.
- `PlayerStateView` gains `speed_scope: SpeedScope { Usual, Book }` so the pill can show it.
- `set_book_speed(book_id, on: bool)`: turning it on copies the current speed into the book;
  off removes it and returns to the usual speed.

**Interface.** `SpeedPill.vue`: a switch "Only for this book" under the speeds, and a small dot
on the pill when the book has its own.

### Slices

1. A book with its own speed starts at it; the next book returns to the usual one.
2. `set_speed` goes to the right place in each case; the usual speed in `settings.toml` is not
   touched by a book's.
3. `speed_scope` in the view; fixture; mock scenario.
4. The pill.

### Logging

`playback.speed` gains `scope`.

### Done when

- Switching between two books switches speed with them.

---

## 2.4 A gentler sleep timer

**Size:** S

### Outcome

The sleep timer stops at the end of a sentence, fades out over its last seconds, and if you
are still awake, pressing play within a few minutes starts the same timer again.

### What exists

- `Controls` in `core/controls.rs`: `SleepTimer { At(Instant), EndOfChapter(usize) }`,
  `set_sleep`, `sleep_due(chapter)` checked by the player tick, `sleep_view`.
- Volume is the user's setting (`set_volume`). **Check** `crates/audio/src/player.rs` for a
  separate gain the fade can use; do not move the user's volume.

### Design

**An explicit state** (the timer becomes a small machine, as AGENTS.md asks):

```
enum Sleep {
    Off,
    Running { until: Until, minutes: Option<u32> },      // Until::At(Instant) | EndOfChapter
    Fading { ends: Instant, minutes: Option<u32> },      // last 20 s of a timed sleep
    Rang { at: Instant, minutes: Option<u32>, end_of_chapter: bool },
}
enum SleepEvent { Set(..), Cancelled, Tick(Instant), EntryEnded, ChapterEnded, Played(Instant) }
fn next(sleep: Sleep, event: SleepEvent) -> (Sleep, Option<SleepAction>)
enum SleepAction { Fade(f32), Pause, RestoreGain }
```

- A timed sleep that is due waits for `EntryEnded` (the end of the sentence) before pausing,
  for at most 15 s.
- During `Fading` the player's gain falls to about 20%, then the pause restores it.
- `Rang` lasts 3 minutes: `Played` in that time starts the same timer again. After that it is
  `Off`.

`SleepView` gains `fading: bool` and `extended: bool` only if the pill shows them.

### Slices

1. The transition table.
2. The player's separate gain (if it does not exist): a ramp that does not change `volume`.
3. Wiring: a due timer pauses at a sentence end; play within 3 minutes re-arms it.
4. The pill shows "again, 15 min" after an extension.

### Logging

`sleep.set`, `sleep.rang`, `sleep.extended` (`minutes`, `end_of_chapter`).

### Done when

- The timer never cuts a word.
- Volume after a sleep is exactly what it was before.

---

## 2.5 Listen next: a queue

**Size:** M

### Outcome

Line up things to listen to. When one ends, the next starts by itself. Add from the library,
the item page, a feed entry or Home; reorder and remove in a panel. Meant for articles: queue
five and go for a walk.

### What exists

- `HomeView.up_next_ids` (`core/facets.rs`): a suggestion list, nothing plays from it.
- Finishing an item is an event in `core/reading.rs` (`ReadEvent`, `next_read`).
- `aoide_generation::plan(listening, preparing)` decides what is made next; `Preparing` jobs
  step aside for listening.
- A feed entry becomes an item on the `feed` shelf when opened (`open_feed_entry`).
- MPRIS is served in `src-tauri/src/media.rs`.

### Design

**Data (next library version).**

```
CREATE TABLE queue (
  book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  position INTEGER NOT NULL
);
```

**Component `core/queue.rs`** (model: `core/bookmarks.rs` for the store, `core/preparation.rs`
for the stage):

```
enum Stage { Idle, Between { next: BookId, since: Instant }, }
enum Event { ItemEnded(BookId), Tick(Instant), UserPlayed, UserStopped, Changed }
```

- When the playing item ends and the queue is not empty, wait 2 s (`Between`), then ask the
  host to play the first queued item from its saved place and take it off the queue. Anything
  the user does in between cancels the hand-over.
- An item is taken off the queue when it starts, not when it ends.
- Items that cannot be listened to (an article still being fetched, a missing file, no voice)
  are passed over and stay queued, marked with why.
- Host trait: `play(book_id)`, `can_listen(book_id) -> Result<(), Why>`.

**Being ready in time.** When the playing item has under 2 minutes left, the first queued item
is given to generation as a low-priority job for its first minute of audio, so the hand-over
has no gap. Use the existing `Preparing` path limited to the entries needed; do not add a
second planner.

**Views and IPC.** `QueueView { items: Vec<QueueItemView { book_id, why_not: Option<..> }> }`,
`queue()`, `queue_add(book_id, place: Next | Last)`, `queue_remove`, `queue_move(book_id, to)`,
`queue_clear()`, event `queue-changed`. `BookSummary` gains `queued: bool`. A feed entry is
queued with `queue_add_entry(entry_id)`, which opens it onto the feed shelf first.

**Interface.** "Listen next" and "Add to queue" in the item menus and on `FeedEntry.vue`; a
queue list in the player dock (a button with the count) with drag to reorder (`src/drag.ts`
exists); "Then: *title*" under the time left when the item is nearly over. The media keys'
*next* goes to the next queued item when the playing one has no further chapter.

### Slices

1. Store: add next and last, move, remove, removed with the book.
2. Stage transitions (table).
3. An item ends, the next plays from its place; a user action in the 2 s cancels. Seam:
   component with a scripted host.
4. Unlistenable items are passed over with a reason.
5. The first minute of the next item is made ahead. Seam: generation test.
6. Feed entries.
7. Interface and mock scenario ("an item ends, the queued one plays").

### Logging

`queue.changed` (`size`), `queue.handover` (`from`, `to`, `outcome`), `queue.passed_over`
(`book`, `why`).

### Done when

- Three queued articles play one after another with no click.
- Closing and starting Aoide keeps the queue.

### Open questions

- Should finishing a queued feed article mark its entry read and archive it (4.3)? Probably yes.
- A chime or a spoken title between items? The plan says the title is not spoken.

---

## 2.6 Shorter silences in recordings

**Size:** M

### Outcome

For a paired or standalone audiobook, long silences are shortened. The item page shows how much
time that saves.

### What exists

- A recording plays as clips per entry, decoded by the feeder (`core/feeder.rs`,
  `core/recording.rs::clip_audio`), with word timings from the sync map.
- The segmenter has frame-based pause detection for its word estimate
  (`crates/segmenter/src/lib.rs:136`, `MIN_PAUSE_MS`, `FRAME_MS`).
- An audiobook with no text plays from its timeline (`core/timeline.rs`); it has no clips per
  sentence. **Check** how it is fed before planning its part.

### Design

**Pure rule in `crates/audio`:** `trim(pcm, rate, options) -> Trimmed { pcm, cuts: Vec<Cut {
at_ms, removed_ms }> }`. Silence is frames under a threshold relative to the clip's level;
any run longer than `keep_ms` (default 500) is shortened to `keep_ms`, cut in its middle with a
short cross-fade.

**Timings.** Cutting inside a clip moves every later word. `cuts` maps a time in the original
to a time in the trimmed clip; word timings are passed through it when the clip is provided to
the session. Leading and trailing silence are the common case and the cheapest: a tail cut
moves nothing.

**Scope.** First version: paired recordings only (they have clips). Standalone audiobooks
second, once it is clear how their stream is fed.

**Settings.** `BookSettings.shorten_silences: Option<bool>` over
`AppSettings.shorten_silences` (off by default). `PlayerStateView.chapter_left_ms` and the
book's time left must use trimmed lengths where known and the original where not.

**Saved time.** Counted as it is played (`saved_ms` per book value), shown as "Shortened
silences saved 14 min".

### Slices

1. `trim`: a 2 s gap becomes 0.5 s, speech samples are untouched, `cuts` maps times correctly.
   Seam: `cargo test -p aoide-audio` with synthetic tones and silence.
2. Word timings pass through `cuts`. Seam: session test.
3. The feeder trims clips when the setting is on; position saving still stores places in the
   original recording's time.
4. Setting, saved time, interface.
5. Standalone audiobooks.

### Logging

`recording.trimmed` (`book`, `removed_ms`, per chapter, not per clip).

### Done when

- A slow narrator's pauses are shorter and the word highlight still lands on the word.
- Bookmarks and the saved place mean the same place with the setting on or off.

---

## 2.7 More keyboard shortcuts

**Size:** S

### Outcome

The keys audiobook players have, listed in Settings › About.

### What exists

- `src/shortcuts.ts` (the list shown) and `src/composables/useShortcuts.ts` (the handling):
  Space, ←/→ by sentence, Ctrl+F, Ctrl+N, Ctrl+comma, zoom, Esc. A test checks them
  (`src/tests/`).
- Commands that already exist: `skip_chapter`, `skip` (seconds, for audiobooks), `set_speed`,
  `set_volume`, `add_bookmark`, `set_sleep_timer`.

### Design

| Keys | Action |
|------|--------|
| Shift+← / Shift+→ | Previous or next chapter |
| Ctrl+← / Ctrl+→ | Back or forward 15 s (audiobook), three sentences (text) |
| `[` / `]` | Slower or faster by 0.1× |
| `\` | Back to 1× |
| ↑ / ↓ | Volume |
| M | Mute |
| B | Bookmark here |
| H | Highlight the sentence being spoken (needs 3.2) |
| T | Sleep timer menu |
| F11 or F | Fullscreen (needs 3.7) |
| `?` | Show this list |

Rules: letter keys do nothing while a text field or an open dialog has focus (the existing
guard); one table in `shortcuts.ts` drives both the list and the handler so they cannot drift
apart. Rebinding is not planned.

### Slices

1. `shortcuts.ts` becomes the single table (key, action id, label); the handler dispatches from
   it. The existing tests pass unchanged.
2. Each new key, with a test that it calls the store action and one that it is ignored in a
   text field.
3. The `?` sheet (reuse the list in `AboutSettings.vue`).

### Done when

- Every row works in the reader and from the library while something plays.
- README's shortcut table is generated from or matches `shortcuts.ts`.


---

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


---

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


---

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


---

# 6. The desktop: being a good application

Read [plan.md](plan.md) first.

The Tauri side is `src-tauri/src/lib.rs` (one window, the dialog plugin, the command list) and
`src-tauri/tauri.conf.json` (`bundle.active` is false: Aoide is built from source and run from
`target/release/aoide`). `src-tauri/capabilities/default.json` grants `core:default` and webview
zoom only. Aoide reads no command-line arguments today.

---

## 6.1 Open with Aoide, one running copy

**Size:** M. 5.2 needs it; file associations in 6.5 need it.

### Outcome

Double-click an EPUB in the file manager and it opens in Aoide, in the window that is already
running if there is one. `aoide book.epub` and `aoide https://…` on the command line do the
same.

### Design

**Pure rule first: `src-tauri/src/launch.rs`.**

```
pub enum Request { OpenFile(PathBuf), SaveLink(Url), Show }
pub fn requests(args: &[String], cwd: &Path) -> Vec<Request>
```

- An argument that is an existing file of a supported format is `OpenFile` (relative paths
  resolved against `cwd`, `file://` addresses accepted).
- `http(s)://…` and `aoide://save?url=…` are `SaveLink` (validated as in 5.2).
- Anything else is ignored and logged. No arguments at all is `Show`.
- Flags are not interpreted: an argument starting with `-` is ignored, so nothing a web page
  or a file name supplies can become an option.

**One running copy.** `tauri-plugin-single-instance`: a second launch hands its arguments and
working directory to the first and exits; the first runs `requests` on them, raises its window
and acts.

**The address scheme.** `tauri-plugin-deep-link` registers `aoide:`. On Linux that is a line in
the desktop file (`MimeType=x-scheme-handler/aoide;application/epub+zip;…`), which exists only
once Aoide is installed (6.5). For a build from source, add a small script or an `aoide
--install-desktop-file` action that writes `~/.local/share/applications/aoide.desktop` pointing
at the binary; document it in the README.

**Acting on a request** (a component `core/launching.rs`, or methods on `Shelf` if that is all
it is):

- `OpenFile`: import through the normal path. A file whose content is already in the library
  is not copied again; either way the item's page opens (`CoreEvent::ItemRequested { book_id,
  then: Open }`; check how the interface is told to navigate today and reuse that).
- `SaveLink`: 5.2.
- Requests that arrive before the interface is ready are held and delivered when it asks
  (`pending_requests()` on start), so a cold start with a file still opens it.

**Capabilities.** Add only what the two plugins need.

### Slices

1. `requests`: files, relative paths, `file://`, links, `aoide://`, flags ignored, unsupported
   files ignored.
2. A request to open a file already in the library opens it without a second copy. Seam: core
   test.
3. A new file is imported, then opened.
4. Requests before the interface is ready are held.
5. Single instance wired; a second launch raises the window (by hand: the plugin is not
   testable in unit tests; add a line to `src-tauri/src/smoke.rs` if it helps).
6. The desktop file writer and the README.

### Logging

`launch.request` (`kind`, `outcome`, `file` name only), `launch.second_instance`.

### Done when

- "Open with → Aoide" works from GNOME Files and Dolphin after the desktop file is installed.
- Starting Aoide twice never gives two windows or two writers on the library.

---

## 6.2 Notifications

**Size:** S

### Outcome

When a long job ends while Aoide is not in front, the desktop says so: a book is prepared, an
audiobook is synced, an export is written, or a job needs you.

### What exists

- Jobs with stages: preparation (`core/preparation.rs`), recording sync (`core/syncing.rs`),
  export (`core/export.rs`), article and feed items that need the user (`NeedsYouNotice.vue`).
- Toasts inside the window (`ToastStack.vue`).

### Design

**Pure rule: `core/notices.rs`.**

```
pub enum Happened { Prepared(BookId), PrepareFailed(BookId), Synced(BookId), SyncFailed(BookId),
                    Exported(BookId), ImportFinished { added: u32 }, BackupFailed }
pub fn notice(happened: &Happened, focused: bool, settings: &NoticeSettings) -> Option<Notice>
pub struct Notice { pub title: String, pub body: String, pub opens: Option<BookId> }
```

- Nothing when the window is focused (the toast is enough).
- Nothing for things the user did a moment ago: only jobs that ran longer than a minute.
- Feed arrivals never notify. A reading app that pings is a worse reading app.
- Text carries the item's title, which is the user's own data shown on their own desktop; it
  is not logged.

**Sending.** `tauri-plugin-notification` behind a host trait `Notifier`, so the rule and the
wiring are tested with a recording notifier. Clicking a notification raises the window and
opens the item (the same `ItemRequested` event as 6.1). Where the desktop has no notification
service the call fails quietly and is logged once.

**Settings.** `AppSettings.notices { enabled: bool }`, on by default, one switch. Per-kind
switches only if someone asks.

### Slices

1. `notice`: each event, focused and not, short jobs silent.
2. Wiring from each job's final transition (where its `next` is applied) to the notifier.
3. Click opens the item.
4. The setting.

### Logging

`notice.sent` (`kind`, `book`, `outcome`).

### Done when

- Preparing a book and switching to another application ends with one notification.

---

## 6.3 Keep working in the background

**Size:** M

### Outcome

Close the window while a book is being prepared or an audiobook synced and Aoide offers to keep
working. A tray icon shows it is still there, with play and pause, and **Quit when done**.

### What exists

- Closing the window ends the process. `AppSettings.resume_prepare` carries on queued
  preparation at the next start.
- Tauri has a `tray-icon` feature. On Linux it needs an app-indicator library at build and run
  time, and GNOME shows no tray without an extension.

### Design

**An explicit state, `core/presence.rs`:**

```
enum Presence { Window, Background, QuittingWhenDone }
enum Event { CloseRequested { working: bool }, Reopened, QuitChosen, QuitWhenDoneChosen, WorkEnded }
fn next(presence: Presence, event: Event, behaviour: CloseBehaviour, tray: bool) -> (Presence, Action)
enum CloseBehaviour { Ask, Quit, KeepWhileWorking, KeepAlways }
enum Action { None, HideWindow, MinimiseWindow, ShowWindow, AskUser, Quit }
```

- "Working" is: something is being prepared, synced, exported, imported or backed up, or
  audio is playing.
- **Without a tray, a hidden window would be unreachable.** When no tray is available, "keep"
  minimises instead of hiding. `tray` is detected at start and is an input to the rule.
- The first close while working asks once ("Keep working in the background?" with *Remember*).
- Background feed checks alone are not "working": Aoide does not stay alive just to poll
  feeds, unless `KeepAlways` is chosen.

**Tray menu.** The playing item with play or pause; what is being worked on, with progress;
Open Aoide; Quit when done; Quit. Built from core views, refreshed on the events the sidebar's
engine card already listens to.

**With 6.1.** A second launch while in the background shows the window.

**Build.** The tray is a Cargo feature (`tray`), on by default, so Aoide still builds where the
indicator library is missing. Add the packages to the README's build steps for each
distribution.

### Slices

1. The transition table, including "no tray".
2. Close while preparing keeps the core running and hides or minimises the window; closing
   while idle quits. Seam: the rule; the window calls by hand.
3. Quit when done quits after the last job's final transition.
4. The tray menu and its refresh.
5. Settings › About or a new General tab: what closing the window does.

### Logging

`presence.changed` (`from`, `to`, `why`).

### Done when

- An overnight preparation is not lost to closing the window.
- On a desktop with no tray, Aoide can always be brought back.

### Open questions

- Start at login, hidden? Not planned.

---

## 6.4 Stop an idle speech server

**Size:** S

### Outcome

If Aoide started the speech server, it stops it after a while without use and starts it again
when needed. A speech model no longer holds the graphics card's memory all evening.

### What exists

- `EngineProfile` has a `LaunchCommand` (`crates/tts/src/lib.rs:414`); `core/engine.rs` starts
  the server, polls it (`poll(busy)`, `Polled`), tracks use (`mark_busy`, `want`) and stops it
  on exit (`shutdown`). Read `engine.rs` and `engine_tests.rs` first: the engine's states are
  already there, and this adds one.
- Only a server Aoide launched can be stopped. One the user runs is theirs.

### Design

- `EngineProfile.stop_after_idle_minutes: Option<u32>` (off by default; offered only when the
  profile has a launch command).
- The engine's state gains "stopped for being idle", entered when Aoide launched the server,
  nothing has been made for the set time, nothing is queued and nothing is playing from speech
  that still needs audio. The next `want()` starts it as a first start does. The status shown
  is "Starting the voice…", never an error.
- Stopping uses the same path as `shutdown` (check that it ends the whole process group: a
  Python server often leaves a child holding the memory).
- Listening to cached audio does not count as use: a prepared book plays with the server
  stopped.

### Slices

1. The idle transition and its timer, with an injected clock (as the engine tests do).
2. `want()` after an idle stop starts the server and generation resumes without an error
   reaching the user.
3. A server not launched by Aoide is never stopped.
4. The whole process group is gone after a stop. Seam: a test launch command that spawns a
   child (opt-in if it is flaky on CI).
5. The setting in Settings › Speech engine.

### Logging

`engine.idle_stop` (`after_s`, `outcome`), `engine.start` gains `why` = `idle_wake`.

### Done when

- `nvidia-smi` shows the memory freed after the set time and taken again on the next play.

---

## 6.5 Releases, Windows and macOS

**Size:** L. Linux packages are already on the roadmap.

### Linux packages

- `bundle.active: true` with targets AppImage, DEB and RPM; icons; a desktop file with the MIME
  types and the `aoide:` scheme (6.1); `productName` and the window title brought up to date
  ("EPUB Audiobooks" no longer describes it).
- **Runtime needs to declare or detect:** WebKitGTK; `ffmpeg` (M4B export runs it,
  `core/export.rs`); the app-indicator library if 6.3 is built; ALSA or PipeWire.
- **Speech recognition:** the default build has `whisper` on the processor. GPU builds stay
  build-from-source, or are separate packages; do not ship CUDA.
- **CI:** a GitHub Actions workflow that builds the three packages on a tag, runs the checks
  from [../reading-app/plan.md](../reading-app/plan.md) first, and attaches checksums. No
  auto-update: the README promises "no update check". Settings › About shows the version and a
  link to the releases page, opened only when clicked.
- **Flatpak** is the format most Linux users would look for. Its sandbox conflicts with
  launching a speech server, finding Chrome and scanning the home folder for books; it needs
  its own investigation and is not planned here.

### Windows and macOS

An audit to do before promising either, one issue each:

| Area | What to check |
|------|---------------|
| Media keys | `souvlaki` is already the path off Linux (`src-tauri/Cargo.toml`); it has never been run. |
| Paths | `core/environment.rs` and `Paths`: config, data and cache folders; `~/Documents/Aoide`. |
| Launch commands | Starting and stopping a speech server (`core/engine.rs`): process groups and signals are Unix-shaped. |
| The browser | Finding Chrome (`core/browsing.rs`), Chrome for Testing pins per platform (`core/chrome-for-testing.toml`). |
| Audio output | `crates/audio/src/output.rs` and the output commands in `core/output.rs`. |
| File watching | `notify` semantics differ; `core/watching.rs` filters inotify's open events. |
| ffmpeg | Not present by default; bundle it or detect and explain. |
| Signing | macOS notarisation needs a paid developer account; unsigned Windows builds warn. |
| Speech engines | Most local TTS servers assume Linux and NVIDIA. On macOS, Kokoro on the processor or an MLX server is the realistic route. |

### Slices (Linux)

1. Bundle configuration; a package installs and runs on a clean Fedora, Debian and Arch
   container (by hand or in CI).
2. Missing `ffmpeg` is detected and explained when exporting.
3. The workflow.
4. README: download first, build from source second.

### Done when

- A person can install Aoide without a compiler.

---

## 6.6 Listening on a phone

**Size:** a direction, not a plan. Each option needs its own design before any code.

M4B export already takes a book to a phone. What is missing is doing so without effort and
coming back to the same place. Three routes, cheapest first:

**A. Export without asking.** A setting: "When a book is prepared, write its M4B to this
folder." Pointed at a folder a sync tool or a phone app watches, prepared books appear on the
phone. Small (it is `core/export.rs` triggered by preparation's final transition). Places do not
come back.

**B. Send to a server you already run.** People who self-host their audiobooks (Audiobookshelf
is the common one) want the M4B uploaded there, and could have their place read back through
that server's API. Medium. It is a network feature to a server on the home network, so it
meets the same reach question as OPDS (4.6).

**C. A player served by Aoide on the home network.** The phone's browser opens a page served
by the desktop: the same read-along view, audio streamed from the cache, the place shared.
Large, and a real change of posture: Aoide would listen on the network for the first time. It
would need pairing, a clear on and off, and a review of everything the page can ask for.

Recommendation: build A when someone asks for it; treat C as a separate project with its own
security design.
