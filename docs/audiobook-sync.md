# Pairing a real audiobook with its EPUB

**Status:** proposed plan (2026-11-13). Once agreed, it replaces the *Idea* section of
[roadmap.md](roadmap.md).

## What the user gets

- Drop a DRM-free audiobook on a book or on the window. It can be one M4B, or a folder of MP3,
  FLAC or Ogg files. Aoide first checks that the recording is the same book. Then it syncs in the
  background ("Syncing… 34 %, about 12 min left").
- After that the book plays the narrator instead of a speech engine, in the same reader:
  - sentence shading and the current word;
  - click a sentence to hear it;
  - the Lines view, bookmarks, search and the sleep timer;
  - speed from 0.5× to 3×, media keys, listening stats, resume where you stopped.
- A sync report says how much of the book matched and what didn't:
  - chapters the narrator skipped;
  - parts of the recording that aren't in the text (credits, or a dedication Aoide left out);
  - a warning when the recording looks abridged, or like a different edition or translation.
- Nothing leaves the computer. Speech recognition runs inside Aoide. The recording is never
  copied or changed.

**Not in the first version:**
- audiobooks without an EPUB;
- DRM-protected files (refused with a clear message);
- languages written without spaces between words;
- one recording covering several EPUBs, or one EPUB split across several recordings.

## The key idea: a recording is one more source of segment audio

Today the player is a gapless playlist whose entries are the book's segments. The session gives it
decoded audio and word timings for each entry, and the reader highlights a (segment, word) pair.

With a recording, each segment gets a **clip**: a slice `[start, end)` of the recording.
- The clips are contiguous, so playing them back to back plays the recording exactly as it is,
  pauses included.
- Nothing after the player changes. The new work is finding the clips.

```
audio files ─probe─▶ timeline ─decode, 16 kHz─▶ transcriber ─▶ heard words ......... stored
                         │
                         └─▶ loudness envelope ...................................... stored
EPUB ─parser─▶ chapter text ─┐
heard words ─────────────────┴─▶ matcher ─▶ sync map (word times by text position) .. stored
sync map + segments + envelope ─▶ clips ................... computed when the book opens
clips ─▶ player playlist, decoded on demand from the original files
```

Aoide stores three things:
- **Heard words.** Expensive: recognition takes minutes to an hour.
- **The loudness envelope.** A cheap by-product of the same pass.
- **The sync map.** About a second to recompute.

Clips are worked out again each time a book opens. So changes to how sentences are split never
spoil a sync. A parser change that alters the text needs only a new match, not a new transcription.

## Decisions (recommended; confirm or change)

| # | Question | Recommendation | Why |
|---|---|---|---|
| A1 | Speech recognition | whisper.cpp built into Aoide through `whisper-rs` (Unlicense), on the CPU by default, with CUDA, Vulkan or ROCm as build features. A `Transcriber` trait, plus an adapter for OpenAI-style `/v1/audio/transcriptions` servers in phase 2 | Recordings are for people who may not run any GPU server. Keeps the adapter model already used for speech engines |
| A2 | Model | `base.en` for English books and `base` for others (142 MiB); `tiny` and `small` as options. Phase 0 settles the default | The book supplies the real words, so recognition only has to find landmarks. Storyteller used `tiny.en` for English |
| A3 | Getting the model | Downloaded only when the user clicks, from a fixed URL, checked against a pinned SHA-256. Or choose a model file already on disk | It is Aoide's first network use besides Open Library, so it must be explicit and listed in the README |
| A4 | Audio files | Used where they are, never copied or modified. A fingerprint notices moved or changed files (the recording becomes *missing* and reconnects later) | Audiobooks are hundreds of MB |
| A5 | What a sync stores | Word times keyed by chapter and character range (as saved progress is), not by segment id | Survives changes to sentence splitting |
| A6 | Matching | Deterministic, no ML: word sequences unique in both texts are anchors, chained in order, refined recursively, then word-level alignment in the small gaps | Testable, fast, and its report is explainable |
| A7 | Text the narrator skips | Skipped in playback and listed in the report. Filling it with TTS is an opt-in for later | Switching voices mid-book is jarring |
| A8 | Audio the text lacks | Kept and played as a lead-in to the next sentence. The reader shows *Not in the text* while it plays | Never cut the narrator's audio |
| A9 | Wrong or poor matches | A quick three-sample check before the long sync. Afterwards, under 75 % of the text matched means *Needs review*; the user can accept it anyway | Catches a wrong edition, translation or abridgement early |
| A10 | Playback | Clips decoded from the original files on a feeder thread, at 48 kHz mono with a proper resampler. Stereo is mixed down to mono | The player is mono and the app is speech-first |
| A11 | Which narration plays | A per-book choice, *Recording* or *Speech engine*. It switches to *Recording* by itself when a sync finishes | Going back to TTS is one click |
| A12 | Word highlight | Whisper's word times, with sentence cuts snapped to pauses. Exact timing from the existing aligner can be added later | Close enough to follow along; exact timing stays opt-in, as it is for TTS |

