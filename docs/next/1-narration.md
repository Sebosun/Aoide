# 1. Narration: what the voice says and how

Read [plan.md](plan.md) first.

Checked against the code on 2026-11-16: the "Check" items are answered in place, and the open
questions the user decided are under **Settled**.

What these five features share: `aoide_text::Segment` (`crates/segmenter/src/lib.rs`) already
separates `display` (what the page shows) from `speech` (what is sent to the engine) and carries
`pause_after_ms`. `Generation::speech_plan` (`src-tauri/src/core/generation.rs:148`) turns each
segment's `speech` into a `SynthesisRequest` through `Voicing::request` and addresses its audio
by `aoide_tts::cache_key`. Almost everything below is a change to what goes into that request.

---

## 1.1 Say one sentence again

**Size:** S
**Built:** 2026-11-16 (640b03f..e9c7c04)

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
- `aoide_generation::plan` (`crates/generation/src/lib.rs`) makes speech **from the playhead
  forward** only. The sentence just heard is behind the playhead, so forgetting its key alone
  would never get it made again.
- The player holds audio for up to `PROVIDE_AHEAD` (12) entries ahead and `KEEP_BEHIND` (2)
  behind (`core/session.rs`). `PlayerCore::release(index)` (`crates/audio/src/player.rs`)
  drops one entry's audio; `Session` tracks what it gave the player in `provided`.
- `segments.key` is the primary key, with one `book_id`: two books saying the same text with
  the same voice already share one row.

### Design

**Rule (core).** A sentence can be said again when it is spoken by an engine: not a recording
clip, not a silent entry, not a sentence the recording lacks. The chapter view's `SegmentView`
(`src-tauri/src/book.rs`) gains `can_say_again: bool`, so the interface never works this out.
It does not depend on whether audio exists yet: the chapter view does not change as audio is
made, and saying again a sentence with no audio simply plays it.

**Say this again = throw the audio away and play from that sentence.** Decided 2026-11-16.

1. *Make it again.* Forget the key, await `Forgotten`, delete the segment row and file, report
   the loss to the session (`audio_lost`), drop the entry from `Session::provided` and release
   its audio in the player, then move the playhead to the start of that sentence, keeping play
   or pause, as clicking a sentence does. The sentence is now at the playhead, so the normal
   plan makes it first; no new job reason is needed. Playback waits on it (the player stalls on
   a speakable entry without audio) and plays the new take when it arrives.
2. *Make it differently.* An engine that is deterministic for a fixed seed returns the same
   audio. Breeze, run through the OpenAI-compatible adapter, has no `seed` parameter and its
   server fixes the seed at 42, so step 1 alone would give back the same audio. Store a **take**
   per sentence:

   ```
   CREATE TABLE segment_takes (
     book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
     segment_id TEXT NOT NULL,
     speech_hash TEXT NOT NULL,     -- of Segment::speech; a take for other text is ignored
     take INTEGER NOT NULL CHECK(take > 0),
     PRIMARY KEY (book_id, segment_id)
   );
   ```

   The take goes in the request, not in the parameters: `SynthesisRequest` gains `take: u32`,
   and `cache_key` includes it only when it is above 0, so every key made so far stays the same.
   (Adding a `seed` parameter to the OpenAI-compatible descriptor would not do: `resolve_params`
   fills in defaults, so a new parameter changes every key of that engine and loses all its
   audio.) `Voicing::request` takes the take. Each adapter decides what a take means:

   - Chatterbox: the seed sent is the profile's seed plus the take.
   - OpenAI-compatible: with a take above 0, the body carries `"seed": take`. Servers that
     honour it vary; ones that ignore it and sample give a different result anyway.
   - `tools/breeze-tts/aoide_breeze_tts.py` reads an optional `seed` from the body instead of
     always using `SEED`.

**Component.** The table access is `Library::count_take` and `Library::takes`
(`crates/library/src/takes.rs`, which hashes the speech itself); `Generation::speech_plan` asks
each sentence for its take. `core/takes.rs` holds `Core::say_again`, which wires the library,
the worker, storage and playback as `regenerate_book` does (the session's entry is retaken with
`narration::Event::Retaken`). Host needs: none beyond the library.

**IPC.** `say_again(book_id, segment_id) -> ()`. The existing generation and player events
report the rest. Saying again in a book other than the one being listened to starts listening
to it at that sentence, as `play_from` does.

**Interface.** In the sentence's context menu (3.1 builds the menu; until then, a button in
`ReaderTools.vue` acting on the current sentence). The reader has no "being made" state of its
own ahead of the playhead (the player says *Buffering…*), so the button itself is busy (a
spinner, disabled) while the call runs and while the generation view's `generatingSegmentId` is
the sentence. Clicking a sentence plays it; Say this again keeps play or pause instead, as
decided above.

