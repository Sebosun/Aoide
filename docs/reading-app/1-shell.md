# Phase 1: the shell

Read [plan.md](plan.md) first. Mockups: `design/Main.dc.html`, `Library.dc.html`,
`Item.dc.html`, `Add.dc.html`.

## Outcome

Every feature that exists today still works, in a shell that no longer assumes listening.
No new kinds of item, no new reading behaviour: the reader still shows the player (phase 2
changes that).

## What exists (start here)

- `src/App.vue` mounts `AppTopBar` (three-way pill: Now Playing, Library, Stats; `EnginePill`
  with `EngineQueue`; settings button), one view by `navigation.view`, `MiniPlayer`, dialogs.
- `src/stores/navigation.ts`: `View = "library" | "reader" | "stats" | "find" | "audiobooks" |
  "book"`; settings and book tools are overlays.
- `src/views/LibraryView.vue`: `ContinueCard`, `WeekCard`, import-problem banner, search, sort,
  filter chips (All, In progress, Ready offline, Preparing, collections), `AddBooksMenu`,
  `BookCard` grid.
- `src/views/BookDetailsView.vue`: cover, "Start listening", "Prepare whole book", stat tiles,
  contents, `RecordingPanel`, `MetadataLookup`.
- `src/views/StatsView.vue`, `FindBooksView.vue`, `FindAudiobooksView.vue`, `ReaderView.vue`.
- Tests in `src/tests/` drive stores and components against `src/api/mock.ts`; `shell.test.ts`,
  `library.test.ts`, `details.test.ts`, `features.test.ts` will change most.

## Core additions

Small, so the views have nothing to derive.

- `BookSummary.audio: AudioSource` (`None | Speech | Recording`): `Recording` when the book's
  narration choice is its recording and the recording is listenable; `Speech` when it has
  cached speech audio, a prepare entry, or listening history with a speech engine; else `None`.
  Rule in a function in `core/extras.rs` or a new `core/facets.rs`, tested in `core/tests.rs`.
- `Core::home() -> HomeView` and command `home`:
  - `continue_ids`: up to 3 items with progress below the end, most recently opened first;
  - `up_next_ids`: up to 5 items with no progress, newest added first;
  - `recent_ids`: up to 8 items by `added_at`, newest first, skipping ones already listed.
  Emitted data is ids only; the UI looks them up in the library store. Refresh on
  `library://changed`.

## Slices

### 1.1 Sidebar and views

- `View` becomes `"home" | "library" | "activity" | "item" | "reader" | "find" | "audiobooks"`.
  `"book"` is renamed `"item"` and `"stats"` `"activity"` everywhere, including `preview.ts`
  params (`view=home|library|activity|item|reader|find|audiobooks`).
- New `src/components/shell/AppSidebar.vue`: wordmark, Add button, Home, Library, Activity, a
  Collections section, and at the bottom the now-playing card and Settings. `AppTopBar.vue` is
  deleted.
- `App.vue`: sidebar plus view for every view except `reader`; in `reader` the sidebar is not
  rendered and the view fills the window. Below 720 px width the sidebar stacks above the view.
- The app opens on Home.
- The now-playing card is `MiniPlayer` restyled for the sidebar: cover, title, state line,
  play/pause. It shows when something is loaded in the player and the reader is not open;
  clicking it opens the reader.
- Test (`shell.test.ts`): each sidebar entry shows its view; opening a book hides the sidebar;
  the back control returns to the view the reader was opened from.
- Done: no reference to `AppTopBar`, "Now Playing" or `view === "stats"` remains in `src/`.

### 1.2 Reader chrome

- `ReaderView` gets a header of its own, as in `ReaderAlong.dc.html`: a back button labelled
  with the view it returns to (Home or Library), the chapter menu in the centre, and on the
  right the Page/Lines switch, Aa, bookmark and a "more" button that opens the existing
  `BookToolsSheet`.
- The player dock stays as it is in this phase.
- `Esc` goes back when no overlay is open; add it to `src/shortcuts.ts` and the shortcuts test.
- Done: every control the old reader had is reachable; `reader.test.ts` and `tools.test.ts`
  pass with selectors updated.

### 1.3 Home

- `src/views/HomeView.vue` from `Main.dc.html`: Continue (cards from `continue_ids`), Up next
  (rows from `up_next_ids`), This week (the existing `WeekCard` data; listening only for now),
  Recently added (covers from `recent_ids`).
- Continue card copy for a book: kind line "Book" plus "· read-along" when `audio` is
  `Recording`; chapter, percent, time left from the existing estimate.
- Empty library: Home shows one panel inviting the first add, with the Add button.
- A section with no entries is not rendered.
- Test (`home.test.ts`): the three lists render the mock's `home` result in order; clicking a
  Continue card opens the reader on that book; an empty `home` shows the invitation.
