# Phase 3: audiobooks on their own

Read [plan.md](plan.md) first. Mockup: `design/ReaderAudio.dc.html`; kind switch in
`design/Library.dc.html`.

## Outcome

An audiobook (one M4B, or a folder of MP3, FLAC or Ogg files) can be added with no EPUB and
listened to: parts, scrubbing, speed, sleep timer, bookmarks, resume, stats. Later the user can
add the EPUB and get read-along without losing their place.

## What exists (start here)

- `crates/recording`: `inspect(paths) -> Inspection` (files, durations, `Tags { title, album,
  artist, album_artist, track, disc }`, `RecordingChapter`s, `Problem`s including DRM),
  `reader.rs` (decoding with seek).
- `crates/library/src/recordings.rs`: `recordings` rows keyed by `book_id` (files, total_ms,
  stage, accepted), transcript blocks, sync maps.
- `src-tauri/src/core/recordings.rs` (sync job), `core/recording.rs` (`Narration`,
  `attach_recording`, `play_recording_at`), `core/feeder.rs` (decodes a recording's clips on
  its own thread), `core/playback.rs` + `core/session.rs` (a session is a book's entries;
  the player is fed clips), `core/narration.rs` (one `Fate` per entry).
- `core/audiobooks.rs` + `crates/discovery/src/folder.rs`, `names.rs`: "Find audiobooks"
  matches audiobooks on disk to books by name.
- UI: `PairRecordingDialog.vue` (drop audio → choose a book), `FindAudiobooksView.vue`.

Read `docs/audiobook-sync.md` sections "Import and inspect" and "Playback" before starting:
the recording is never copied or changed, and that stays true here.

## Data (library v8)

- `books.kind TEXT NOT NULL DEFAULT 'book' CHECK(kind IN ('book','audiobook','article'))`.
- `progress.position_ms INTEGER` and `bookmarks.position_ms INTEGER` (null for text items).
  For an audiobook, `chapter` is the part index and `position_ms` is the place on the whole
  recording's timeline; `char_offset` is 0 and `segment_id` null.
- An audiobook item has a `books` row (`path` = the M4B, or the folder) and a `recordings` row
  with the same id. Its recording stage is a new terminal stage that needs no sync (see below).
- `NewBook` gains `kind`; `BookRecord` and `BookSummary` gain `kind: Kind`.

## Identity

`aoide_recording::fingerprint(files) -> String`: SHA-256 over, for each file in listed order,
its length and its first and last 64 KiB. Hex, used as the `BookId`. The same audiobook dropped
twice is the same item. Tested with synthetic files: equal for the same files, different when a
file's size or its edges change, different for a different order.

## Playing a timeline (`src-tauri/src/core/timeline.rs`, new)

A text item plays entries (segments). An audiobook has none: it plays its recording straight
through on one timeline in milliseconds.

- Make what the player is playing an explicit enum in `core/playback.rs`:
  `Playing { Entries(Session), Timeline(Timeline) }`. Every place that today assumes a
  `Session` matches on it.
- `Timeline` holds the item id, the recording's files and total length, its parts
  (`Vec<Part { title, start_ms, end_ms }>` from `RecordingChapter`s, else one part per file,
  else a single part), and the position.
- Audio comes from the existing feeder: ask it for the stretch from the position onward in
  pieces of a few seconds, as it already does for clips. Seeking drops what was asked before.
- `PlayerStateView` for a timeline: `chapter` = part index, `segment_id` = none,
  `chapter_fraction` and `chapter_left_ms` from the part, plus new fields
  `position_ms: Option<u64>`, `duration_ms: Option<u64>` (whole item) and
  `part_position_ms: Option<u64>`.
- Commands: existing `play`, `pause`, `resume`, `stop`, `skip_chapter`, `set_speed`,
  `set_volume`, `set_sleep_timer` work on a timeline. New `seek(id, position_ms)` and
  `skip(id, delta_ms)` (the ±30 s buttons). "End of chapter" sleep means end of part.