## Pipeline

### 1. Import and inspect (new crate `aoide-recording`)

The input is files and folders: dropped on the window or a book page, or chosen with *Add
audiobook…*.

- **Expand folders:** one level deep, plus disc subfolders (`CD1`, `Disc 2`). Keep audio
  extensions only.
- **Order:** by disc and track tags, then by natural file-name order ("Part 2" before "Part 10").
  Show the order to the user; reordering by hand comes in phase 2.
- **Probe each file:**
  - container and codec, sample rate, channels;
  - exact length in samples;
  - tags: title or album, artist or album artist, track, disc;
  - embedded chapters (M4B) and cover.
- **Exact lengths matter.** The timeline is the files laid end to end. The transcription pass and
  playback must agree on every time in it, so both use the same decoder with the same settings.
  - Each file's decoder is chosen once, at inspection, and stored.
  - A seek lands on a packet at or before the target. The reader then decodes on and drops samples
    up to the exact one.
- **Decoding:**
  - `symphonia` 0.6 with its `isomp4` and `aac` features turned on handles MP4/M4B with AAC-LC,
    MP3, FLAC, Vorbis and WAV.
  - It has no Opus decoder. Ogg Opus goes through the libopus that `aoide-audio` already links.
  - Symphonia refuses HE-AAC when the stream header declares it ("aac too complex"). Those files,
    and anything else it can't open, are decoded by piping through `ffmpeg` when it is installed.
    It is already an optional dependency for export. Without ffmpeg, the file is listed as
    unsupported, with that reason.
  - Symphonia doesn't trim MP4 encoder priming (about 46 ms). That doesn't matter because both
    passes see the same samples.
- **Chapters:**
  - M4B: `mp4ameta` reads both kinds, Nero `chpl` lists and QuickTime chapter tracks, because
    symphonia reads neither.
  - MP3: ID3v2 `CHAP` frames, through symphonia's `chapters()`.
  - Chapters are for the report and for showing the files. Matching doesn't need them.
- **DRM:** check the container before decoding, because symphonia otherwise fails with an obscure
  "no track". Refuse with "This recording is DRM-protected; Aoide can only play audiobooks that
  play in any audio player." Signs of DRM:

  | Format | Sign |
  |---|---|
  | Audible AAX | `ftyp` brand `aax `, an `adrm` box, or an `aavd` sample entry |
  | Audible AAXC | `ftyp` brand `aaxc` |
  | Audible `.aa` | the 32-bit value `0x57907536` at offset 4 |
  | Apple FairPlay | a `drms` sample entry |
  | Encrypted MP4 (CENC) | an `enca` sample entry with `sinf` |
  | Protected WMA/ASF | a content-encryption object |
- **Fingerprint** each file by size, modification time and a SHA-256 of its first and last MiB.
  This is fast, and enough to notice a moved or replaced file.
- **Suggest the book:** compare the tags with the library's books (normalised overlap of title
  and author words). Pre-select the best match when it is clear; otherwise show a picker.

```rust
pub fn inspect(paths: &[PathBuf]) -> Result<Inspection, RecordingError>;
pub struct Inspection { pub files: Vec<AudioFile>, pub chapters: Vec<RecordingChapter>,
                        pub total_ms: u64, pub tags: Tags, pub problems: Vec<FileProblem> }
pub struct Reader; // read(&mut self, range_ms) -> Pcm across file boundaries;
                   // sequential reads never seek; a jump seeks once.
pub struct Envelope { pub frame_ms: u32, pub levels: Vec<u8> } // + quietest(range) -> ms
```

### 2. Quick check (seconds)

Before starting a long job:
1. Transcribe three 45-second windows, at 15, 50 and 85 % of the timeline, in the book's language
   (or its per-book override).
2. Count how many of their four-word sequences occur in the book.

The result is either "looks right" or "this doesn't look like the same book" (the latter goes to
*Needs review*). The check also measures recognition speed on this computer, which gives the time
estimate.

### 3. Transcription (new crate `aoide-transcribe`; the job runs in `src-tauri`)

- **Decode** the timeline in order, mix down to mono and resample to 16 kHz.
- **Work in blocks** of about 5 minutes. Cut each block at the quietest 10 ms within 30 s of the
  5-minute mark, so no word straddles a cut and no overlap needs stitching.
- **Store each finished block** with its start, end, words and transcriber/model id.
  - A paused, cancelled or interrupted job carries on from the next block.
  - Progress is time transcribed over total time. The time left comes from the measured speed.
- **Store the loudness envelope** from the same pass: 10 ms frames, one byte of dB each, about
  3.6 MB for 10 hours. Clips use it to place their cuts.
- **Whisper settings:**
  - language fixed to the book's, no translation;
  - token timestamps with DTW (`DtwMode::ModelPreset`, which exists from `tiny.en` to
    `large-v3-turbo`). DTW needs flash attention off, which is `whisper-rs`'s default;
  - a word starts at its first token's DTW time and ends where the next word starts, or at its last
    token's end before a pause;
  - whisper.cpp's built-in Silero voice-activity detection, to skip music and silence and to cut
    down hallucinations. Its model is 864 KB and is fetched along with the Whisper model;
  - half the CPU cores by default;
  - an abort callback, so *Pause* takes effect within seconds.
