# Phase 2: reading without listening

Read [plan.md](plan.md) first. Mockups: `design/ReaderText.dc.html` (shape `Read`),
`design/ReaderAlong.dc.html` (shape `ReadAlong`).

## Outcome

A book can be read from start to finish with no audio and no speech engine. Listening is a
choice the user makes per book and can change at any time, also for a book that has a
recording. Aoide remembers where you are, counts reading time, and knows when you finished.

## What exists (start here)

- Progress is written only by playback: `Core::save_progress` in `src-tauri/src/core.rs` reads
  the playhead (`playback.place_to_save`) and stores a `Progress` plus a
  `progress.fraction.<id>` setting.
- `BookSettings` (per book, in the library database) holds `narration: Option<Narration>`.
- `src/stores/reader.ts` opens a book and loads chapters; `ReaderView.vue` always renders the
  player dock; `src/composables/useFollow.ts` scrolls to the playhead.
- Listening time: `Library::record_listening(book, day, ms)`, table `listening`, written from
  `core/extras.rs`; shown by `WeekCard` and the Activity view.
- Time-left estimates are computed in the UI (`src/estimate.ts`) from words and speed.

## Data (library v7)

- `books.finished_at INTEGER` (null = not finished).
- `books.word_count INTEGER` (null until the item has been parsed once; filled by
  `Core::load_book` and at import).
- `CREATE TABLE reading (book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE, day TEXT
  NOT NULL, ms INTEGER NOT NULL CHECK(ms >= 0), PRIMARY KEY (book_id, day))`.
- `Library::record_reading`, `Library::activity_stats` (per day: reading ms and listening ms;
  per book: both). `listening_stats` stays until its callers have moved, then goes.

## State: mode and shape (`src-tauri/src/core/reading.rs`, new)

```
enum Mode  { Read, Listen }
enum Shape { Read, ReadAlong, Listen }
enum ModeEvent { ListenChosen, ReadChosen }
fn next(mode: Mode, event: ModeEvent) -> Mode
fn default_mode(has_recording, has_cached_speech, has_listening_history) -> Mode
fn shape(has_text: bool, mode: Mode) -> Shape   // no text => Listen (phase 3)
```

- `BookSettings` gains `mode: Option<Mode>`; `None` means "use `default_mode`". It is written
  only when the user chooses.
- `default_mode` is `Listen` when any of its three inputs is true, else `Read`. So books the
  user already listens to open as they do today, and a new book opens silent.
- `BookView` gains `mode: Mode` and `shape: Shape`. `BookSummary` gains `read_state` and
  `word_count`.
- Command `set_mode(id, mode)`. Choosing `Read` for the book that is playing pauses it first.
  Choosing `Listen` does not start playback by itself; the UI calls `play` next.
- Starting playback of a book by any route (`play`, media key, item page Listen) sets its mode
  to `Listen`.

## State: read state (`core/reading.rs`)

```
enum ReadState { Unread, InProgress, Finished }
enum ReadEvent { Progressed { at_end: bool }, MarkedFinished, MarkedUnread }
fn next(state: ReadState, event: ReadEvent) -> ReadState
```

- `Unread`: no progress row and no `finished_at`. `InProgress`: a progress row, no
  `finished_at`. `Finished`: `finished_at` set.
- `Progressed { at_end: true }` (the last segment of the last chapter was reached by reading
  or playback) finishes the item. Progress after `Finished` keeps it finished.
- `MarkedUnread` clears `finished_at` and deletes the progress row.
- Commands `mark_finished(id)` and `mark_unread(id)`.

## Reading position

- Command `set_reading_place(id, chapter, segment_id)`. The core resolves the segment, writes
  the same `Progress` row and fraction setting playback writes, and applies `ReadEvent`.
- The core ignores the call while that item is the one playing: playback owns the position
  then.
- Extract the shared "store this place" code out of `Core::save_progress` so both callers use
  one function.
- UI: the reader reports the first segment whose top edge is inside the viewport, at most once
  a second and only when it changed. A new composable `src/composables/useReadingPlace.ts`.
- On open in shape `Read`, the reader scrolls to the saved segment (today it relies on the
  player to bring the place into view).

## Reading time

- Command `reading_activity(id)`. The UI sends it at most every 15 s while all hold: the reader
  is open on that item, shape is `Read`, the window is focused, and there was a scroll, key or
  pointer event in the last 60 s.
- The core credits the time since the previous call for the same item, capped at 30 s, to
  `reading` for today. The first call of a session credits nothing. Rule and cap live in
  `core/reading.rs` and are tested with a supplied clock.
- Time spent in `ReadAlong` counts as listening only, as today.
- `AppSettings.reading_wpm: u32` (default 230, range 100–600), in Settings under Appearance or
  a new Reading tab. `src/estimate.ts` gains a reading estimate from `word_count`, fraction and
  `reading_wpm`.

## Slices

### 2.1 Mode and shape in the core
Test (`core/tests.rs`): a fresh book reports `mode: Read, shape: Read`; a book with cached
speech reports `Listen, ReadAlong`; `set_mode` round-trips and survives a restart; `play` sets
`Listen`; `set_mode(Read)` on the playing book pauses it. Plus the transition table for `next`.
Done: contract regenerated; mock returns `mode` and `shape` for each sample book.

