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