- **Speed.**
  - Published figures: `small` runs at about 6× real time on an 8-thread desktop CPU, and
    Storyteller measured `tiny.en` at 40× on an M1 Max.
  - So a 10-hour book should take very roughly 20–60 minutes on a CPU with `base`. Phase 0
    measures this.
- **Timing bias.** Phase 0 also measures any systematic early or late bias in each model's word
  times against hand-checked points, and stores a correction per model. Storyteller does the same
  for its models (±40 ms).

```rust
pub trait Transcriber: Send + Sync {
    fn descriptor(&self) -> &TranscriberDescriptor; // kind, name, model id, languages
    fn transcribe<'a>(&'a self, request: &'a Request) -> BoxFuture<'a, Result<Vec<Word>, Error>>;
}
pub struct Request { pub samples: Vec<f32> /* 16 kHz mono */, pub language: Option<String>,
                     pub abort: Arc<AtomicBool> }
pub struct Word { pub text: String, pub start_ms: u32, pub end_ms: u32, pub probability: f32 }
```

**Implementations:**
- `WhisperCpp`, behind the cargo feature `whisper`: on in the app, off for `npm test`.
- A scripted fake, for tests.
- Phase 2: `OpenAiTranscription`. Like the speech clients, it is loopback-only by default, with
  no proxies, redirects or automatic retries.

### 4. Matching (new crate `aoide-sync`: pure and deterministic)

**Inputs:**
- each chapter's display text, exactly as the segmenter sees it, with notes left out;
- the heard words, with times on the whole recording's timeline.

**Steps:**
1. **Tokenise both sides the same way:**
   - Unicode NFKC and lower case; accents folded for comparison only;
   - one kind of apostrophe;
   - surrounding punctuation stripped;
   - compounds split at hyphens, dashes and slashes;
   - `[12]` note markers dropped, and so is any token without a letter or digit.

   Book tokens keep their (chapter, character range). Tokens containing digits are marked
   *numeric*. For English, numbers are also spelt out in words, as Storyteller does.
2. **Locate each chapter.** Order is never assumed, because some recordings order chapters
   differently from the EPUB.
   - Each chapter's five-word sequences that appear once in the transcript vote for where the
     chapter starts and ends there. Votes are counted in bins of 1,000 words.
   - Chapters are placed strongest first. Each one claims its stretch of the transcript, so two
     chapters never share audio.
   - A chapter with too few votes is reported as not narrated. The threshold: the winning bin needs
     20 % of the votes, and more than half the sequences must be found.
3. **Anchors** inside each placed chapter: six-word sequences (found with a rolling hash) that
   occur exactly once in the chapter and exactly once in its stretch of the transcript, with no
   numeric token.
   - Chain them with a longest increasing subsequence, so positions rise on both sides
     (O(n log n)).
   - Merge neighbouring anchors into runs.
   - Inside each gap between runs, look again for sequences unique *within that gap*, with four
     words, then three, then two.