### Slices

1. The take in the request: `cache_key` is unchanged for take 0 and differs for take 1;
   Chatterbox sends seed plus take; the OpenAI-compatible body carries `seed` only for a take
   above 0. Seam: `cargo test -p aoide-tts`, each adapter's loopback tests.
2. The `segment_takes` table: a take is counted up per sentence; one whose `speech_hash` no
   longer matches is ignored. Seam: library test.
3. `say_again` on a sentence behind the playhead: its row and file are gone, no other row is
   touched, the playhead is on it, and the scripted engine receives one request, with take 1.
   Seam: core test with the scripted engine (see `core/tests.rs`).
4. On the sentence playing now and on one the player already holds ahead: its old audio is
   released and the new take is what plays. Seam: core test.
5. `can_say_again` on `SegmentView`: false for recording clips, silent entries and sentences
   the recording lacks. Fixture and mock scenario.
6. The Breeze script accepts `seed`.
7. Interface: a **Say this again** button in `ReaderTools.vue` acting on the current sentence
   (3.1 later adds it to the menu on selected text), its busy state, preview screenshot.

### Logging

`speech.say_again` (`book`, `segment`, `take`, `outcome`).

### Done when

- A garbled sentence can be replaced without touching any other audio.
- With Chatterbox's seed set to a fixed value, and with Breeze, the replacement differs from the
  original.
- A book's `prepared` mark and its M4B export still hold after a sentence is replaced.

### Settled

- The same text with the same voice in two books shares a key; saying it again in one book
  deletes that audio, so the other book makes its own again when it needs it. Acceptable: it is
  rare.
- No undo: the old file is deleted.

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
- Word timings. No engine adapter returns any (`word_timings: false` in every descriptor). An
  entry's timings come from one of two places:
  - `aoide_text::align_to_pauses`, fitted to the *written* words of the segment
    (`Session::provided`).
  - The optional local aligner (`align_speech` in `core/align.rs`), which aligns the written
    words to the made audio and stores the result with it.

  `Session::provided` drops stored timings whose count differs from `Segment::words` and falls
  back to fitting pauses, so a mismatch makes the highlight rougher, not broken.

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
-- SQLite treats NULLs as distinct in UNIQUE, so the list for all books needs its own index:
CREATE UNIQUE INDEX pronunciations_all_books ON pronunciations(written) WHERE book_id IS NULL;
```

Both lists live in the database (decided 2026-11-16). Keeping them safe and moving them to
another computer is 4.8's job.

**Where it is applied.** `Voicing` gains `rules: Arc<[Rule]>` (book rules then global), and
`Voicing::request` runs `pronounce` on the segment's speech. The cache key then differs for the
sentences a rule touches and for no others. Voice preview and export go through the same
request, so they follow without more work. A paired recording is not affected.

**Timings.** Only the aligner hears the difference: give `align_speech` the words of the
*spoken* text and fold its result with `word_of` before it is stored, so it is again one timing
per word of `Segment::words`. Fitting to pauses works on the written words and stays as it is.
If an adapter ever returns its own timings, they are folded the same way.

**Component.** `core/pronunciation.rs` (model: `core/bookmarks.rs`): list, add, change, remove,
and "what does this change": `affected(book_id) -> AffectedView { sentences, cached }`, so the
interface can say "12 sentences will be made again" before saving.

**After a change.** The plan of the book being listened to is rebuilt, the way saving a book's
voice settings does it (`core/shelf.rs`: `restart_session`, then
`PlanEvent::BookSettingsChanged`). A rule for all books does this for the book being listened
to and drops the plans of queued books so they are made again. A prepared
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
4. Timings folded: with a rule turning one word into three, the aligner is given the spoken
   words and the stored timings have one per written word. Seam: the align component's test
   with a scripted aligner.
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

### Settled

- The list for all books is in the database, like the per-book lists.
- Plain respelling only, no raw engine markup such as Kokoro's `[word](/phonemes/)`: it would
  not carry over to another engine.

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
- The player passes over an entry that is not speakable and has no pause at once
  (`PlayerCore::pull`, `crates/audio/src/player.rs`); that is how `Fate::Unrecorded` entries
  are skipped today. The generation planner never makes an unspeakable item.
- `BookSettings` (`core/wire.rs`) is more than the voice: it already holds `read_notes`,
  `narration`, `speak_missing` and `mode`, outlives the book, and changing it re-plans
  (`save_book_settings` in `core/shelf.rs`).

### Design

**Data.** `BookSettings` gains `left_out: Vec<usize>` (`#[serde(default)]`), the chapter
indexes not narrated. It outlives the book being removed and added again like the rest of the
book's settings, and needs no new library value or migration.