- Progress is saved as for books (on pause, stop, and periodically) with `position_ms`.
  Reaching the end sets `finished_at` (phase 2's `ReadEvent`).
- Listening time is recorded as for any listening. MPRIS (`src-tauri/src/media.rs`) shows the
  item's title and author.

## Recording stage for an audiobook

In `core/syncing.rs` add a stage for a recording that has no text to sync with (name it for
what it is, for example `Stage::Standalone`). It is `listenable`, has no report, and no event
moves it except attaching text (below). The sync job skips recordings in it.

## Item views

- `BookView` for an audiobook: `shape: Listen`, `chapters` = parts (title, and a new optional
  `duration_ms` on `ChapterSummary`), `word_count: 0`, no warnings from text extraction;
  recording problems (unreadable file, missing file) surface as warnings.
- `load_book` is not called for audiobooks. Find every `Core` method that calls it (`chapter`,
  `search_book`, `prepare_book`, `export_book`, `lookup_metadata`, voice settings, ...) and
  make each either work or return a clear "not available for an audiobook" error. The UI hides
  the entry points; the error is the backstop. List them in "Notes from implementation".
- Metadata: title from tags (`album`, else `title`, else the folder or file name cleaned by
  `crates/discovery/src/names.rs`), author from `album_artist` else `artist`. Cover: embedded
  art if the recording crate can read it (add that to `inspect` if missing), else the
  generated cover.

## Adding an audiobook

- `Core::add_audiobook(paths) -> ImportResult`, command `add_audiobook`. Inspects, refuses DRM
  and unreadable input with the existing messages, computes the fingerprint, inserts the item
  and its recording row.
- Entry points:
  - Add dialog and window drop: audio with no book open and no name match goes straight to
    "add on its own" with a toast naming it; audio dropped on a book's page pairs as today.
  - `PairRecordingDialog` gains "Add on its own" beside the book choices.
  - Find audiobooks: audiobooks that match no book are listed in their own section with "Add".
- Missing files: `rescan` marks an audiobook `missing` when its files are gone, like a book.

## Adding the text later

"Add the EPUB…" on the audiobook's reader and item page:

1. The EPUB is imported as a book (existing import).
2. `Core::merge_audiobook(audiobook_id, book_id)`: moves the recording row to the book and
   starts the normal sync (stage leaves `Standalone` through the usual first event); copies
   collections, `added_at` (the earlier one), bookmarks (kept as time bookmarks) and listening
   stats; stores the audiobook's `position_ms` in the book's settings as a pending place;
   removes the audiobook item.
3. When the sync is far enough that the pending place has a matching segment, the core turns it
   into normal progress and clears it. If the user reads or plays the book first, the pending
   place is dropped.

The merge is one transaction in the library crate, tested there.

## Slices

### 3.1 Kind in the library
Test (`library_v8.rs`): a v7 database opens with every row `kind = book`; an audiobook row
round-trips; `position_ms` round-trips on progress and bookmarks.
Done: `BookSummary.kind` is in the contract and the mock.

### 3.2 Fingerprint and `add_audiobook`
Test (crate): fingerprint properties above. Test (core, `recording_tests.rs` style with the
time-encoding WAV): adding a WAV creates an item of kind `audiobook` with a `Standalone`
recording, title from tags; adding it again returns the same id; a DRM file is refused.
Done: the item appears in `list_books` with `audio: Recording`, `shape: Listen`.

### 3.3 Timeline playback
Test (core, silent sink): play an audiobook; the player state reports part, position and
duration; pause saves `position_ms`; reopening resumes there; `seek`, `skip(±30 s)` and
`skip_chapter` land where expected (the WAV's samples encode their time); playing to the end
finishes the item; sleep at end of part stops at the boundary; listening time is recorded.
Done: no `Session`-only assumption is left unmatched in `core/playback.rs`.

### 3.4 Guard the text-only operations
Test (core): each text-only method called on an audiobook returns the clear error and leaves
state unchanged.
Done: the list of methods is recorded in "Notes from implementation".

### 3.5 The Listen reader
- `ReaderView` shape `Listen`: `src/components/reader/AudioReader.vue` from
  `ReaderAudio.dc.html`: cover, title, author, scrubber over the current part with elapsed and
  remaining time, transport with −30 / +30, speed, sleep, parts list (current part marked,
  click to jump), "percent of the book · time left at speed", the Text card with "Add the
  EPUB…".
- Bookmark button stores a time bookmark; the tools sheet lists them as "Part 7 · 18:42" and
  jumps on click. Search and voice tabs are hidden.
Test (`reader.test.ts`): renders from mock state; scrubbing calls `seek`; a part click jumps;
bookmarks add and jump.
Done: the Page/Lines switch, Aa and Listen controls are absent in this shape.

### 3.6 Adding from the interface
Test (`recording-ui.test.ts`, `audiobooks.test.ts`): each of the three entry points adds the
audiobook and shows the toast; unmatched audiobooks list in Find audiobooks.
Done: dropping an M4B on an empty library ends with an item on Home.

### 3.7 Library and item page for kinds
- Library kind switch (All, Books, Audiobooks) with counts; shown once two kinds exist.
  Audiobook cards use a square cover and the line "Audio only · 61%".
- Item page for an audiobook: Listen is the only primary action; Text card says there is no
  text and offers "Add the EPUB…"; Audio card shows length, parts, files and problems;
  Contents lists parts with durations. Details online is hidden.
- Home cards for audiobooks as in `Main.dc.html`.
Test (`library.test.ts`, `details.test.ts`).
Done: no control that needs text is visible for an audiobook.

### 3.8 Merge with an EPUB
Test (library crate): the merge transaction moves and copies everything listed and removes the
audiobook. Test (core): after a scripted sync reaches the pending place, progress points at the
matching segment; reading first drops the pending place.
Done: in the native app, an audiobook listened to part 3, then given its EPUB, opens in
read-along near the same place once synced.

## Done when

- All slices committed, all checks pass.
- Native app: add a folder of MP3s and an M4B; listen, seek, change speed, set a sleep timer,
  bookmark, quit and resume; media keys work.
- A book with a paired recording behaves exactly as before the phase.

## Notes from implementation

- 3.1: the enum is `ItemKind`, not `Kind` (the contract names TS types by their Rust name).
  The migration is library v9 (phase 2 used v8).
- 3.3: what plays is `Playing { Entries(Session), Timeline(TimelineSession) }`. The player
  holds each entry's audio whole, so the timeline is laid out as stretches of at most 10 s that
  never cross a part; the feeder decodes the six ahead of the playhead and four are kept
  behind. `Place` is an enum too (`Entry` or `Time`). The core method behind the `seek` command
  is `Core::seek_to` (`Core::seek` moves to a book's entry). Seeking or skipping an audiobook
  that is not playing moves its saved place. A place within 1 s of the end is the end: it
  finishes the item, and playing it again starts from the beginning. A file's parts, when an
  audiobook has several files and no chapters, are titled by the file's title tag, else its
  cleaned name; they are stored with the recording.
- 3.4: `Shelf::load` refuses an audiobook, so everything that needs the text gets "Not
  available for an audiobook: it has no text.": `chapter`, `search_book`, `prepare_book`
  (before it wants the speech engine), `prepare_chapter`, `set_reading_place`, `lookup_metadata`, `apply_metadata`. Guarded up front because they
  would change state or give a less clear error: `regenerate_book`, `export_book`,
  `export_read_along`, `set_mode(Read)`, `attach_recording`, `attach_recording_as_listed`,
  `remove_recording` (an audiobook goes with its recording: remove the item),
  `set_narration`, `play_recording_at`. Made to work: `book_overview`/`open_book` (parts as
  chapters with `duration_ms`, a missing file as a warning), `book_details` (stored details
  only), `add_bookmark` (a time bookmark at the playhead or saved place, its excerpt the part's
  title), `chapter_audio` (empty: no speech is made for it, and its page asks on opening).
  Voice settings save but change nothing for an audiobook.
- 3.5: the scrubber spans the part playing, not the whole book; the line under the transport
  gives the whole book's percentage and time left at the current speed. Bookmarks read
  "Part N · m:ss" and jump by seeking, then playing. The sleep timer's "End of chapter" ends
  the part. The mock's audiobook (`book-5`, Winter Radio) has parts of about a second, so
  previews show tiny lengths.
- 3.6: Find audiobooks lists folders close to no book under "Not in your library". Any folder
  of audio counts, so music folders in the scanned places show up there too; they are only
  added when the user picks Add. Dropped audio goes to the book whose page or reader is open
  (not an audiobook), else to pairing when its name comes close to a book, else on its own.
- 3.7: the library's kind switch only shows when the library holds more than one kind. An
  audiobook's square cover sits at the foot of a portrait frame, so titles line up with
  EPUBs on the same row.
- 3.8: "Add the EPUB…" (reader and page) imports the chosen EPUB, merges the audiobook into it
  (`merge_audiobook`) and opens the book's page. The listened place waits as
  `pending_place` until the sync has a clip at or after it, then becomes the reading place.
- Covers: the embedded front cover (else the first picture) is saved beside the EPUB covers
  as `<id>.<ext>`. Debug smoke homes now allow their own covers folder in the asset scope,
  which only names the real one.
- Done-check (native, muted, synthetic audio): a folder of three tagged MP3s and an M4B with
  chapters and cover; listen, part jumps, ±30 s, seek, 1.5×, sleep at the end of a part,
  bookmark and jump back, quit and resume at the same place with the same speed; media keys
  (play-pause, next, previous) over MPRIS. A tiny EPUB paired with espeak-ng speech of its
  text synced to Ready and plays in the text reader with highlighting as before. MPRIS sends
  no length for what plays (neither did speech).
