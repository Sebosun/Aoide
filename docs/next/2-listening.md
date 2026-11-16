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
