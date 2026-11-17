# 2. Listening: the player

Read [plan.md](plan.md) first.

Checked against the code on 2026-11-16: the "Check" items are answered in place, and decided
questions are under **Settled**.

The player's state is `PlayerStateView` (`src-tauri/src/core/wire.rs:110`), built by
`Playback::state` in `core/playback.rs` (`entries_state` for a book, `timeline_state` for an
audiobook alone); `Core::player_state` adds the sleep timer. What sits around playback (the
sleep timer, media keys) is `core/controls.rs`. The tick loop that drives all of it is
`Core::tick_loop` in `src-tauri/src/core.rs`: every 33 ms (`TICK`) while something plays or a
sleep timer runs, else once a second. It builds the state on each tick and emits it only when
it differs from the last one sent (`Controls::observe`). The interface side is
`src/composables/usePlayerControls.ts`, `src/stores/player.ts` and `src/components/player/`.

---

## 2.1 Step back a little on resume

**Size:** S
**Built:** 2026-11-16 (1652335..f42029c)

### Outcome

Come back after a break and playback starts a little earlier, so you catch the thread: from
the start of the sentence after a short pause, a sentence or two earlier after a long one.

### What exists

- `pause`, `resume` and `play` in `commands/playback.rs` call `Core::pause`, `Core::resume`
  and `Core::play`; the media keys call `Core::resume`, `Core::pause` and `Core::toggle`
  (`src-tauri/src/media/mod.rs`). `Playback` (`core/playback.rs`) owns the player. Every move
  of the playhead returns a `Moved` (place to save, what to tell Generation, what the output
  should do), and `Core::follow_through` (`core/playback.rs:846`) is the one place that acts on
  it. A `Moved` carries no cause: the cause of a step back is said in its log line, not in the
  `Moved`.