- Done: `ContinueCard.vue` is deleted or reused, not both.

### 1.4 Library

- `LibraryView` from `Library.dc.html`: title, search, sort, filter chips, grid. Continue and
  week cards leave this view (they are on Home).
- Chips: Unread (no progress), In progress, Can listen (`audio !== "none"`), Preparing (only
  while a prepare queue exists). "Ready offline" is folded into Can listen. The kind switch is
  not built yet (phase 3). "Finished" arrives in phase 2.
- `BookCard`: cover, title, authors, and one status line built from core fields: "Text" /
  "Text + voice" / "Text + recording", then "· unread" or the percent. Preparing and syncing
  states keep their existing progress display.
- Collections move to the sidebar: each is a link that opens Library filtered to it; create,
  rename and delete live in the sidebar section (reuse `useCollections`). The collection chips
  leave the filter row.
- The import-problem banner stays at the top of Library.
- Test (`library.test.ts`): each chip filters by the core field; a sidebar collection opens the
  filtered library; search and sort behave as before.
- Done: the library store derives no state the core already sends.

### 1.5 Item page

- `BookDetailsView.vue` becomes `src/views/ItemView.vue` from `Item.dc.html`.
- Header: cover, kind and read-state line, title, authors, description. Actions: **Read**
  (opens the reader without starting playback), **Listen** (opens the reader and plays from the
  saved position), **Add to collection** (existing menu).
- **Text card**: source format, estimated time (existing estimate), chapters, words, language,
  and the extraction-notes link that opens the existing warnings display.
- **Audio card**, one body per state, all from existing data:
  - nothing set up: "Narrate with a voice" (shows the engine status line; starts playback) and
    "Pair an audiobook" (existing pairing entry points);
  - speech: voice name, prepared amount, "Prepare whole book" / pause / cancel, link to voice
    settings;
  - recording: the content of `RecordingPanel` (stage, progress, report, remove), plus the
    narration switch.
- Contents list with per-chapter prepare control where speech is the source; Details online
  (`MetadataLookup`) below.
- Test (`details.test.ts`): Read opens the reader with the player stopped; Listen opens it
  playing; each Audio card state renders for the matching mock book.
- Done: the words "Start listening" and "Listening time" are gone from this view.

### 1.6 Add dialog

- `src/components/library/AddDialog.vue` from `Add.dc.html`, opened by the sidebar Add button
  and by `Ctrl+N`. It replaces `AddBooksMenu.vue`.
- This phase builds the "Files on this computer" half: drop zone, Choose files (existing
  `choose_books` / `choose_audio`), Choose a folder, Scan for books (opens Find books), Find
  audiobooks (opens that view). The link field is added in phase 4.
- Dropping onto the window keeps working through `DropOverlay` and the existing `dropped()`
  routing in `App.vue`.
- Test: choosing EPUBs imports them and closes the dialog with the existing toasts; choosing
  audio opens pairing as today.
- Done: one component owns "add"; no other view has its own add menu.

### 1.7 Engine status and the narration queue

- `EnginePill` leaves the top of the app. The engine's state appears:
  - in the item page Audio card and the reader's player, as a status line, when the state is
    not `ready`;
  - in the sidebar as a card above the now-playing card while the prepare queue is non-empty
    or the engine is in `error`: "Narrating 1 book", progress, current book. Clicking it opens
    the existing `EngineQueue` panel.
- Test (`queue.test.ts`): the card appears when a book is queued and disappears when the queue
  empties; an engine error shows the card with the error line.
- Done: with an idle, healthy engine and an empty queue, no engine status is on screen outside
  Settings.

### 1.8 Activity and words

- `StatsView.vue` becomes `ActivityView.vue`; heading "Activity". Content unchanged in this
  phase.
- Vocabulary sweep across `src/`: "Continue listening" → "Continue"; "Add EPUB" → "Add";
  "Your books" → "Library". Listening-specific words stay only where audio is the subject.
- Update `README.md` screenshots and wording, `docs/roadmap.md`, and the preview-param comment
  in `src/preview.ts`.
- Done: `grep -rn "Now Playing\|Continue listening\|Add EPUB" src README.md` finds nothing.

## Done when

- All eight slices are committed and every check in plan.md passes.
- Each of Home, Library, item page, reader (Page and Lines), Activity, Find books, Find
  audiobooks, Settings and the Add dialog has been looked at in the preview in light and dark,
  at 1280 px and at 700 px width.
- The native app (`AOIDE_SMOKE_HOME`) imports a book, plays it, prepares it and pairs a
  recording exactly as before the phase.