**Rule (core), in a new `core/leaving_out.rs`.**

- Entries of a left-out chapter get a new `Fate::LeftOut`: unspeakable with no pause, so the
  player passes over them and the planner never makes them. Playback that reaches a left-out
  chapter therefore continues at the next chapter that is not left out; at the end of the book
  it finishes as a book does.
- Starting playback *inside* a left-out chapter (the user clicked a sentence there) plays it:
  an explicit request wins. Seeking into a left-out chapter sends a new `Event::Admitted` to
  its entries, turning them back into what they would otherwise be, for the rest of the
  session. Leaving out only applies to arriving from the chapter before. A table-driven test
  covers `next` with the two new cases.
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

**IPC.** `set_left_out(book_id, chapters: Vec<usize>) -> BookView`, which saves the field
through the same path as the other book settings.

**Interface.** A checkbox per chapter in `BookContents.vue` while its "choose what is narrated"
mode is on; left-out chapters are dimmed with a small "not narrated" label. In the reader's
chapter menu they carry the same label.

### Slices

1. `front_and_back_matter`: English titles, case and punctuation, nothing proposed for a novel
   whose chapters are all numbered. Seam: `cargo test -p aoide-text`.
2. The setting is stored, listed in `ChapterSummary`, and kept across remove and add.
3. `Fate::LeftOut` and `Event::Admitted` in `next`: table-driven test in `core/narration.rs`.
4. Playback arrives at a left-out chapter and continues after it; the last chapter left out
   ends the book. Seam: core test with the scripted engine (see how `tests.rs` drives chapter
   ends).
5. Clicking a sentence in a left-out chapter plays it.
6. Preparation and its progress count leave the chapters out; the `prepared` mark is reached.
7. Export leaves them out, with chapter markers still right.
8. Recording: the same for a paired audiobook.
9. Interface and mock scenario.

### Logging

`narration.left_out` (`book`, `chapters` as a count), `playback.skipped_left_out` (`book`,
`from`, `to`).

### Done when

- A book with front matter starts speaking at chapter one after one click.
- No audio is made for a left-out chapter unless the user asks for that chapter.

### Settled

- M4B export has no "export everything" switch: it follows the book's choice.
- Per-book only, with the suggestion one click away; no global setting.

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
- Every entry's `pause_after_ms`, spoken or silent, reaches the player through
  `NarrationPlan::player_entries`. The M4B export writes the same silence after each entry and
  counts it in its chapter marks (`core/export.rs`). Both read the segments, so new pauses
  reach them with no change there.
- `<hr>` is a void element that both parsers drop (`crates/epub-parser/src/xhtml.rs`,
  `crates/article/src/scan.rs`). A paragraph with no letter or digit, such as `* * *`, already
  becomes a segment with empty speech.
- Book models are built at load, never stored: `BookModel::from_parsed` (`src-tauri/src/book.rs`)
  segments with `SegmentOptions::default()`, and `Shelf::load` (`core/shelf.rs`) keeps them in
  memory until `Shelf::unload`. The per-book `read_notes` setting already reaches
  `from_parsed` this way. A new field on `Block` needs no migration.

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
and 700, each 0–3000. They reach `from_parsed` as `SegmentOptions`, the way `read_notes`
does. Changing them unloads every book model the shelf holds, restarts the session of the book
being listened to and re-plans. Settings ›
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
4. Export writes the same silences: a test only, since export already reads `pause_after_ms`.
5. Interface.

### Logging

None new (a settings change is already logged).

### Done when

- Paragraphs are audibly separated at the default settings.
- Changing a pause makes no audio again.

### Settled

- Pauses are global only, not per book (plan's default).

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
they carry `char_offset`, and this is already in place:

- Resuming playback finds the place by chapter and `char_offset` (`index_at` in `play_from`,
  `src-tauri/src/core.rs`).
- Bookmarks try the `segment_id`, then `index_at` (`core/bookmarks.rs`).
- A recording's sync is stored by character offsets (`crates/sync`).
- The other `index_of` callers (`play_from`, `set_reading_place`) take ids from the chapter
  view on screen, which is built from the current segments.

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

1. A regression test that stored places survive new segment ids: re-segment a book with
   different options, reopen it, and check that progress and a bookmark land on the same text.
   Seam: core test.
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

### Settled

- Inner thoughts in italics get no third role (plan's default).
- The short-span threshold is a constant, not a setting (plan's default).