- Three routes resume a paused book, all through `Playback::play()`: `Core::resume`,
  `Core::toggle`, and `Core::play` for the book already loaded with no chapter or segment
  (`core.rs:462`; the reader's play button calls `play(book, null, null)`). The same holds for
  an audiobook alone (`play_timeline`, `core/timeline.rs:249`).
- A start from the saved place (`Core::play` with neither chapter nor segment, for a book not
  loaded) resolves the saved `Progress` to an entry with `book.index_at(chapter, char_offset)`,
  so it already starts at a sentence's start. `Progress.updated_at`
  (`crates/library/src/lib.rs:132`, ms since the epoch) says when that place was saved, by
  listening or by reading (`Core::store_place`).
- A click on a sentence is `play` with a segment; a chapter skip is `play` with a chapter
  (`Core::skip_chapter`); a seek is `Playback::seek`. All three name the place, so none of
  them steps back.
- **Clock.** Nothing injects a clock today: `controls.rs`, `session.rs` and `playback.rs` call
  `Instant::now()` directly, and their tests do not fake time. This feature therefore keeps
  time out of the rule (it takes a `Duration`) and passes `now` into the one method that needs
  it.

### Design

**Pure rule, in a new `core/resuming.rs`.**

```
pub struct Away(pub Duration);
pub enum StepBack { None, SentenceStart, Sentences(u8) }     // speech and synced recordings
pub fn step_back(away: Away) -> StepBack
pub fn step_back_ms(away: Away) -> u64                        // an audiobook with no text
```

| Away | With text | Audiobook alone |
|------|-----------|-----------------|
| under 10 s | nothing | nothing |
| 10 s to 5 min | start of the current sentence | 3 s |
| 5 min to 1 h | one sentence earlier | 10 s |
| over 1 h | two sentences earlier | 20 s |

**How long the listener was away.**

- *Resuming a paused session:* the time since the pause. `Playback` keeps
  `paused_at: Option<Instant>`, set by `Playback::pause`, cleared by `seek`, `start`,
  `start_timeline` and `stop`. So a seek while paused (or a click on a sentence) means no step
  back. The two places that stop and start the session again in place (`restart_session` and
  `text_laid_out_again` in `core.rs`) keep a paused session paused since its pause
  (`Playback::paused_since`, `Playback::pause_since`).
- *Starting from the saved place* (the first play after starting Aoide, or coming back to a
  book): the time since `Progress.updated_at`. This replaces the earlier idea of a "cold start"
  flag: a restart two minutes after a crash then steps back as little as a two-minute pause,
  and a place the user just set by reading or by `seek_to` on a stopped audiobook is not moved.

**Where it applies.** One method, `Playback::resume_at(now: Instant) -> Moved` (the tests pass
`now = paused_at + 10 min`), used by the three resume routes above in place of
`Playback::play()`, and one step in `Core::play` / `play_timeline` before `start` /
`start_timeline` when they start from the saved place.

- *Sentences* are counted over spoken entries (the narration's `Fate::Speech` and
  `Fate::Clip`); silent and unrecorded entries are passed over. The start of the target entry
  is `Playback::seek(index, None)`, which for a recording is just before its first word
  (`RecordingPlan::start_offset`).
- It never crosses the start of the chapter (`book.locate(index)` and the chapter's
  `first_entry`), nor, on a timeline, the start of the part (`Timeline::parts`).
- An audiobook alone steps back on its timeline with `Playback::seek_time`.
- The step back is an ordinary seek, so its `Moved` (place saved, `Listening::Jumped`) goes
  through `follow_through` like any other. Entries behind the playhead whose audio the player
  released (it keeps `KEEP_BEHIND` = 2) are read again from the cache on the next tick.

**Setting.** `AppSettings.step_back_on_resume: bool` (`core/settings.rs`, with its
`AppSettingsPatch` field), on by default. There is no playback section in Settings; it goes in
Settings › Library (`src/components/settings/LibrarySettings.vue`), as a Listening section.
`Core::resume` and the two starts from the saved place read it (`Core::steps_back`); off, they
play as before.

### Slices

1. `step_back` and `step_back_ms`: the table above as a table-driven test, boundaries included.
2. `Playback::resume_at` after a scripted 10 minutes starts one sentence earlier, with the place
   saved. Seam: `Playback` test in `core/playback.rs` with the four-entry test book.
3. Not across a chapter start; nothing after a seek while paused; silent entries are not
   counted.
4. A start from the saved place uses `Progress.updated_at`. Seam: core test in `core/tests.rs`
   with a stored progress row two hours old.
5. An audiobook alone steps back in milliseconds, not across a part start. Seam: the timeline
   tests in `core/timeline.rs`.
6. The setting; off means nothing moves. Contract regenerated.

### Logging

A new event `playback.resumed` (`book`, `away_s`, `stepped_back` with its `unit`: `entries`,
0 being the sentence's own start, or `ms`; `from` = `pause` or `saved`), one line per resume
that steps back. New row in `docs/logging.md`.

### Done when

- Resuming after lunch replays the last sentence or two.
- Pause and play in quick succession changes nothing.

### Settled

- One switch; the thresholds are not settings. (plan's default)

---

## 2.2 Time left in the book

**Size:** S
**Built:** 2026-11-16 (3ad0853..5bfcc1d)

### Outcome

The player shows time left in the book as well as in the chapter, at the current speed, and
when the book would end ("ends 23:40"). Click the time to switch between chapter and book.

### What exists

- `chapter_left_ms` in `PlayerStateView`: computed in `Playback::entries_state` only for a
  synced recording (`recording.and_then(...)`, `core/playback.rs`), and in `timeline_state` for
  an audiobook alone; `None` for speech, where `timeLeftMs` in `usePlayerControls.ts:44`
  estimates it from the words left (`speechMs`, `wordsLeft` in `src/estimate.ts`).
- `NarrationPlan::playing_ms(range)` (`core/narration.rs:270`) sums the durations of a range of
  entries, pauses included: exact where audio is made or a clip, estimated (`estimate_ms`,
  about 15 characters a second) where speech is not made yet, `None` while part of the range
  waits for the sync. It already works for speech: the recording-only gate in `entries_state`
  is the only reason speech gets `None`.
- For an audiobook alone, `position_ms` and `duration_ms` are in the view.
- The buffered-ahead bar (`ahead` in `usePlayerControls.ts`) also estimates the chapter's
  length from its words, to divide `GenerationStateView.buffered_ahead_ms` by.
- The interface stores no view preference in local storage; view preferences live in
  `Appearance` (`crates/config/src/lib.rs:206`, `[appearance]` in `settings.toml`).
- The mock plays playback itself: it computes `chapterLeftMs` for an audiobook
  (`src/api/mock.ts:793`) and sends `null` for speech (`src/api/mock.ts:1108`).

### Design

**The core decides.** `PlayerStateView` gains `book_left_ms: Option<u64>`: from the playhead to
the end, at the current speed, leaving out chapters left out by 1.3 (`Fate::LeftOut` counts 0
in `playing_ms` once 1.3 exists).

- Speech and synced recordings: `playing_ms(index..total_entries)` minus the playhead's offset
  into its entry, through `at_speed`. `None` while part of the rest awaits the sync; the
  interface then shows the chapter only.
- Audiobook alone: `total_ms - position_ms`, at speed.

**Cost.** The state is built on every tick (30 a second) while playing. Summing a long book's
entries each time is a few microseconds per thousand entries; start with the plain sum and add
a suffix-sum array (rebuilt in `segment_made`, `audio_lost` and `follow`) only if a test on a
20 000-entry book shows it matters.

**Emitted once a second, not 30 times.** `observe` emits when the state differs. A figure that
falls by the millisecond would emit every tick (a synced recording's `chapter_left_ms` already
does). Round both `chapter_left_ms` and `book_left_ms` down to whole seconds in the view.

While here, move the chapter figure for speech into the core as well: drop the recording-only
gate so `chapter_left_ms` is `Some` for speech, and delete the estimate in
`usePlayerControls.ts`. For the buffered-ahead bar, the view gains `chapter_ms: Option<u64>`
(the whole chapter's playing length at 1×), so `ahead` stops estimating too. The figures move
a little: the core estimates from characters, the interface from words.

**Interface.** `PlayerDock`, `PlayerTransport` and `MiniPlayer` show `timeLeft` from
`usePlayerControls`; the label toggles chapter and book. The choice is
`Appearance.time_left: TimeLeft { Chapter, Book }` (default `Chapter`). The tooltip gives the
end time by the wall clock. As built: the scrubber's end label is a button in the dock and
the Lines transport ("3 min left", "3 h 10 min left in book"); the mini player shows the
chosen figure but its card opens the reader, so it does not switch. The tooltip reads
"Chapter ends 21:05 · book ends 23:40 · click for the time left in the book" (`endsAt` in
`src/estimate.ts`, with the weekday when it is not today). The audiobook reader's "left" line
uses `book_left_ms` while it plays. The reader's margin still estimates a chapter that is open
but not playing, from its words.

**Mock.** The mock's player sends `bookLeftMs` and `chapterLeftMs` from the durations in its
fixtures (`player.json` gains the core-made figures for its scripted positions) rather than
estimating from words. As built: `player.json` is `{stopped, at}`, where `at` holds the core's
three figures at 1x at the start of each sentence of each preview book read by speech (before
any speech is made, so the file is the same on every run); the mock divides by the speed and
rounds down to whole seconds as the core does. `mock.emitPlayer` (for tests) sends a state as
given.

### Slices

1. `book_left_ms` for speech: cached and estimated entries, speed 1.5. Seam: `Playback` test in
   `core/playback.rs` (`speech(&[..])` builds the target).
2. For a paired recording (`None` while awaiting the sync) and for an audiobook alone.
3. Both figures in whole seconds; `chapter_left_ms` and `chapter_ms` for speech from the core;
   the estimates in the composable deleted; fixtures and contract regenerated.
4. `Appearance.time_left`, the interface toggle and the end time.

### Logging

None.

### Done when

- The figure falls steadily while playing and changes at once when the speed changes.
- The interface holds no listening-time arithmetic for the playing book.

---

## 2.3 A speed for each book

**Size:** S
**Built:** 2026-11-17 (d3737fd..e6933dc)

### Outcome

A book can keep its own playback speed. A slow narrator stays at 1.4×, poetry at 0.9×, and
everything else follows the usual speed.

### What exists

- Speed is one value in `settings.toml` (`[player]`, `PlayerSettings` at `core/config.rs:24`),
  read at startup (`core.rs:264`) and applied by `Playback::set_speed`; `PlayerCore::set_speed`
  clamps it to 0.5–3.0 (`crates/audio/src/player.rs:168`).
- `Core::set_speed` and `Core::set_volume` both call `save_player_settings`, which writes the
  **player's current** speed and volume to `[player]` (`core/playback.rs:917`).
- A hand edit of `[player]` reaches `settings_edited` (`core/config.rs:400`), which sets the
  player's speed directly.
- `BookSettings` (`core/wire.rs:203`) holds per-book overrides, stored as
  `BookKey::Settings` in the `settings` table, so it outlives the book. Its `params` may contain
  Chatterbox's `speed_factor` (`crates/chatterbox-client/src/engine.rs:41`): the engine's
  speaking rate, part of the request and the cache key. That is a different thing: name the new
  field `playback_speed`.
- `Core::save_book_settings` (`core/shelf.rs:869`) restarts the listening session and re-plans
  generation; `Shelf::prefer_recording` shows a narrower read-modify-write of one field.
- `SpeedPill.vue` is a `UiSelect` (`src/components/ui/UiSelect.vue`), a listbox of options
  only: it has no room for a switch.

### Design

- `BookSettings.playback_speed: Option<f32>` (`#[serde(default)]` is already on the struct).
- `Playback` keeps the usual speed (`usual: f32`) apart from the player's speed. When a session
  starts (`Core::play`, `play_timeline`), the core applies the book's speed if it has one, else
  the usual one.
- `set_speed(speed)` while a book with its own speed plays changes that book's speed (written
  with a narrow update of the one field, like `prefer_recording`, not through
  `save_book_settings`, which would restart the session); otherwise it changes the usual one.
  The command does not grow a flag: the core knows which applies.
- `save_player_settings` writes `usual`, not the player's speed, so changing the volume while a
  book's own speed plays does not make it the usual speed.
- A hand edit of `[player].speed` sets `usual`, and the player only when the playing book has
  no speed of its own.
- `PlayerStateView` gains `speed_scope: SpeedScope { Usual, Book }` so the pill can show it.
- `set_book_speed(book_id, on: bool)`: turning it on copies the current speed into the book;
  off removes it and returns to the usual speed.
- Speeds are kept to two decimals in the core, so 2.7's ±0.1 steps do not drift (1.4000001);
  `[player].speed` is written to two decimals too (an `f32` reached the file as
  `1.100000023841858`).
- `save_book_settings` keeps the stored `playback_speed`, as it keeps `left_out`: a settings
  form holding older settings cannot undo the book's speed.
- Stopping returns the player to the usual speed (nothing plays, so nothing has its own).

**Interface.** `SpeedPill.vue` becomes a small popover (`usePopover`, as `VolumeControl.vue`
does) with the speeds (a grid of buttons, `aria-pressed`) and a switch "Only for this book"
under them, and a small dot on the pill when the book has its own.

### Slices

1. A book with its own speed starts at it; the next book returns to the usual one. Seam: core
   test in `core/tests.rs`.
2. `set_speed` goes to the right place in each case; `[player].speed` in `settings.toml` is not
   touched by a book's speed, also after `set_volume`; a hand edit while a book's speed plays
   changes only the usual one.
3. `speed_scope` in the view; `set_book_speed`; fixture; contract; mock scenario.
4. The pill.

### Logging

A new event `playback.book_speed` (`book`, `on`), when a book's own speed is turned on or off.
New row in `docs/logging.md`.

### Done when

- Switching between two books switches speed with them.

---

## 2.4 A gentler sleep timer

**Size:** S (upper end)
**Built:** 2026-11-17 (5777942..17d33c8)

### Outcome

The sleep timer stops at the end of a sentence, fades out over its last seconds, and if you
are still awake, pressing play within a few minutes starts the same timer again.

### What exists

- `Controls` in `core/controls.rs`: a private `SleepTimer { At(Instant), EndOfChapter(usize) }`
  behind a mutex, `set_sleep`, `sleep_due(chapter)` and `sleep_view`. `Core::tick_loop` calls
  `sleep_due(state.chapter)` each tick and `Core::pause` when it is due, so a timed sleep cuts
  mid-word, and an end-of-chapter sleep fires on the tick after the next chapter has begun.
  The tick loop stays at 30 Hz while `state.sleep` is `Some`.
- **Checked:** `crates/audio/src/player.rs` has no gain apart from `volume`, which `render`
  multiplies in after the stretcher. A fade needs a new one.
- The player knows exactly when an entry ends: `pull` moves `index` on after the entry's audio
  and its `pause_after_ms`. The tick sees it up to 33 ms (plus output latency) later.
- The media keys and the play buttons reach `Core::resume`, `Core::toggle` and `Core::play`.

### Design

**An explicit state** in `core/controls.rs` (the timer becomes a small machine, as AGENTS.md
asks):

```
enum Sleep {
    Off,
    Running { until: Until, minutes: Option<u32> },      // Until::At(Instant) | EndOfChapter(usize)
    Fading { ends: Instant, minutes: u32 },              // last 20 s of a timed sleep
    Stopping { since: Instant, minutes: u32 },           // due; waiting for the sentence's end
    Rang { at: Instant, minutes: Option<u32>, end_of_chapter: bool },
}
enum SleepEvent { Set(..), Cancelled, Tick { now, chapter, playing }, Paused(Instant), Played { now, chapter } }
fn next(sleep: Sleep, event: SleepEvent) -> (Sleep, Option<SleepAction>)
enum SleepAction { Fade(f32), PauseAtEntryEnd, PauseAtChapterEnd(usize), Pause, RestoreGain }
```

- **The player stops at an entry boundary.** A new `PlayerCore::pause_before(index)`: `pull`
  stops and pauses when the playhead would enter `index`. It is sample-exact, and what the
  stretcher still holds then is the entry's pause, so no word is cut. A due timed sleep arms it
  for the next entry; an end-of-chapter sleep arms it at once for the next chapter's first
  entry, so it stops at the chapter's end rather than a tick into the next one.
- `Stopping` lasts at most 15 s; then `Pause` stops wherever it is.
- When the player has paused itself, the tick sees `Status::Paused` in `Stopping` and calls
  `Core::pause`, which saves the place as any pause does.
- **Fade.** A new `PlayerCore::set_fade(target)` with a per-sample ramp, multiplied in with
  `volume` in `render`; `volume` is not touched. During `Fading` the target falls to about 20%;
  the pause (and any cancel or play) restores it to 1.
- `Rang` lasts 3 minutes: `Played` in that time starts the same timer again. Its expiry is
  checked when `Played` arrives, so `Rang` needs no ticks and the tick loop does not stay at
  30 Hz for it.
- An audiobook alone has no sentences (its entries are 10 s stretches, `core/timeline.rs`): a
  timed sleep fades and pauses when due, and end of chapter keeps today's check on the part
  number.

`SleepView` gains `extended: bool`, shown by the pill ("again, 15 min"); fading is not shown.

(Built: `Running`, `Fading` and `Stopping` carry `extended` too, for the view. The pill keeps its
countdown and puts "again, " before it ("again, 14:59"), or says "again" by the moon for the end
of the chapter. A pause while fading or stopping counts as the ring, and play while fading starts
the timer again; a timed sleep that comes due while paused just ends. Ending the chapter is set up
again on each play in that chapter, since a new playlist drops the player's pause. The preview
reaches the state with `sleep=again|again-chapter`.)

### Slices

1. The transition table. Seam: `next` in `core/controls.rs`, table-driven, with `Instant`s in
   the events.
2. `PlayerCore::pause_before` and `set_fade`: the player stops exactly at the boundary; a fade
   ramps without changing `volume()`. Seam: `cargo test -p aoide-audio`.
3. Wiring: a due timer pauses at a sentence end; an end-of-chapter timer pauses before the next
   chapter's first entry; play within 3 minutes re-arms it. Seam: core test.
4. `extended` in the view; the pill shows "again, 15 min" after an extension.

### Logging

`sleep.set` (`minutes`, `end_of_chapter`), `sleep.rang` (`minutes`, `end_of_chapter`),
`sleep.extended` (`minutes`, `end_of_chapter`), each where `next` is applied. New rows in
`docs/logging.md`.

### Done when

- The timer never cuts a word.
- Volume after a sleep is exactly what it was before.

---

## 2.5 Listen next: a queue

**Size:** M

**Needs:** 2.2 for the time left; 4.3 for the archive step.

### Outcome

Line up things to listen to. When one ends, the next starts by itself. Add from the library,
the item page, a feed entry or Home; reorder and remove in a panel. Meant for articles: queue
five and go for a walk.

### What exists

- `HomeView.up_next_ids` (`core/facets.rs:52`): a suggestion list, nothing plays from it.
- Finishing an item is `ReadEvent::Progressed { at_end: true }` in `core/reading.rs`
  (`next_read`), sent when the place saved is the item's **last entry**, that is when the
  playhead reaches it, not when it has played. Nothing reacts to the player reaching its end:
  `PlayerCore` reports `Status::Finished`, which `status_of` maps to `PlaybackStatus::Stopped`
  (`core/playback.rs:786`), and the session stays loaded.
- `aoide_generation::plan(listening, preparing)` (`crates/generation/src/lib.rs:88`) orders
  jobs: the listened book's next 90 s (`LOOKAHEAD_MS`, `core/generation.rs:31`), then each
  `Preparing` target, all its uncached items. `Generation::work` builds the targets from the
  preparation queue. `NarrationPlan::speech_items` already shows how to keep the planner off
  items: they are passed as cached.
- A feed entry becomes an item through `entry_item` (`core/feeds.rs:1134`): it creates the
  article on the feed shelf (fetched later when the entry has no full text) and links the
  entry. `open_feed_entry` (`core/feeds.rs:1064`) calls it and **marks the entry read at
  once**. `library.entry_for_book` finds an item's entry.
- The media keys are `src-tauri/src/media/` (`mod.rs`, `mpris.rs`, `souvlaki.rs`); Next and
  Previous call `Core::skip_chapter(±1)`, which at the last chapter plays that chapter's start
  again.
- `merge_audiobook` (`crates/library/src/merge.rs`) moves an audiobook's rows to the book it is
  paired with, table by table, then deletes the audiobook.
- `src/drag.ts` is only the MIME type for dragging a library card onto a collection; nothing in
  the interface reorders a list yet.

### Design

**Data (next library version).**

```
CREATE TABLE queue (
  book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  position INTEGER NOT NULL
);
```

`merge_audiobook` moves the row (`UPDATE OR IGNORE queue SET book_id = ?2 WHERE book_id = ?1`).
An item on the feed shelf that is deleted with its entry leaves the queue with it.

**Component `core/queue.rs`** (model: `core/bookmarks.rs` for the store, `core/preparation.rs`
for the stage):

```
enum Stage { Idle, Between { next: BookId, since: Instant }, }
enum Event { ItemEnded(BookId), Tick(Instant), UserPlayed, UserStopped, Changed }
```

- **An item ends** when the player first reports `Status::Finished`. `Playback::tick` gains a
  `finished` flag for that tick; the tick loop sends `ItemEnded`.
- When the playing item ends and the queue is not empty, wait 2 s (`Between`), then ask the
  host to play the first queued item from its saved place and take it off the queue. Anything
  the user does in between cancels the hand-over. The tick loop idles at one tick a second once
  nothing plays, so the gap is 2 to 3 s.
- An item is taken off the queue when it starts, not when it ends. The component remembers the
  item it started, for the feed rule below.
- Items that cannot be listened to (an article still being fetched or failed, a missing file,
  no voice) are passed over and stay queued, marked with why.
- Host trait: `play(book_id)`, `can_listen(book_id) -> Result<(), Why>`,
  `finished_from_feed(book_id)`.
- **A queued feed article that finishes playing** has its entry marked read and is archived
  (`archive(book_id, true)` with `from = finished`, from 4.3). Until 4.3 is built, only the
  read mark.
- **Next.** `Core::skip_chapter(+1)` at the last chapter hands over to the first queued item at
  once (no 2 s wait) when the queue is not empty. The media keys and the transport's next
  button both go through it.

**Being ready in time.** When the playing item has under 2 minutes left (`book_left_ms`, 2.2),
the first queued item is given to generation for its first minute of audio from its saved
place, so the hand-over has no gap. `Generation` gains an up-next target set by a new
`PlanEvent::UpNext(Option<(BookId, usize)>)`. `work()` puts it first among the `Preparing`
targets, with a copy of its plan items in which every item outside that minute is passed as
cached (as `speech_items` does). The listened book's own 90 s still comes first. No change to
`crates/generation` and no second planner.

**Views and IPC.** `QueueView { items: Vec<QueueItemView { book_id, why_not: Option<..> }> }`,
`queue()`, `queue_add(book_id, place: Next | Last)`, `queue_remove`, `queue_move(book_id, to)`,
`queue_clear()`, event `queue-changed` (`CoreEvent::QueueChanged`). `BookSummary` gains
`queued: bool`. A feed entry is queued with `queue_add_entry(entry_id)`, which makes its item
with `entry_item` and does **not** mark the entry read (`open_feed_entry` would).

**Interface.** "Listen next" and "Add to queue" in the item menus and on `FeedEntry.vue`
(`src/components/feeds/`); a queue list in the player dock (a button with the count), with
move up and move down buttons and drag to reorder (a new drag type beside `ITEM_DRAG` in
`src/drag.ts`); "Then: *title*" under the time left when the item is nearly over.

### Slices

1. Store: add next and last, move, remove, removed with the book, moved by
   `merge_audiobook`. Seam: library test.
2. Stage transitions (table).
3. An item ends (`Status::Finished`), the next plays from its place; a user action in the 2 s
   cancels. Seam: component with a scripted host.
4. Unlistenable items are passed over with a reason.
5. Next at the last chapter hands over.
6. The first minute of the next item is made ahead. Seam: generation test in
   `core/generation.rs`.
7. Feed entries: queued without being marked read; marked read (and archived, with 4.3) when
   finished.
8. Interface and mock scenario ("an item ends, the queued one plays").

### Logging

`queue.changed` (`size`), `queue.handover` (`from`, `to`, `outcome`), `queue.passed_over`
(`book`, `why`). New rows in `docs/logging.md`.

### Done when

- Three queued articles play one after another with no click.
- Closing and starting Aoide keeps the queue.

### Settled

- A queued feed article that finishes playing is marked read and archived (4.3's archive).
  (decided 2026-11-16)
- Nothing is played between items, neither a chime nor the title; only the 2 s gap. (plan's
  default)

---

## 2.6 Shorter silences in recordings

**Size:** M for paired recordings; standalone audiobooks (slice 5) add about as much again.

### Outcome

For a paired or standalone audiobook, long silences are shortened. The item page shows how much
time that saves.

### What exists

- A paired recording plays as one player entry per clip. `Session::feed`
  (`core/session.rs:145`) asks the feeder for clips near the playhead (`clip_audio`,
  `core/recording.rs:690`); the feeder (`core/feeder.rs`) decodes on its own thread and
  delivers each clip **in pieces of 5 s** (`CHUNK_MS`), which `Playback::tick` appends to the
  player (`PlayerCore::append`, then `finish`). A whole clip is never in hand at once.
- Word timings, `off_text`, `start_offset` and `seek_recording` are all in the recording's own
  time (`Clip.audio`, `Clip.words` in `crates/sync/src/clips.rs`); `Session::word_at` reads the
  player's `offset_ms` against the clip's untrimmed length (`NarrationPlan::audio_ms`).
  Exact word times from the aligner (`Alignable::Clip`) are in the same time.
- `crates/segmenter` (package `aoide-text`) finds pauses for its word estimate
  (`align_to_pauses`, `MIN_PAUSE_MS` and `FRAME_MS` at `lib.rs:136`), with a threshold relative
  to the **whole clip's peak**: that needs the whole clip, which streaming does not give.
- **Checked: how an audiobook alone is fed.** `TimelineSession` (`core/timeline.rs`) uses the
  same `Feeder`. Its player entries are fixed stretches of the timeline (`Timeline::entries`,
  `chunks` of `CHUNK_MS` = 10 s), not sentences, and every position is turned into a time by
  `Timeline::time_at(index, offset_ms)` and back by `Timeline::locate`. Trimming a stretch
  shortens it, so both have to map through the cuts.

### Design

**Pure rule in `crates/audio`, streaming.** A `Trimmer` per entry:
`Trimmer::new(rate, options, keep_from_ms)`, `push(&[f32]) -> Vec<f32>`, `finish() -> Vec<f32>`,
and `cuts() -> &[Cut { at_ms, removed_ms }]`. Silence is 10 ms frames under a threshold relative
to the loudest frame heard so far in this recording (kept by the session), with a floor; a run
longer than `keep_ms` (default 500) is shortened to `keep_ms`, cut in its middle with a short
cross-fade. The trimmer holds back only the silent run it is in, until it ends or the entry
does. Nothing before `keep_from_ms` is cut (see seeking).

**Keep every address in the recording's time.** Rather than rewriting word timings, the
session keeps each entry's cuts, and `Playback` maps at the player's edge: offsets read from
`PlayerCore::position()` are turned back into original time before anything uses them (word,
`off_text`, the saved place, `time_at`, the time left), and offsets given to
`PlayerCore::seek_to` are turned into trimmed time. Word timings, the sync map, bookmarks,
aligned times and saved places then mean the same with the setting on or off, with no change
to them.

**Seeking.** A seek lands at a time the trimmer has not reached yet. The trimmer for the
entry sought into starts with `keep_from_ms` = the seek's offset, so the trimmed and original
offsets agree up to that point.

**Where it runs.** In the feeder thread, so the tick never trims: `Feeder::push` takes the
options and `keep_from_ms`, and each `Fed` piece carries its new cuts.

**Scope.** First version: paired recordings. Standalone audiobooks second (slice 5): the same
mapping inside `time_at` and `locate`. A silence across two stretches is shortened in each, so
it can keep up to twice `keep_ms`.

**Settings.** `BookSettings.shorten_silences: Option<bool>` over
`AppSettings.shorten_silences` (off by default). Saving book settings restarts the session
(`save_book_settings`), which is what a change of this setting needs. `chapter_left_ms` and
`book_left_ms` (2.2) subtract the cuts known so far.

**Saved time.** Counted as it is played: when the playhead leaves an entry, its removed ms are
added to a new `BookKey::SilenceSaved` book value. Shown as "Shortened silences saved 14 min".

### Slices

1. `Trimmer`: a 2 s gap becomes 0.5 s, also when it spans two pushes; speech samples are
   untouched; nothing before `keep_from_ms` is cut; `cuts` maps times correctly. Seam:
   `cargo test -p aoide-audio` with synthetic tones and silence.
2. The mapping at the player's edge: with cuts in an entry, the word, the place saved and a
   seek mean the same original time. Seam: `Playback` test with scripted cuts.
3. The feeder trims clips when the setting is on. Seam: session test.
4. Setting, saved time, interface.
5. Standalone audiobooks: `time_at` and `locate` through the cuts.

### Logging

`recording.trimmed` (`book`, `chapter`, `removed_ms`), once per chapter as the playhead leaves
it, not per clip. New row in `docs/logging.md`.

### Done when

- A slow narrator's pauses are shorter and the word highlight still lands on the word.
- Bookmarks and the saved place mean the same place with the setting on or off.

---

## 2.7 More keyboard shortcuts

**Size:** S
**Built:** 2026-11-16 (763f6d2..8b28553)

### Outcome

The keys audiobook players have, listed in Settings › About.

### What exists

- `src/shortcuts.ts` is only the list shown in Settings › About (`AboutSettings.vue` renders
  it inline); `src/composables/useShortcuts.ts` handles the keys with its own `if` chains:
  Space, ← / →, Ctrl+F, Ctrl+N, Ctrl+comma, Ctrl + / − / 0 and Ctrl + wheel, Esc.
  `src/tests/shortcuts.test.ts` checks them.
- ← / → move a sentence for text; for an audiobook alone they move 10 s (`STEP_MS` in
  `usePlayerControls.ts`, through the `skip` command). No ±30 s buttons exist in the interface.
- `onKeydown` ignores every plain key pressed with Shift (`!event.shiftKey && plain(event)`)
  and every key with Alt. Ctrl keys (zoom) work even while typing.
- The reader and the library scroll, and ↑ / ↓ are their scroll keys.
- Already in the interface: `player.skipChapter`, `player.skip`, `player.setSpeed`,
  `player.toggleMute` (`src/stores/player.ts:71`, which remembers the level to go back to),
  `controls.bookmark()`, `controls.setSleep`.
- The README's table (`README.md:228`) has already drifted: it lacks Ctrl+N.

### Design

| Keys | Action |
|------|--------|
| Shift+← / Shift+→ | Previous or next chapter |
| Ctrl+← / Ctrl+→ | Back or forward 30 s (audiobook), three sentences (text) |
| `[` / `]` | Slower or faster by 0.1× |
| `\` | Back to 1× |
| Shift+↑ / Shift+↓ | Volume up or down by 10% |
| M | Mute (`player.toggleMute`) |
| B | Bookmark here |
| H | Highlight the sentence being spoken (needs 3.2) |
| T | Sleep timer menu |
| F11 or F | Fullscreen (needs 3.7) |
| `?` | Show this list |

Rules:

- Letter keys and the new arrow combinations do nothing while a text field or an open dialog
  has focus (the existing guard). Ctrl+← / Ctrl+→ in a text field move by word as usual: the
  Ctrl branch must apply the typing guard to them, unlike zoom.
- Shift is part of a key now: the handler stops refusing Shift outright and matches it from the
  table (`?` is Shift+/ on most layouts, so `?` matches on `event.key`).
- ↑ / ↓ alone stay the page's scroll keys; volume is on Shift+↑ / Shift+↓.
- One table in `shortcuts.ts` (keys, action id, label, and whether it works while typing)
  drives both the list and the handler, so they cannot drift apart. Rebinding is not planned.
- H and F join the table when 3.2 and 3.7 are built.
- Built: T asks the player store for the sleep timer's menu (`askSleep`), and every `SleepPill`
  opens on it. Outside the reader the pill lies unseen across the mini player, so the menu
  opens over that card; in a book read silently while another plays, it lies unseen at the
  right end of the reader's foot. The speed and volume keys say the new value in a toast, as zoom does;
  speed steps land on whole tenths (1.25 goes to 1.3 or 1.2) within 0.5× and 3×.

### Slices

1. `shortcuts.ts` becomes the single table; the handler dispatches from it. The existing tests
   pass unchanged.
2. Each new key, with a test that it calls the store action and one that it is ignored in a
   text field.
3. The `?` sheet: the list in `AboutSettings.vue` becomes a `ShortcutList.vue` both use.
4. The README's table matches `shortcuts.ts` (a test compares them).

### Done when

- Every row works in the reader and from the library while something plays.
- README's shortcut table matches `shortcuts.ts`.

### Settled

- Volume is on Shift+↑ / Shift+↓; plain ↑ and ↓ keep scrolling the reader and the library
  (decided 2026-11-16).
- Ctrl+← / Ctrl+→ move 30 s in an audiobook without text, where plain ← and → already move
  10 s (decided 2026-11-16).