4. **Fill** gaps of up to about 250 × 250 tokens with Needleman–Wunsch alignment:

   | Pair | Cost |
   |---|---|
   | Exact match | 0 |
   | Close spelling (edit similarity ≥ 0.8; names Whisper misspells, *colour*/*color*) | 0.5 |
   | A numeric token against 1–6 number words ("1865" ↔ "eighteen sixty-five") | 0.5 |
   | Substitution | 1.5 |
   | Gap | 1 |

   Larger gaps without anchors stay unmatched. They are text the narrator didn't read, or audio
   the text doesn't have.
5. **Clean up:**
   - drop isolated matches of very common words inside long unmatched stretches;
   - drop matches that imply an impossible pace (over 8 words per second across 5 or more
     words).
6. **Times:**
   - A matched word takes the heard word's span.
   - Unmatched words between matched neighbours share out the time in between by letters, as
     `estimate_word_timings` does. This only happens when the gap is plausible for that many
     letters; otherwise those words stay untimed (not narrated).
7. **Report:**
   - text coverage: timed words over all words;
   - audio coverage: matched heard words over all heard words;
   - coverage and time span per chapter;
   - unmatched stretches of audio longer than 3 s, each with a short excerpt of what was heard;
   - chapters under 20 % matched, reported as not narrated.

A 100,000-word book should match in about a second. The map holds four numbers per word (about
1.6 MB raw) and is stored compactly with a format version byte.

```rust
pub fn tokenize(text: &str) -> Vec<Token>;  // Token { range: Range<usize>, norm: String, numeric: bool }
pub fn match_book(chapters: &[&str], heard: &[HeardWord], options: &MatchOptions) -> SyncMap;
pub fn clips(map: &SyncMap, units: &[Unit], envelope: Option<&Envelope>, total_ms: u64) -> Vec<Clip>;
impl SyncMap { pub fn report(&self) -> &Report; pub fn to_bytes(&self) -> Vec<u8>;
               pub fn from_bytes(bytes: &[u8]) -> Result<Self, FormatError>; }
```

### 5. Clips: from the sync map to segments (when a book opens)

Take the BookModel's segments in order. Recording books are built with `read_notes` off.

- **Timed segments.** A segment is timed if any of its words is. Its text span runs from its first
  word's start to its last word's end.
- **Cuts.** Between consecutive timed segments *a* and *b*, cut at the quietest envelope frame
  between *a*'s end and *b*'s start. When noisy timestamps make the two overlap, cut 30 ms before
  *b*'s start.
- **Lead-ins.** When more than about 3 s of unmatched speech lies between *a* and *b*, cut just
  after *a*. That stretch becomes *b*'s lead-in and is recorded as an *off-text* range.
- **Ends.** The first timed segment's clip starts at 0. The last one's ends at the end of the
  recording, so closing credits become a tail, also off-text.
- **Untimed segments** get empty clips. They become non-speakable entries, which the player
  already skips.
- **Word times** inside a clip are made relative to the clip's start.

```rust
pub struct Clip { pub audio: Range<u64>, pub text: Option<Range<u64>>,
                  pub words: Vec<WordEstimate>, pub off_text: Vec<Range<u64>> }
```

Each boundary is converted to samples once, so clip lengths add up exactly to the recording's
length.

### 6. Playback (`src-tauri` and `aoide-audio`)

- **`Target` gets an explicit narration:**
  `enum Narration { Speech(Plan), Recording(RecordingPlan) }`. A recording session:
  - never calls the planner or the worker;
  - doesn't need an engine, so `play()` skips `ensure_voice` and `ensure_model`;
  - hides *Prepare whole book* and M4B export.
- **A feeder thread** owns the decoder. The tick loop asks it for the clips in the next couple of
  minutes and hands the results to the player, so the tick loop never blocks on decoding. The
  feeder:
  - decodes each clip from the original files;
  - resamples to 48 kHz with `rubato`, carrying the resampler's state from one clip to the next
    (reset on a seek);
  - returns 48 kHz mono PCM.
- **`PlayerCore` changes** (test-first in `crates/audio`):
  - `seek_to(index, offset_ms)`;
  - streaming slots (`append(index, samples)`, `finish(index)`), so a lead-in of several minutes
    never has to be decoded in one go;
  - stall only on reaching the end of the audio provided so far for an unfinished slot.
- **Where a seek lands:**

  | Action | Starts at |
  |---|---|
  | Click a sentence | its text start, minus 150 ms |
  | Chapter menu or chapter skip | the start of the chapter's first clip, so you hear "Chapter Seven" |
  | Resume | the saved sentence's text start |
  | ← / → | the previous or next sentence's text start |

- **`PlayerStateView` gains** `source` (speech or recording) and `offText`. For recordings it also
  gains the exact `chapterElapsedMs` and `chapterDurationMs`, so the dock shows real times instead
  of the 160-words-a-minute estimate.
- **Unchanged:** progress, bookmarks, stats, the sleep timer and MPRIS. They all work on segments
  and text positions.
- **Switching** narration mid-book restarts the session at the current sentence, as changing the
  voice does today.

### 7. Exact word timing (phase 3)

`core/align.rs` aligns one segment's audio with its words. Change its input from "a cached TTS
file" to "segment audio plus words", and recording clips can be refined the same way, with the
results kept in the sync map.

- **This aligner is safe to build on.** torchaudio 2.10 says `forced_align` "is preserved and will
  remain"; 2.11 still has it and `MMS_FA` with no deprecation.
- **Keep alignment short.** Its memory grows with audio length times text length (about 18 GB
  for a whole hour). So it stays per sentence, as today, inside the clips the matcher found.

If phase 0 shows Whisper's word times are too loose, the stronger route is Storyteller's newer
method: frame-level CTC emissions from MMS (wav2vec2), forced alignment between anchors, and a
`<star>` token that absorbs audio with no text. In Aoide that could run in the aligner service,
or in Rust through ONNX Runtime. It is heavier (an MMS model of about 1.2 GB), so it isn't the
starting point.

## Data (library migration v6)

| Table | Holds | Notes |
|---|---|---|
| `recordings` | id, book_id, label, files (JSON: path, size, mtime, fingerprint, codec, rate, channels, samples), chapters (JSON), total_ms, stage, error, sample_check, accepted, added_at | The stage is saved so the queue survives restarts |
| `recording_blocks` | recording_id, start_ms, end_ms, transcriber, words (BLOB) | One row per finished block |
| `recording_envelopes` | recording_id, frame_ms, levels (BLOB) | |
| `sync_maps` | recording_id, text_hash, matcher_version, map (BLOB), report (JSON), created_at | When the text hash or matcher version differs, match again from the stored words |
| book settings (JSON) | `narration: "speech" \| "recording"` | |

Removing a book or a recording deletes these rows and never the audio files. The Storage tab shows
the size of the sync data. Recordings are listed there as "not managed by Aoide", outside the cache
limit.

## State machine: a recording's sync (`src-tauri/src/core/syncing.rs`)

Following AGENTS.md, a recording has one explicit stage, changed only by `next(stage, event)` and
tested against the table. The UI renders the stage. Progress numbers (time done, total, time left)
sit beside the stage and never decide it. A recording can be played **only** in `Ready`.

| From | Event | To |
|---|---|---|
| (new) | `Attach` | `Checking` |
| `Checking` | `Verified` (no or partial transcript) | `Waiting` |
| `Checking` | `Verified` (complete transcript, map stale or missing) | `Matching` |
| `Checking` | `Verified` (complete transcript, current map) | `Ready`, or `PoorMatch` when the report is poor and not accepted |
| `Checking` | `SampleFailed` | `SampleMismatch` |
| `Waiting` | `Promote` (first in line) | `Transcribing` |
| `Transcribing` | `TranscriptDone` | `Matching` |
| `Matching` | `Matched` (good) / `Matched` (poor) | `Ready` / `PoorMatch` |
| `SampleMismatch` | `Accept` | `Waiting` |
| `PoorMatch` | `Accept` | `Ready` |
| `Waiting`, `Transcribing` | `Pause` | `Paused` |
| `Paused` | `Resume` | `Waiting` |
| `Ready`, `PoorMatch` | `TextChanged` | `Matching` |
| any except `Missing` | `FilesLost` | `Missing` |
| `Missing` | `FilesFound` | `Checking` |
| `Checking`, `Transcribing`, `Matching` | `Fail` | `Failed` |
| `Failed` | `Retry` | `Checking` |

`Checking` is the one place that works out "where are we?" from stored data. It runs on attach, at
start-up, when files reappear, and on retry. The quick sample check runs only when no transcript
exists yet. Removing a recording deletes it rather than being a stage. Only one recording
transcribes at a time, in the order they were added.

## Crates and files

| Where | Change |
|---|---|
| `crates/recording` (`aoide-recording`, new) | Inspect, order, probe and DRM-check files; multi-file timeline; range reader; 16 kHz stream; envelope. No Tauri, no network |
| `crates/transcribe` (`aoide-transcribe`, new) | `Transcriber` trait, whisper.cpp backend (feature `whisper`), scripted fake; later the OpenAI-style adapter, tested over loopback HTTP like the speech clients |
| `crates/sync` (`aoide-sync`, new) | Tokeniser, matcher, report, clips, map encoding. Pure; depends on nothing in Aoide |
| `crates/audio` | `seek_to`, streaming slots |
| `crates/library` | Migration v6 and data access for the tables above |
| `crates/config` / `core/extras.rs` | Recognition settings next to `AlignerSettings`: transcriber kind, model, threads, server URL, carry on at start-up |
| `src-tauri/src/core/recording.rs` (new) | Commands, the sync job runner, the feeder thread, building `RecordingPlan` |
| `src-tauri/src/core/syncing.rs` (new) | The stage machine and its table tests |
| `src-tauri/src/core.rs`, `core/session.rs` | The `Narration` enum in `Target`; session, `provide_audio`, `replan`, `play` and `player_state` branch on it |
| `src-tauri/src/lib.rs`, `capabilities/` | New commands; `tauri-plugin-dialog` for *Add audiobook…* (the app has no file picker yet) |
| `src/api/contract.ts` | Regenerated (`AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`) |
| `src/api/mock.ts`, `src/preview.ts` | Mock recordings in each stage; `?recording=syncing\|ready\|review\|missing` |
| `README.md`, `docs/roadmap.md` | Using an audiobook; "What leaves your computer" (the model download); troubleshooting; roadmap entry |

## Interface

- **Book tools → *Voice* becomes *Narration*.** At the top, a two-way switch: *Recording* or
  *Speech engine*. *Recording* stays disabled until `Ready` and shows the stage meanwhile. Under
  *Speech engine* is the current Voice content. The *Recording* panel shows:
  - the files (for example "1 file, 11 h 42 min, 34 chapters");
  - the stage, with a progress bar and *Pause* / *Cancel*;
  - the report summary and *See report*;
  - *Sync again* and *Remove recording*.
- **Book page:** a *Recording* card with the same status and an *Add audiobook…* button.
- **Pairing dialog** (`PairRecordingDialog.vue`), opened by dropping audio or by *Add audiobook…*:
  - the files in order, with lengths, total length and chapter count;
  - the title and author found in the tags;
  - the suggested book, or a picker;
  - problems file by file (DRM, unsupported format);
  - then *Check and sync*. After the quick check it says, for example, "Looks right: 3 of 3
    samples found in the text. About 25 min to sync on this computer."
- **Queue panel** (`EngineQueue.vue`): a *Syncing recordings* section with progress, time left,
  *Pause* and *Cancel*. It reuses the preparation rows.
- **Sync report** (`SyncReport.vue`):
  - overall coverage;
  - a chapter table with title, % narrated and time in the recording;
  - *In the recording but not in the text*: time, excerpt and a play button for that stretch;
  - *Not in the recording*: the chapters the narrator skipped;
  - *Accept* or *Remove* when the match is poor.
- **Reader:**
  - a *Not in the text* chip while `offText` is true;
  - sentences with no audio drawn as skipped;
  - chapters not narrated marked in the chapter menu.
- **Player dock:** real elapsed and remaining times, and a headphones mark when a recording is
  playing.
- **Library:** a headphones badge on books that have a recording.
- **Settings:** a *Recordings* section:
  - recognition engine;
  - model, with download or *Choose file…*, and its size;
  - threads;
  - carry on syncing at start-up.
- **Drop overlay:** accepts audio files and folders ("Drop to add an audiobook").

**Commands:**
- `inspect_recording(paths)`
- `attach_recording(bookId, paths)`
- `recording(bookId)`
- `sync_report(bookId)`
- `sync_action(bookId, pause | resume | cancel | retry | accept | resync)`
- `remove_recording(bookId)`
- `set_narration(bookId, speech | recording)`
- `transcription_options()`, `save_transcription_settings(…)`
- `download_model(id)`

**Events:**
- `sync://state`: the recording view on every stage or progress change;
- `model://download`: download progress.

`BookSummary` gains `recording: { stage } | null`.

## Tests (test-first, per AGENTS.md)

- **`aoide-sync`, the core of the work:**
  - **Synthetic book texts.** Original prose, a few chapters. Each is paired with a synthetic
    transcript made from it by a seeded, deterministic perturbation:
    - 5 % of words dropped and 5 % misspelt;
    - numbers written as digits;
    - opening credits added;
    - a narrated dedication that isn't in the text;
    - one chapter removed;
    - two chapters swapped;
    - a passage repeated, as a retake would be.
  - **Assertions:**
    - coverage figures;
    - each sentence start within 50 ms of the truth;
    - times never decrease;
    - the unmatched stretches and the skipped chapter are reported;
    - clips are contiguous and add up to the total length;
    - lead-ins and off-text ranges land where expected;
    - the map survives an encode/decode round trip.
  - A few thousand random cases with a fixed seed check that times only ever rise and clips never
    overlap.
- **`aoide-recording`:**
  - **Fixtures:** tiny files committed with the tests (tones and silence of known lengths, a few
    KB each), with the `ffmpeg` commands that made them in a README:
    - an MP3 set with track tags;
    - an M4B with two chapters;
    - a FLAC;
    - an Ogg Opus.
  - **DRM:** a hand-built MP4 header with the AAX brand, made in test code.
  - **Assertions:**
    - lengths are exact;
    - file order follows tags, then names;
    - a read across a file boundary returns exactly the samples asked for;
    - sequential reads join without a gap;
    - DRM is refused.
  - An opt-in check on any real audiobook through `AOIDE_TEST_AUDIOBOOK` prints only metadata and
    coverage.
- **`aoide-transcribe`:**
  - the fake's contract;
  - the OpenAI-style adapter over loopback HTTP with synthetic responses;
  - a real whisper run only as an opt-in (`AOIDE_TEST_WHISPER_MODEL`).
- **`aoide-audio`:**
  - `seek_to` lands on the right sample;
  - streaming slots stall only at the end of what has been provided;
  - clips played back to back equal the source.
- **Core:**
  - the stage table;
  - a recording session with a fake reader: what it wants and provides, `word_at`, off-text;
  - `play()` with a recording never touches the engine;
  - resume after a restart in the middle of a block;
  - `TextChanged` matches again without transcribing.
- **UI (Vitest):**
  - Narration tab states;
  - pairing dialog: suggestion, problems, the quick-check outcome;
  - queue rows;
  - report;
  - the off-text chip;
  - preview screenshots of each `?recording=` state.
- **End to end, by hand,** with two or three DRM-free audiobooks you own, one with footnotes or a
  long introduction: coverage, the highlight by ear at several points, seeking, pause and resume
  during a sync, a restart mid-sync, moving the files away and back.

## Phases

| Phase | Scope | Done when |
|---|---|---|
| **0. Prototype** (2–3 days) | `aoide-recording` (inspect plus sequential decode), the whisper backend, and matcher v1 in `aoide-sync`, driven by an example: `cargo run -p aoide-sync --example sync -- book.epub audio…`. It prints the report and writes the map as JSON | On 2–3 owned unabridged books: ≥ 90 % text coverage, and median sentence-start error under 150 ms (checked by ear by playing clips). Recognition speed and per-model timing bias measured for tiny, base and small, on CPU and GPU, to settle A2. If word times are too loose, decide between "Whisper + per-sentence aligner" and the CTC route (section 7) |
| **1. Listen to a synced recording** (1–2 weeks) | Migration v6; the stage machine and a resumable job runner; *Add audiobook…* and the Narration switch; playback (`Narration`, feeder, streaming slots, `seek_to`); clips with envelope cuts; queue rows; report summary; refusing DRM; missing files; contract; README | A dropped M4B or MP3 folder syncs in the background, survives a restart, then plays with sentence and word highlight; the narration can be switched both ways |
| **2. Polish** (about 1 week) | Drop to pair, with the suggested book and quick check; the full report with "play this stretch"; off-text excerpts; exact times in the dock; the model-download interface; the OpenAI-style transcription adapter; reordering files; **listening while it syncs** (match the transcript so far every few blocks and allow playback up to the transcribed edge) | A wrong book is caught in seconds; you can start listening a minute after adding |
| **3. Extras** (open) | Exact word timing through the aligner; narrated footnotes matched and shown; opt-in TTS for passages the recording lacks; export a synced EPUB 3 with Media Overlays to Exports (a new file, never the source; it plays in other read-along apps); *Find books* pairing audiobooks it finds next to EPUBs. Low priority: importing EPUBs that already carry Media Overlays, which are rare outside fixed-layout children's books | |

**Status (2026-11-14):** phases 0 and 1 are done. From phase 2: drop to pair, the quick check, the report with off-text excerpts, exact times in the dock, the model download, and listening while it syncs. While a recording syncs, the transcript so far is matched in memory when playback needs it; sentences ending more than 30 s before the sync's edge play, later ones wait for it (`SYNC_MARGIN_MS`, `Session::follow`). Also the OpenAI-style transcription adapter (`ServerTranscriber`, Settings → Recordings → A recognition server). Open: reordering files, and phase 3.

## Phase 0 results (2026-11-14)

Measured with `crates/transcribe/examples/sync_book.rs` on an owned 10.4-hour unabridged novel:
one M4B, AAC-LC at 44.1 kHz, 20 chapters, about 100,000 words of text. Machine: 8-core desktop
CPU and an RTX 4070 Ti SUPER (CUDA 13).

**Whole book, base.en on the CPU (8 threads):**

| | |
|---|---|
| Transcription | 27× real time, about 23 minutes for the book |
| Matching | 71 ms for 100k words of text against 99k heard words |
| Text matched to a heard word | 94.1 % |
| Text timed (matched or estimated) | 99.5 %; the 0.5 % is the front matter, which isn't narrated |
| Heard words placed in the text | 99.2 % |
| Chapters | All 20 found in order, each 92–96 % matched and about 100 % timed |
| Not in the text | Opening and closing credits (32 s each), correctly flagged; five short stretches of 3–28 s, mostly misheard names and one repeated passage |
| Sentence starts against speech onsets after a pause (418 sampled) | Median error 70 ms, bias +60 ms (late), 85 % within 150 ms |

**Model and device on the first 30 minutes:**

| Model | Device | Speed | Narrated words matched | Sentence starts: median error, bias, within 150 ms |
|---|---|---|---|---|
| base.en | CPU | 27× | 94.5 % | 70 ms, +70 ms, 84 % |
| base.en | GPU | 77× | 94.5 % | 80 ms, +70 ms, 81 % |
| small.en | GPU | 52× | 96.5 % | 130 ms, +110 ms, 64 % |

Conclusions:

- **A2 holds: base.en is the default.** small.en hears a third fewer words wrong, but its DTW times
  are looser, and timing is what the highlight shows. A missed word still gets an estimated time.
- **The GPU is decode-bound.** Decoding and resampling to 16 kHz runs at about 175× real time and
  currently alternates with Whisper. A feeder thread that decodes the next block while Whisper works
  should bring base.en on the GPU to about 140×. Done: the app's sync job decodes the next block
  on a second thread while the current one is recognised.
- **A constant timing correction of about −60 ms** (Whisper's DTW times are late) would centre the
  sentence-start error. Done: `ModelInfo::late_ms` (60 ms for tiny and base, 110 ms for small)
  is taken off the stored transcript's times when it is matched, so it can be retuned without
  listening again.
- **Seeking:** symphonia seeks every track of an MP4. An M4B's chapter-title track ends where its
  last chapter starts, so a seek past that point fails (here, the last 140 s). The reader now
  bisects once for the latest point that seeks and decodes forward from it (tested with a synthetic
  fixture). Worth reporting upstream.

## App end-to-end (2026-11-14)

The app itself (CUDA build, base.en on the GPU, decoding the next block alongside) synced a
second owned unabridged novel: one M4B of 10 h 15 min, 13 chapters, about 98,000 words.

| | |
|---|---|
| Sync time | about 4 min 20 s, about 140× real time |
| Text matched to a heard word | 94.9 %; every story chapter 94–97 %, the front matter 70 % (its title page and series blurb are not read) |
| Text timed | 99.6 % |
| Not in the text | 2 stretches: a spoken part title (5 s) and the closing credits (32 s) |

It found two bugs that the tests had missed, both now covered:

- whisper-rs 0.16's `set_abort_callback_safe` hands whisper.cpp a pointer of the wrong type, so
  the abort flag read as "stop" and every recognition failed with error -6.
- The read-ahead appended audio from the end of the block's range instead of from the end of
  what was already decoded, so up to 30 s per block were heard twice (86.6 % matched, a hundred
  false "not in the text" stretches).

"Listen now" during the sync played the synced part from the book page. Waiting at the sync's
edge is covered by a core test, not yet checked by hand.

## Risks

| Risk | Mitigation |
|---|---|
| The decoder refuses to seek part of a file (symphonia stops at an M4B's last chapter start) | The reader finds the latest seekable point once and decodes forward from there |
| Recognition is slow on a CPU-only machine | Small models; resumable blocks; a time estimate before starting; GPU builds; a server adapter; listening while it syncs (phase 2) |
| Imprecise word times | DTW timestamps; cuts snapped to pauses; optional exact alignment later |
| Codecs the decoder can't handle (HE-AAC, xHE-AAC, odd containers) | Decode through `ffmpeg` when it is installed; otherwise list the file as unsupported, with the reason |
| Imprecise seeking in VBR MP3s would shift every highlight after a jump | Exact lengths at probe time and the same decoder in both passes. Seeks are packet-level, so decode on and drop samples up to the target |
| Whisper hallucinating over music or long silences | Voice-activity detection. The matcher ignores what it can't match, and repeated junk is never unique, so it never anchors |
| Wrong pairing (abridged, other edition or translation) | Quick check; coverage threshold; *Needs review* |
| A parser update shifts the text | The text hash triggers a new match from the stored words, in seconds |
| Long off-text stretches using a lot of memory | Streaming slots |
| A heavier build | whisper.cpp builds with CMake and a C++ compiler, which the bundled Opus already needs. Bindings also need libclang (`clang-devel`, `libclang-dev` or `clang`: add these to the README's install lines); `WHISPER_DONT_GENERATE_BINDINGS=1` avoids it. Off for `npm test` |
| Licences | Compatible with an MIT app: `whisper-rs` is Unlicense; whisper.cpp and the Whisper weights are MIT; `symphonia` is MPL-2.0 (file-level copyleft only); `mp4ameta` is MIT/Apache; `rubato` is MIT. Storyteller's `align` package is MIT, but its `ghost-story` dependency is GPL-3.0, so take ideas from it, not code |

## Research notes (checked 2026-11-13)

**Storyteller** (MIT;
[gitlab.com/storyteller-platform/storyteller](https://gitlab.com/storyteller-platform/storyteller))
has two aligners.

- **The older one** transcribes with whisper.cpp (`tiny.en` for English).
  - It finds each EPUB chapter anywhere in the transcript by five-word votes binned per 1,000 words
    (`align/search.ts`).
  - It aligns about 5,000 characters of sentences at a time against the transcript with an
    edit-graph search (`getSentenceRanges.ts`), and interpolates sentences it can't match.
- **The newer one** (`--ctc`, Storyteller 3 beta) is described as more accurate.
  - It computes CTC emissions with MMS-300m through ONNX Runtime.
  - It places chapters by fitting a line through ten-character matches (RANSAC).
  - It force-aligns between unique anchors with a `<star>` token for audio that has no text.
  - It drops text that scores badly as not narrated.
- **Options worth copying:** `--drop-unread` (A7), and `--gap-seconds` (long unmatched audio
  becomes its own entry; Aoide makes it a lead-in instead, A8).
- **Multi-file input:** files are sorted naturally, and cuts move to silence found by VAD.
- **Non-narrated sections** are classified from EPUB structure (much like Aoide's front- and
  back-matter rules).
- **Output:** an EPUB 3 with Media Overlays.

**Whisper.**
- `whisper-rs` 0.16 (Codeberg, Unlicense) bundles whisper.cpp 1.8.3.
- It offers token timestamps, DTW presets, Silero VAD, and the `cuda`/`vulkan`/`hipblas`
  features.
- Model sizes: tiny 75 MiB, base 142 MiB, small 466 MiB.
- WhisperX gets exact word times from wav2vec2 CTC forced alignment: the same idea as Aoide's
  aligner.

**Decoding.**
- `symphonia` 0.6.1: MP4 and AAC need features turned on; there is no Opus; HE-AAC declared in the
  header is refused.
- Seeking is packet-level.
- It doesn't trim MP4 priming and reads no MP4 chapters; `mp4ameta` 0.13 reads both kinds.

**Other approaches.**
- aeneas (AGPL): TTS of the text, then DTW of MFCCs.
- Gentle (MIT): Kaldi biased to the transcript, then a word diff.
- MMS (Pratap et al. 2023): emissions in 15–30 s chunks, `<star>` for audio without text.
- ctc-forced-aligner (BSD-2): a lighter-memory MMS alignment.

**Media Overlays** (EPUB 3.3 §9): SMIL `<par>` elements, each pairing a text fragment with an
audio clip (`clipBegin`/`clipEnd`). Apple supports read-aloud only in fixed-layout books, so
reflowable EPUBs that ship with overlays are rare.
