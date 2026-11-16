# What comes next: feature plans

**Status:** proposed 2026-11-16. Nothing here is built. Written for an implementing agent and for
choosing what to build. Read this file fully, then only the feature you are working on.

These plans came out of a review of what a reading and listening app is expected to do and
Aoide does not do yet. Each feature is planned on its own, so they can be built in any order
unless "Needs" says otherwise. PDF support is being built separately and is not planned here;
[4-library.md](4-library.md) (More formats) builds on it.

## The features

Size is a rough guess: **S** is a day or two, **M** about a week, **L** more than that or with
an open design question.

| # | Feature | File | Size | Needs |
|---|---------|------|------|-------|
| 1.1 | Say one sentence again | [1-narration.md](1-narration.md#11-say-one-sentence-again) | S | |
| 1.2 | Pronunciation list | [1-narration.md](1-narration.md#12-pronunciation-list) | M | |
| 1.3 | Leave chapters out of the narration | [1-narration.md](1-narration.md#13-leave-chapters-out-of-the-narration) | M | |
| 1.4 | Pauses between paragraphs and at scene breaks | [1-narration.md](1-narration.md#14-pauses-between-paragraphs-and-at-scene-breaks) | S | |
| 1.5 | A second voice for dialogue | [1-narration.md](1-narration.md#15-a-second-voice-for-dialogue) | L | |
| 2.1 | Step back a little on resume | [2-listening.md](2-listening.md#21-step-back-a-little-on-resume) | S | |
| 2.2 | Time left in the book | [2-listening.md](2-listening.md#22-time-left-in-the-book) | S | |
| 2.3 | A speed for each book | [2-listening.md](2-listening.md#23-a-speed-for-each-book) | S | |
| 2.4 | A gentler sleep timer | [2-listening.md](2-listening.md#24-a-gentler-sleep-timer) | S | |
| 2.5 | Listen next: a queue | [2-listening.md](2-listening.md#25-listen-next-a-queue) | M | |
| 2.6 | Shorter silences in recordings | [2-listening.md](2-listening.md#26-shorter-silences-in-recordings) | M | |
| 2.7 | More keyboard shortcuts | [2-listening.md](2-listening.md#27-more-keyboard-shortcuts) | S | |
| 3.1 | A menu on selected text | [3-reading.md](3-reading.md#31-a-menu-on-selected-text) | M | |
| 3.2 | Highlights, and exporting notes | [3-reading.md](3-reading.md#32-highlights-and-exporting-notes) | M | 3.1 |
| 3.3 | Reader typography | [3-reading.md](3-reading.md#33-reader-typography) | S | |
| 3.4 | Dictionary | [3-reading.md](3-reading.md#34-dictionary) | L | 3.1 |
| 3.5 | Search the whole library's text | [3-reading.md](3-reading.md#35-search-the-whole-librarys-text) | M | |
| 3.6 | Pictures, tables and code | [3-reading.md](3-reading.md#36-pictures-tables-and-code) | L | |
| 3.7 | Fullscreen and a quiet reader | [3-reading.md](3-reading.md#37-fullscreen-and-a-quiet-reader) | S | |
| 4.1 | Edit a book's details | [4-library.md](4-library.md#41-edit-a-books-details) | M | |
| 4.2 | Series | [4-library.md](4-library.md#42-series) | M | 4.1 |
| 4.3 | Archive, and quicker collections | [4-library.md](4-library.md#43-archive-and-quicker-collections) | S | |
| 4.4 | Goals | [4-library.md](4-library.md#44-goals) | S | |
| 4.5 | More formats | [4-library.md](4-library.md#45-more-formats) | M each | PDF |
| 4.6 | Calibre libraries and OPDS catalogues | [4-library.md](4-library.md#46-calibre-libraries-and-opds-catalogues) | M, L | 4.2 |
| 4.7 | An inbox folder | [4-library.md](4-library.md#47-an-inbox-folder) | S | |
| 4.8 | Backup and moving to another computer | [4-library.md](4-library.md#48-backup-and-moving-to-another-computer) | M | |
| 5.1 | Import from read-later apps | [5-articles-feeds.md](5-articles-feeds.md#51-import-from-read-later-apps) | M | |
| 5.2 | Send a page from the browser | [5-articles-feeds.md](5-articles-feeds.md#52-send-a-page-from-the-browser) | S | 6.1 |
| 5.3 | Feed folders | [5-articles-feeds.md](5-articles-feeds.md#53-feed-folders) | S | |
| 5.4 | Podcasts | [5-articles-feeds.md](5-articles-feeds.md#54-podcasts) | L | |
| 5.5 | Newsletters | [5-articles-feeds.md](5-articles-feeds.md#55-newsletters) | S | |
| 6.1 | Open with Aoide, one running copy | [6-desktop.md](6-desktop.md#61-open-with-aoide-one-running-copy) | M | |
| 6.2 | Notifications | [6-desktop.md](6-desktop.md#62-notifications) | S | |
| 6.3 | Keep working in the background | [6-desktop.md](6-desktop.md#63-keep-working-in-the-background) | M | |
| 6.4 | Stop an idle speech server | [6-desktop.md](6-desktop.md#64-stop-an-idle-speech-server) | S | |
| 6.5 | Releases, Windows and macOS | [6-desktop.md](6-desktop.md#65-releases-windows-and-macos) | L | |
| 6.6 | Listening on a phone | [6-desktop.md](6-desktop.md#66-listening-on-a-phone) | direction only | |

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
