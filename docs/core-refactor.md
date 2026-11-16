# Core refactor

Turns `Core` (`src-tauri/src/core.rs`) from a god object into wiring between **components**, and
closes the gaps an architectural review found around it. Work the steps in order, one per session.

## How to work a step

1. Start from a clean `git status`. If it is dirty, stop and tell the user.
2. Take the first unticked step in [Steps](#steps). Read only that step and the reference
   sections it names.
3. Every step except 5 is **behaviour-preserving**: the UI contract and stored data stay as they
   are unless the step says otherwise. Where a step names characterisation tests, write them
   first and see them pass against the old code.
4. The step is done when its **Done when** holds and `cargo test -p aoide`, `npm run check`,
   `npm run typecheck` and `npm run test:ui` all pass. Tick the box, then stop: the next step
   belongs to the next session.

Project conventions (TDD, state machines, contract regeneration) are in `AGENTS.md` and apply
throughout.

## Component

A **component** is the unit this refactor produces. `Recordings` (`core/recordings/`, with its
`SyncHost` trait) and `Playback` (`core/playback.rs`) are the models to copy.

- It lives in its own module under `core/` with private fields. Sibling modules reach its state
  only through its methods. (`Core`'s own fields are visible to every `core::*` submodule, which
  is why state must move off `Core` to be protected.)
- It owns its mutexes. A method computes under the lock, returns a value, and the caller acts on
  that value after the lock is released. `Core::check_voices` already has this shape.
- What it needs from the rest of the app arrives through a host trait that `Core` implements, or
  as arguments.
- `library` (`Arc<Mutex<Library>>`) is the one shared lock. It is the **leaf lock**: taken last,
  released before any call out.
- Its tests sit beside it and drive it without a `Core`.

`Core` ends as: construct components, spawn loops, implement host traits, forward commands.

## Steps

### - [x] 1. Run the core's tests and lints by default

`package.json` scripts `test` and `check` pass `--exclude aoide`, so the app crate is skipped.

**Done when:** a `test:core` script runs `cargo test -p aoide`; `check` formats and lints the
whole workspace including `aoide` with `-D warnings`; both pass.

### - [x] 2. Move initial UI state out of the mock

`src/stores/player.ts` imports `initialPlayer` and `initialGeneration` from `src/api/mock.ts`.

**Done when:** both live in `src/api/initial.ts`; no file outside `src/api/index.ts`,
`src/preview.ts` and `src/tests/` imports `api/mock`.

### - [x] 3. Merge app-settings changes in the core

Four UI sites read all of `AppSettings`, spread one change into it and save the whole struct:
`stores/library.ts` (`setSort`), `composables/useAppSettings.ts`, `views/FindBooksView.vue`,
`views/FindAudiobooksView.vue`. Two of them racing lose a change.

Add a command `update_app_settings(patch)` whose patch type has every `AppSettings` field
optional. The core merges it into the current settings while holding the config lock, then runs
the existing validation and side effects of `save_app_settings`. Return the merged settings.

**Done when:** a core test shows two patches to different fields both survive; the four sites
call the new command; `save_app_settings` has no remaining UI caller and is removed from the
contract and the mock.

### - [x] 4. One error type at the IPC boundary

About 170 signatures in `src-tauri` return `Result<_, String>`; the UI cannot tell error kinds
apart and background failures go only to stderr.

- Add `core/error.rs`:
  ```rust
  #[serde(tag = "kind", rename_all = "camelCase")]
  pub enum CoreError {
      NotFound { what: String },
      Invalid { field: String, message: String },
      EngineUnavailable { message: String },
      Busy { message: String },
      Io { message: String },
      Internal { message: String },
  }
  ```
  with `Display` giving the message, `From` for each workspace crate's error type, and
  `From<String>` / `From<&str>` mapping to `Internal` so modules migrate one at a time.
- Commands in `lib.rs` return `Result<_, CoreError>`. Export the type through the contract.
- In `src/api/tauri.ts`, `call` rethrows a rejection as an `ApiError` carrying `kind`, whose
  `toString()` is the message, so existing `String(e)` sites keep showing the same text.
- Add `Core::report(context, error)`: logs, and for failures the user should know about emits a
  new `CoreEvent::Problem` that the UI shows in the toast stack. Route every `eprintln!` in
  `src-tauri/src` through it.
- Persistence writes whose result is discarded with `let _ =` (the `set_setting` calls in
  `core.rs`, `core/recording.rs`, `core/preparation.rs`) go through `report`. File-system
  cleanup may stay best-effort.

**Done when:** every `#[tauri::command]` returns `CoreError`; `grep -rn 'eprintln!' src-tauri/src`
finds only the body of `report`; a UI test shows a rejected command reaching a store as an
`ApiError` with its `kind`; a core test shows a failed background save emitting `Problem`.

### - [x] 5. Typed keys and a per-book table

The `settings` key-value table holds per-book values under formatted keys
(`book.{id}.settings`, `progress.fraction.{id}`, the per-book prepare keys in
`core/preparation.rs`, the narration key in `core/recording.rs`). `Core::remove_book` deletes the
`books` row and leaves these behind; the library crate has no way to delete a setting.

**Ask the user first:** should a book that is removed and added again keep its voice settings
and progress? The answer decides which keys cascade. Record the answer as a comment on the
table's migration.

*Answered 2026-11-15:* keep both. Voice settings (`BookKey::Settings`, which also holds the
narration choice) and progress stay in `settings` and survive removal; the prepared mark
(`BookKey::Prepared`) moves to `book_values` and cascades.

- Migration v7 in `crates/library/src/store.rs`: table
  `book_values(book_id REFERENCES books ON DELETE CASCADE, key, value, PRIMARY KEY(book_id, key))`;
  move the existing per-book rows into it. Keys the user wants to survive removal stay in
  `settings`.
- In the library crate: `enum BookKey` and `enum GlobalKey` naming every key, with
  `get<T: DeserializeOwned>` / `set<T: Serialize>` / `remove`. A value that fails to parse is an
  error the caller passes to `report`.
- `src-tauri` reaches stored values only through those enums.

**Done when:** a library test migrates a v6 database holding each per-book key and reads every
value back through `BookKey`; a test shows removing a book removes its cascading values;
`grep -rn 'format!("\(book\|progress\|prepare\)\.' src-tauri/src` finds nothing;
`Library::setting` and `set_setting` are private or gone.

### - [x] 6. Relocate wire types and split `extras.rs`

Pure moves, no logic changes.

- The view and wire types at the top of `core.rs` (the `// wire types` section) and `CoreEvent`
  move to `core/wire.rs`.
- `core/extras.rs` becomes `core/bookmarks.rs`, `core/listening.rs`, `core/settings.rs`; the
  cache limit joins `core/storage.rs`.

**Done when:** `extras.rs` is gone; `core.rs` declares no `Serialize` type; the regenerated
`src/api/contract.ts` is byte-identical to before.

### - [x] 7. Extract the `Engine` component

Moves off `Core`: fields `engine`, `registry`, `last_engine_status`; `ActiveEngine`, `VoiceList`;
`check_voices`, `ensure_voice`, `ensure_model`, `engine_loop`, `launch_engine`,
`emit_engine_status`, `engine_status`, and the engine half of `activate_profile` and `shutdown`.

Characterisation tests first, at `Core`'s public seam with the `FakeEngine` from `core/tests.rs`:

- switching profile while a voice listing is in flight leaves the new engine's voices unasked
  and the old listing ignored;
- an engine that is ready and lists no voices has saved voice names used as they are;
- an offline engine with a launch command is launched once (since 6.4 of `docs/next`: once speech
  is wanted, by the `ServerProcess` component in `core/engine_process.rs`, which also notices
  the exit, restarts after a crash and stops it when idle);
- `engine://status` is emitted only when the status changes.

**Done when:** those tests pass before and after; `Core` has one `engine` field of the component
type; `core.rs` contains no `lock(&self.engine)`; the tests now also exist beside the component,
driving it without a `Core`.

### - [x] 8. Extract the `Generation` component

This is the step that removes "remember to call `replan`". Today `replan` is called from 17
sites, `emit_generation` from 5, across `core.rs` and `core/recording.rs`.

Moves off `Core`: fields `worker`, `worker_events`, `store`, `preparation`, `generation`;
`replan`, `plan_waiting_books`, `replan_with_voices`, `emit_generation`, the worker-event
handling, `Store`.

The component has one mutating entry point, in the style of `core/preparation.rs`:

```rust
enum PlanEvent {
    ListeningChanged { jump: bool },
    BookQueued(String), BookForgotten(String), PreparePaused { book: String, paused: bool },
    EngineSwitched(Arc<dyn SpeechEngine>), VoicesKnown,
    AudioLost(HashSet<String>), SegmentMade { key: String, duration_ms: u32 },
}
fn apply(&self, event: PlanEvent, host: &dyn GenerationHost)
```

`apply` transitions, replans, and emits `generation://state` when the view changed. The host
supplies what is being listened to (book, narration plan, playhead). Extend the event list to
cover every existing `replan` caller; each caller becomes one `apply`.

Characterisation tests first: one per event, asserting the work the engine is then asked for.

**Done when:** `grep -rn '\.replan(\|\.emit_generation(' src-tauri/src` finds nothing outside the
component; every `PlanEvent` variant has a test; `Core::activate_profile` is an engine switch
followed by one `apply`.

### - [x] 9. Make progress saving a return value

`save_progress` is called from 7 sites after playback changes.

`Playback::pause`, `stop` and `seek` return a `#[must_use]` `Option<Place>`; one `Core` method
persists a `Place`. The periodic save in the tick loop stays.

**Done when:** `save_progress(true)` appears only inside that one method and `shutdown`; a test
shows pause, stop and seek each persisting the place.

### - [x] 10. Extract the `Shelf` component

Moves off `Core`: field `books`, the ignored list (`ignored`, `set_ignored`), `describe`,
`rescan`, `watch_library`, `import_files`, `load_book`, `summary`, `list_books`, and the library
half of `remove_book`. `Core::remove_book` becomes an ordered call across components: playback,
generation (`BookForgotten`), recordings, shelf.

**Done when:** `core.rs` contains no `lock(&self.books)`; the shelf has tests beside it for
import, rescan finding a removed file, and an ignored book staying out of a rescan.

### - [x] 11. Close out `Core`

- Sleep timer and media keys move into a module beside `Playback`.
- `lock()` logs through `report` the first time it recovers a poisoned mutex.
- Replace the lock-order comments on `Core`'s fields with the leaf-lock rule, stated once in
  the module doc.
- Add to `AGENTS.md`: new core features are components (own module, own locks, host trait),
  with a pointer to this file's [Component](#component) section.

**Done when:** `core.rs` is under 600 lines and every `impl Core` block outside it contains only
command forwarding or a host-trait implementation.

### - [x] 12. Mock fixtures generated from the core

`src/api/mock.ts` hand-builds views and re-implements core rules, so UI tests check the mock.

Add a core test, gated on an environment variable like `AOIDE_WRITE_CONTRACT`, that runs `Core`
with `FakeEngine` over the synthetic EPUB through a few scenarios (fresh library, playing,
preparing, recording synced) and writes the resulting `BookSummary`, `BookView`, `PlayerState`,
`GenerationState` and `RecordingView` as JSON under `src/tests/fixtures/`. The mock serves those
fixtures and keeps only scripted event sequences and in-memory edits.

**Done when:** the mock constructs none of those five types by hand; the fixture command is
documented next to the contract command in `AGENTS.md`; `npm run test:ui` passes.

### - [x] 13. Formatter for the UI (ask the user first)

No formatter is configured and the stores, composables and API files have many lines over 160
characters. This is a taste call: ask whether the dense style is deliberate before doing
anything. If the user wants it: Prettier at `printWidth: 120`, one formatting-only commit, a
format check added to `check`.

*Answered 2026-11-15:* add Prettier at 120.

**Done when:** the user has declined, or the format check passes.
