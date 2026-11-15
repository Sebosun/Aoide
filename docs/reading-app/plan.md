# Aoide as a reading app: implementation plan

**Status:** proposed 2026-11-15. Written for an implementing agent. Read this file fully, then
only the phase file you are working on.

## Goal

Aoide today is a read-along player: every screen assumes an EPUB that is being listened to. It
becomes a reading app where an **item** is anything you read or listen to:

- a book you read silently, or listen to with a voice or a paired recording;
- an audiobook with no EPUB;
- an article saved from the web;
- articles arriving from RSS/Atom subscriptions.

## Phases, in this order

Finish a phase (its "Done when" list is all true) before opening the next file.

| # | File | Outcome |
|---|------|---------|
| 1 | [1-shell.md](1-shell.md) | Sidebar, Home, Library, item page, Add dialog. Same features, new shell and words. |
| 2 | [2-reading.md](2-reading.md) | Read a book without any audio. Listening becomes a choice per book. Reading position, reading time, finished state. |
| 3 | [3-audiobooks.md](3-audiobooks.md) | An audiobook is an item on its own, with no EPUB. |
| 4 | [4-articles.md](4-articles.md) | Save an article by link: plain HTTP fetch, deterministic extraction, read it like a book. |
| 5 | [5-feeds.md](5-feeds.md) | Subscribe to feeds, read new entries, save the ones worth keeping. |
| 6 | [6-browser.md](6-browser.md) | Optional headless Chrome for pages plain HTTP cannot read; sign-in flow; per-site rules. Off by default. |

## Start condition

`git status` is clean. On 2026-11-15 the "Find audiobooks" work was uncommitted
(`src-tauri/src/core/audiobooks.rs`, `crates/discovery/src/folder.rs`, `names.rs`,
`src/views/FindAudiobooksView.vue`, `src/stores/audiobooks.ts` and edits around them). This plan
builds on that work. If it is still uncommitted, stop and ask the user to commit it.

## How to work: one slice at a time

Each phase file lists **slices**. A slice is one behaviour, visible through a public seam.

1. Write one failing test for the slice's behaviour at the seam the slice names. Run it and see
   it fail for the expected reason.
2. Write the smallest implementation that passes it.
3. Run the checks below. All green is the slice's completion criterion, together with the
   slice's own "Done" line.
4. Commit the slice on the working branch with a message in the style of `git log` (one
   sentence saying what the user or the code gains). Leave pushing to the user.

Checks for every slice:

- `npm run typecheck` and `npm run test:ui`
- `cargo test --workspace --exclude aoide` and `cargo test -p aoide`
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check`
- After any change to a type that crosses IPC, a `CoreEvent` or a command:
  `AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`, then update `src/api/tauri.ts`,
  `src/api/mock.ts` and `src/api/types.ts` to match.
- For a slice that changes a screen: open it in the browser preview (`src/preview.ts` params)
  and look at a screenshot in light and dark. Add a preview param when the state cannot be
  reached with the existing ones.

`AGENTS.md` conventions apply to everything here. The ones this plan leans on hardest:

- **Explicit state.** Anything with states is one enum changed by one transition function over
  named events (model: `src-tauri/src/core/preparation.rs`, `syncing.rs`). Every stage named in
  this plan gets that shape, in its own module, with a table-driven test of the transitions.
- **The core decides.** Views render fields the core computed. When a screen needs "is this
  unread", "what would play", "why did the fetch stop", that is a field on a core view, with the
  rule tested in Rust. The mock (`src/api/mock.ts`) returns the field; it does not re-derive it.
- **Pure crates.** Parsing and protocol code live in `crates/*` with no Tauri, UI or (for
  parsers) network. Each new crate is tested with `cargo test -p <crate>` alone.
- **Synthetic fixtures.** Committed tests use small original HTML, feeds and audio written for
  the test. Real pages and personal files stay outside Git.
- **Vue.** BEM class names, `<style scoped>`, colours only from `src/styles/theme.css` tokens.

New code goes in new modules. `src-tauri/src/core.rs` is already 2,100 lines: add a
`core/<feature>.rs` module that owns its state and exposes methods on `Core`, as
`core/audiobooks.rs` and `core/recordings/` do.

## The model

These words mean the same thing in every phase file, in code and in the interface.

- **Item**: one row of the library. In Rust and SQL it keeps the existing names (`books` table,
  `BookId`, `BookSummary`, `BookView`). Renaming them is out of scope; a doc comment on `BookId`
  says "any library item". The interface says "book", "article" or "audiobook", never "item".
- **Kind** (`Kind { Book, Audiobook, Article }`): what the item is. Stored in `books.kind`.
- **Text**: the item has readable text (an EPUB, or an article snapshot). Audiobooks have none.
- **Audio**: what would play if the user listens: `AudioSource { None, Speech, Recording }`.
  Any text can be narrated by a speech engine, so for a text item this says what is set up, not
  what is possible.
- **Mode** (`Mode { Read, Listen }`): the user's choice for a text item, kept per item. Read
  shows no player. Introduced in phase 2.
- **Shape** (`Shape { Read, ReadAlong, Listen }`): what the reader screen is. Computed by the
  core from text, audio and mode; the reader view switches on it and on nothing else.
  `Read` = text, no player. `ReadAlong` = text with the player. `Listen` = audio only.
- **Read state** (`ReadState { Unread, InProgress, Finished }`): phase 2.
- **Shelf** (`Shelf { Library, Feed }`): whether an item was kept by the user or only opened
  from a feed. Phase 5.
- **Fetch stage**: where an article's fetch is, including `NeedsYou(reason)`. Phase 4, extended
  in phase 6.
- **Snapshot**: the stored HTML an article was extracted from. Extraction is a pure function of
  the snapshot, so it can be re-run without the network.

One position per item. Reading and listening move the same `progress` row, so switching from
reading to listening continues from the same sentence.

## Decisions already taken

Change these before starting the phase they belong to; each is cheap to change now and
expensive later.

| Decision | Chosen | Phase |
|---|---|---|
| Navigation | Left sidebar; the reader takes the whole window | 1 |
| Rust/SQL naming | Keep `Book*` names; add `kind` | 1, 3 |
| Default mode for a book | `Listen` when it has a recording, cached speech or listening history; otherwise `Read` | 2 |
| Position | One shared reading/listening position per item | 2 |
| Reading speed for estimates | Setting, default 230 words per minute | 2 |
| Audiobook identity | Hash of a cheap fingerprint of its files, not of the whole audio | 3 |
| Adding an EPUB to an audiobook | The audiobook item is merged into the imported book | 3 |
| Article identity | Hash of the normalised canonical URL | 4 |
| Article content | Text only: headings, paragraphs, lists, preformatted. Images and tables are left out with a warning | 4 |
| Feeds and the library | Feed entries are not library items until saved | 5 |
| Feed retention | Read entries kept 30 days, unread 90 days, saved ones forever | 5 |
| Podcast feeds | Out of scope | 5 |
| Browser | Off by default; a setting chooses Off, Fallback or First; per-site override | 6 |

## Design reference

The mockups are static HTML in [design/](design/). Read the file for a screen before building
it: the inline styles carry the sizes, spacing, radii and copy.

| File | Screen | Phase |
|---|---|---|
| `Main.dc.html` | Home | 1 |
| `Library.dc.html` | Library | 1 (kind switch in 3, articles in 4, "needs you" marker in 4) |
| `Item.dc.html` | Item page | 1 |
| `Add.dc.html` | Add dialog | 1 (link field in 4, feed offer in 5) |
| `ReaderText.dc.html` | Reader, shape `Read` | 2 (article header in 4) |
| `ReaderAlong.dc.html` | Reader, shape `ReadAlong` | 2 |
| `ReaderAudio.dc.html` | Reader, shape `Listen` | 3 |
| `Feeds.dc.html` | Feeds | 5 |
| `NeedsYou.dc.html` | Article that needs you | 4 (sign-in action in 6) |
| `SignIn.dc.html` | Sign-in dialog | 6 |
| `Sites.dc.html` | Settings, Sites tab | 6 |

How to read them:

- They are pictures, not components. Build Vue components with BEM classes; reuse existing
  components (`CoverArt`, `UiMenu`, `UiSelect`, `UiDialog`, `PlayerTransport`, ...) wherever one
  already does the job.
- Map every hex colour to the theme token with the same role (`#E9ECF7` is `--ground`,
  `#121638` `--ink`, `#575E88` `--dim`, `#CBD1EA` `--line`, `#DDE3F8` `--wash`, `#C9D1F1`
  `--hero`, `#262B4F` `--reading`). The mockups are light only; dark comes from the tokens.
- The mockups use `#C9431F` for the accent because white text on the current `--accent`
  (`#E0502F`) is below 4.5:1. Keep `--accent` unless the user asks to change it.
- `#17754A` and `#8A520B` are `--ok` and `--warn` darkened for text; use the tokens.
- A token the mockups need and the theme lacks (the warm notice background `#FBF1DE` in
  `NeedsYou`) is added to `theme.css` in both themes, never written into a component.
- Sample titles, sites and numbers in the mockups are placeholders. The mock API keeps its own
  synthetic data.

## Schema ledger

`crates/library/src/store.rs` is at version 6. Each migration gets a `library_vN.rs` test that
opens a database at the previous version with rows in it and checks they survive.

| Version | Phase | Change |
|---|---|---|
| 7 | 2 | `books.finished_at`, `books.word_count`; table `reading(book_id, day, ms)` |
| 8 | 3 | `books.kind` (`book`, `audiobook`, `article`; default `book`); `progress.position_ms`, `bookmarks.position_ms` |
| 9 | 4 | table `articles` |
| 10 | 5 | `books.shelf` (`library`, `feed`; default `library`); tables `feeds`, `feed_entries` |
| 11 | 6 | table `sites` |

`ALTER TABLE ... ADD COLUMN` with a `CHECK` and a default covers every column change; no table
is rebuilt.

## Third-party crates to evaluate

Named from memory on 2026-11-15, not verified. Before adding one, check on crates.io and its
repository that it is maintained, its licence fits MIT distribution, and its API does what the
slice needs. Wrap each behind an Aoide type so it can be replaced.

| Need | Candidates |
|---|---|
| HTML parsing | `scraper` (html5ever) |
| Readable-content extraction | `dom_smoothie`, `readability` |
| Charset decoding | `encoding_rs` |
| URL handling | `url` |
| RSS, Atom, JSON Feed | `feed-rs` |
| Driving Chrome | `chromiumoxide` |

## Out of scope for the whole plan

- Renaming `Book*` types, the `books` table or the `aoide-epub` crate.
- Images, tables, video and comments in articles.
- PDF, MOBI and other book formats.
- Podcasts and audio enclosures.
- Sync between devices, accounts, a browser extension.
- Reading an audiobook's speech-recognition transcript as its text.

## When something does not fit

The plan was written from a reading of the code on 2026-11-15, not from running it. Where the
code disagrees with the plan, the code is right: keep the slice's behaviour and its test, change
the route, and note the difference in the phase file under a "Notes from implementation"
heading. Where a slice turns out to need a decision the plan did not take, pick the option that
keeps the phase's "Done when" list true and record it in the same place.