### 2.2 The reader renders the shape
- `ReaderView` switches on `book.shape`: `Read` renders the sheet with no dock and a footer
  with a progress line, percent and reading time left (`ReaderText.dc.html`); `ReadAlong`
  renders today's layout (`ReaderAlong.dc.html`).
- Header control: in `Read`, a "Listen" button; in `ReadAlong`, the same button pressed,
  labelled "Listening". Pressing it calls `set_mode` and, when turning listening on, `play`
  from the current reading place.
- With no usable engine and no recording, "Listen" opens the Audio card's choices (narrate /
  pair) in a menu instead of failing.
- Page/Lines switch shows only in `ReadAlong`. Lines mode with shape `Read` falls back to Page.
- Clicking a sentence plays from it in `ReadAlong` and does nothing in `Read`.
- Another item may keep playing while this one is read silently; the reader shows no dock for
  it. Pressing Listen here stops the other item first (existing single-player behaviour).
Test (`reader.test.ts`): each shape renders its layout; the button moves between them and
calls the right commands; Lines is unavailable in `Read`.
Done: the reader contains no condition on player status to decide its layout.

### 2.3 Reading place
Test (core): `set_reading_place` stores progress and fraction; it is ignored for the playing
item; reopening the book returns that place. Test (UI, `reader.test.ts`): scrolling reports the
top segment once; opening scrolls to the saved segment; switching to Listen starts from it.
Done: a book read silently shows its percent on Home and in the Library.

### 2.4 Read state
Test (core): the transition table; reaching the last segment by `set_reading_place` and by
playback finishing both set `finished_at`; `mark_unread` clears progress. Test (UI): Library
chips Unread, In progress and Finished filter by `read_state`; the item page and the card
context menu offer "Mark as finished" / "Mark as unread".
Done: Home's Continue list excludes finished items (update `Core::home`).

### 2.5 Reading time
Test (core, supplied clock): two calls 10 s apart credit 10 s; 5 min apart credit 30 s; the
first credits 0; a day boundary splits correctly. Test (UI): the composable sends activity only
under the four conditions.
Done: `activity_stats` returns reading and listening per day.

### 2.6 Activity and Home show both
- `WeekCard` and `ActivityView` show reading and listening as two series (stacked bars, two
  totals), as in `Main.dc.html`. The two series differ in lightness, not only hue.
- Home Continue cards and Library cards show reading time left for shape `Read` and listening
  time left otherwise.
- Item page Text card shows "About N min to read"; Read is the primary action when mode is
  `Read`, Listen when it is `Listen`.
Test (`stats.test.ts`, `home.test.ts`): totals and bars for a mock week with both series.
Done: `listening_stats` and its command are removed if nothing calls them.

### 2.7 Books with no audio path
- A book with no engine configured and no recording opens and reads with no error state, no
  engine banner and no buffering indicator.
- Engine errors surface only after the user asks to listen.
Test (core + UI): start the core with an offline engine; open, page through and finish a book;
no `engine://status` error reaches a visible component.
Done: the smoke home with no speech engine can be used as a plain e-reader.

## Done when

- All slices committed, all checks pass.
- In the native app: import a book, read two chapters silently, close, reopen at the same
  sentence; press Listen and hear it continue from there; press it again and keep reading.
- Activity shows today's reading minutes after a few minutes of reading.
- Books that had a recording or cached speech before the upgrade open in read-along as before.

## Notes from implementation

- The library was already at v7 (`book_values`), so this phase's schema is **v8**; later
  phases shift by one (phase 3 v9, 4 v10, 5 v11, 6 v12).
- The mode enum is `ReadingMode` in Rust and TypeScript: the contract names types by their Rust
  name, and `aoide_config::Mode` (exported as `AppearanceMode`) already took `Mode`.
- The core parts of 2.1, 2.3, 2.4 and 2.5 landed in one commit (they share `core/reading.rs`);
  the UI slices follow one commit each.
- A mode choice is stored with `Shelf::save_mode`, not `save_settings`, which would drop the
  parsed book and restart a session.
- Playback saves its place at once when the playhead reaches the book's last entry (otherwise
  every 3 s), so playing to the end finishes the book.
- `books.word_count` is filled when a book is parsed (`Shelf::load`), not at import.
- The engine status gained `wanted` (the user listened, prepared a book or tried a voice in this
  run); the sidebar shows engine failures only once it is set. This is how "engine errors
  surface only after the user asks to listen" is decided in the core.
- Read on the item page also chooses the Read mode, and the end of each chapter offers the next
  while reading silently (otherwise a book could not be read through without the chapter menu).
- Restoring the saved place waits for the reading font, and the scroll it makes is never
  reported: a place near a chapter's end lands at the bottom, which the "scrolled to the end"
  rule would otherwise take for having read the chapter (found in the native check).
- `listening_stats` (command and views) is gone; `Library::listening_stats` stays, since the
  shelf uses it for each book's listened flag.
- Native check (synthetic book, no engine, then the local Chatterbox): read two chapters, the
  place survived a cold restart, Listen with no engine offered narrate / pair / settings, with
  the engine it played on from the reading place, and Listening turned it back into silent
  reading. The smoke window had no focus, so the window's reading-time sender stayed quiet as
  designed; Activity's reading time came from `reading_activity` called directly.
