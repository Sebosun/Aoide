# 6. The desktop: being a good application

Read [plan.md](plan.md) first.

Checked against the code on 2026-11-16: the "Check" items are answered in place, and decided
questions are under **Settled**. 6.4 is being redesigned separately and was not checked here.

The Tauri side is `src-tauri/src/lib.rs` (one window labelled `main`, the dialog plugin, the
command list, `core.shutdown()` on `RunEvent::Exit`) and `src-tauri/tauri.conf.json`
(`bundle.active` is false: Aoide is built from source and run from `target/release/aoide`).
`tauri-plugin-opener` is a dependency but not a registered plugin: `core/environment.rs` calls
its functions (`reveal_item_in_dir`, `open_url`) directly. `src-tauri/capabilities/default.json`
grants `core:default` and webview zoom only. Aoide reads no command-line arguments today (only
`AOIDE_*` environment variables in debug smoke runs, `src-tauri/src/smoke.rs`), has no
`on_window_event` handler, and closing the window ends the process.

---

## 6.1 Open with Aoide, one running copy

**Size:** M. 5.2 needs it; file associations in 6.5 need it.

### Outcome

Double-click an EPUB in the file manager and it opens in Aoide, in the window that is already
running if there is one. `aoide book.epub` and `aoide https://…` on the command line do the
same.

### What exists

- `Shelf::import_files(paths) -> Vec<ImportResult>` (`core/shelf.rs:325`, behind the
  `import_files` command) takes EPUB and PDF files (`aoide_library::is_book_file`, by
  extension; `BOOK_EXTENSIONS` in `crates/library/src/lib.rs`), refuses a PDF with no text,
  copies a file from outside the library folder into it, rescans, and answers with each
  file's `book_id` (the content hash). A file already inside the library folder is not
  copied. A book the user removed earlier is taken off the ignored list, so opening it brings
  it back.
- **It does copy a book that is already in the library under another file name.** The only
  duplicate check is "the target file name already holds the same content"; opening
  `~/Downloads/x.epub` when the library has the same book as `Books/y.epub` writes
  `Books/x.epub`. The rescan then lists one book (the id is the content hash) with two files on
  disk.
- **The interface navigates by itself.** `src/stores/navigation.ts` `openItem(id)` sets the
  view to `item`; no core event asks the window to show an item. Dropped files are routed in
  `App.vue` (`dropped`): EPUBs and PDFs go to `importFiles` and the library view, audio to
  `recordings.receive`. A link pasted or dropped on the window opens the Add dialog filled in
  (`navigation.openAdd(link)`); nothing saves a link without the user.
- Audio files (an M4B, a folder of MP3s) are added through `add_audiobook`, a separate path.
- `zbus` 5 is already a Linux dependency (MPRIS, `src-tauri/src/media/mpris.rs`); the
  single-instance plugin uses D-Bus on Linux too.
- Smoke runs (`AOIDE_SMOKE_HOME`, `lib.rs` `smoke_home`) use a throwaway library and are
  started while a real Aoide may be running.

### Design

**Pure rule first: `src-tauri/src/launch.rs`.**

```
pub enum Request { OpenFile(PathBuf), SaveLink(Url), Show }
pub fn requests(args: &[String], cwd: &Path) -> Vec<Request>
```

- `args` are the arguments after the program name. Both callers (`std::env::args()` at a cold
  start, the single-instance callback's `argv`) include `argv[0]`; the caller drops it, and a
  test says so.
- An argument that is an existing file for which `aoide_library::is_book_file` holds (EPUB,
  PDF) is `OpenFile`: relative paths resolved against `cwd`, `file://` addresses accepted and
  percent-decoded (`Url::to_file_path`; GNOME Files passes `file://` addresses for `%U`).
  Audio files are ignored and logged: adding an audiobook stays in the Add dialog.
- `http(s)://…` and `aoide://save?url=…` are `SaveLink` (validated as in 5.2).
- Anything else is ignored and logged. No arguments at all is `Show`.
- Flags are not interpreted: an argument starting with `-` is ignored, so nothing a web page
  or a file name supplies can become an option. The one exception is the desktop-file writer
  below, which `main` handles before `requests` runs and only as the sole argument.

**One running copy.** `tauri-plugin-single-instance`, registered first in `run()`: a second
launch hands its arguments and working directory to the first and exits; the first runs
`requests` on them, shows, unminimises and focuses its window and acts. It needs no
capability (it has no JavaScript side). It is not registered in a smoke run
(`AOIDE_SMOKE_HOME`), which uses its own library and must not be handed to a running Aoide. A
debug build keeps it: it shares the real library with a release build, which is the "two
writers" this prevents.

**The address scheme, without a deep-link plugin.** On Linux an `aoide:` address reaches the
program as a command-line argument: the desktop file's `Exec=… %U` line passes it, a running
copy gets it through the single-instance callback, and `requests` already parses it.
`tauri-plugin-deep-link` adds nothing on Linux and is needed only on macOS (out of scope with
6.5), so it is not added. The desktop file
(`MimeType=x-scheme-handler/aoide;application/epub+zip;application/pdf;`) exists only once
Aoide is installed (6.5). For a build from source, `aoide --install-desktop-file` writes
`~/.local/share/applications/aoide.desktop` with `Exec=<current_exe> %U`, copies the icon
(`include_bytes!` of `src-tauri/icons/icon.png`) to
`~/.local/share/icons/hicolor/512x512/apps/aoide.png`, and runs `update-desktop-database` and
`xdg-mime default aoide.desktop x-scheme-handler/aoide` (each logged; a missing tool is
reported, not fatal). The README documents it. A flag on the binary rather than a script,
because the binary knows its own path.

**Acting on a request** (a component `core/launching.rs`; its host is the shelf's import and
the event sink):

- `OpenFile`: first look the content hash up in the library (`Library::book`). A book that is
  already there (and not an error record) is opened without copying anything; this check is
  new, `import_files` alone would copy it again under the new name. Otherwise
  `Shelf::import_files(vec![path])` on a blocking thread, and its `book_id` is opened. A failed
  import shows as a problem (`CoreEvent::Problem`), the same as a failed drop. With several
  files, all are imported and the first one's page opens.
- Opening is a new event, `CoreEvent::OpenItem(String)` (`"item://open"`, payload the book
  id). `App.vue` listens and calls `navigation.openItem(id)`, closing the Add dialog and
  leaving the reader as the existing `openItem` callers do. 6.2 reuses it.
- `SaveLink`: 5.2. Until 5.2 is built it is ignored and logged.
- Requests that arrive before the interface is ready are held: `launching` keeps the item ids
  it has not delivered, and a new command `take_open_requests() -> Vec<String>` (called once
  from `App.vue` `onMounted`, after the listener is attached) returns and clears them and marks
  the window ready; from then on `OpenItem` is emitted directly. So a cold start with a file
  still opens it. The mock answers `[]`.

**Capabilities.** None: the single-instance plugin has no JavaScript side, and the new event
and command go through the existing `core:default` and the generated contract.

### Slices

1. `requests`: files, relative paths, `file://` (percent-encoded), links, `aoide://`, flags
   ignored, unsupported and audio files ignored, `argv[0]` not passed.
2. A request to open a file already in the library under another name opens it without a
   second copy. Seam: core test (`core/launching.rs`, a fixture EPUB written twice).
3. A new file is imported, then `OpenItem` is emitted with its id; a file that cannot be
   imported reports a problem and opens nothing.
4. Requests before the interface is ready are held and returned by `take_open_requests`.
   Contract and mock.
5. Single instance wired in `lib.rs`, skipped for smoke runs; a second launch shows the window
   (by hand: the plugin is not testable in unit tests).
6. `--install-desktop-file` and the README.

### Logging

`launch.request` (`kind`, `outcome`, `file` name only), `launch.second_instance`,
`launch.desktop_file` (`outcome`).

### Done when

- "Open with → Aoide" works from GNOME Files and Dolphin after the desktop file is installed.
- Starting Aoide twice never gives two windows or two writers on the library.
- Opening a book that is already in the library adds no file to the library folder.

---

## 6.2 Notifications

**Size:** S

### Outcome

When a long job ends while Aoide is not in front, the desktop says so: a book is prepared, an
audiobook is synced, an export is written, or a job needs you.

### What exists

- **Preparation** (`core/preparation.rs`): stages `NotPrepared, Waiting, Queued, Preparing,
  Paused, Done`, changed by `next` and applied in one place, `apply(entry, event)` (which logs
  `prep.stage`). A book reaches `Done` in `Preparation::finish_complete`, called from
  `planned` and `segment_made`; `segment_made` returns only whether *some* book finished
  (`bool`), and `Generation` (`core/generation.rs:317`) uses it to emit `LibraryChanged`.
  **There is no failed stage**: when the engine fails, `PlanEvent::WorkFailed` sets
  `GenerationStateView.error` and the book stays `Preparing`, waiting. Preparation keeps no
  start time. Preparation covers whole books and chosen chapters (`Scope`).
- **Sync** (`core/syncing.rs`, applied in `Recordings::sync_event`,
  `core/recordings/mod.rs:352`, which logs `sync.stage`). A long job ends when the stage leaves
  `Transcribing` or `Matching` for `Ready`, `PoorMatch`, `SampleMismatch` or `Failed`;
  `PoorMatch` and `SampleMismatch` wait for the user. Every start re-checks each recording
  (`Checking → Ready`), which must not notify. `live.pace_from` holds when transcribing last
  started.
- **Export** (`core/export.rs`): `Exports::audiobook` and `read_along` take `started =
  Instant::now()` and end in `Encoder::finished(id, &result)`, which emits
  `ExportProgress { stage: Done | Error }`. An M4B export first runs whole-book preparation
  through `ExportHost::prepare`, so that book also reaches preparation's `Done`.
- **Finding books**: `Discovery::import` (`core/discovery.rs:568`) imports chosen books and
  ends with `ImportProgress { done: total }`; it keeps no start time.
- `NeedsYouNotice.vue` is not a job: it is the article page's note that a fetch got only part
  of the text. Nothing about it should notify.
- In-window feedback: `ToastStack.vue`, and `CoreEvent::Problem` for background failures.
- Backups (4.8) do not exist yet.
- The core cannot tell whether the window is focused: `lib.rs` has no window-event handler.
- `tauri-plugin-notification` has no click callback on desktop: its actions and `onAction`
  are mobile-only (plugin documentation). `notify-rust` on Linux speaks the
  freedesktop notification protocol over D-Bus (`zbus`, already used for MPRIS) and can wait
  for the `default` action, which is a click on the notification.

### Design

**Pure rule: `core/notices.rs`.**

```
pub enum Happened {
    Prepared(BookId), PreparationStalled(BookId),
    Synced(BookId), SyncNeedsYou(BookId), SyncFailed(BookId),
    Exported(BookId), ExportFailed(BookId),
    ImportFinished { added: u32 },
}
pub fn notice(happened: &Happened, took: Duration, focused: bool, settings: &NoticeSettings)
    -> Option<Notice>
pub struct Notice { pub title: String, pub body: String, pub opens: Option<BookId> }
```

- Nothing when the window is focused (the toast is enough). A hidden window (6.3) is not
  focused.
- Nothing for things the user did a moment ago: only jobs that ran longer than a minute
  (`took`), except the ones that need the user (`PreparationStalled`, `SyncNeedsYou`,
  `SyncFailed`, `ExportFailed`), which notify whenever the window is not focused.
- Feed arrivals never notify. A reading app that pings is a worse reading app.
- Text carries the item's title, which is the user's own data shown on their own desktop; it
  is not logged.

**Where each `Happened` comes from** (each at the place its transition is applied):

- `Prepared`: `finish_complete` returns the ids that finished (instead of `bool`; the
  `LibraryChanged` emit keys on `!is_empty()`), and `Entry` gains `since: Option<Instant>`,
  set when it is first promoted to `Preparing` in this run. Not sent while that book is being
  exported (`Exports::exporting() == Some(id)`): the export's own notice follows.
- `PreparationStalled`: replaces the plan's `PrepareFailed`, which has no stage to come from.
  Sent on the first `WorkFailed` while a book is `Preparing`, and not again until a segment is
  made.
- `Synced`, `SyncNeedsYou`, `SyncFailed`: in `sync_event`, only when `before` is
  `Transcribing` or `Matching`, so the start-up re-check never notifies. `took` from when the
  job's first `Transcribing` began.
- `Exported`, `ExportFailed`: in `audiobook` and `read_along` beside `finished`, `took` from
  `started`.
- `ImportFinished`: at the end of `Discovery::import`, which takes a start time. Imports through
  `import_files` (drops, 6.1) are immediate and do not notify.
- `BackupFailed` is left to 4.8, which adds it when backups exist.

The components reach the rule through their host traits (`GenerationHost`, `SyncHost`, the
export's and discovery's hosts gain `fn happened(&self, happened: Happened, took: Duration)`);
`Core` implements it by calling `notices`.

**Sending.** A host trait `Notifier { fn focused(&self) -> bool; fn send(&self, notice:
Notice) -> Result<(), String>; }` in `Environment`, so the rule and the wiring are tested with
a recording notifier. The desktop one reads `is_focused()` of the `main` window and sends with
`notify-rust` (Linux; elsewhere it does nothing until 6.5), on a thread that waits for the
`default` action. Clicking shows, unminimises and focuses the window and emits 6.1's
`CoreEvent::OpenItem`. Where the desktop has no notification service the send fails quietly
and is logged once per run.

**Settings.** `AppSettings.notices: bool`, on by default, one switch (with its
`AppSettingsPatch` field and `From` arm in `core/settings.rs`). Per-kind switches only if
someone asks. Shown in Settings › Library beside **Carry on preparing**.

### Slices

1. `notice`: each `Happened`, focused and not, short jobs silent, jobs that need the user not.
2. Preparation: `finish_complete` returns the ids; a book prepared over a minute, unfocused,
   gives one notice; one being exported gives none. Seam: `Preparation` with an injected
   clock, then a core test with the recording notifier.
3. Sync, export and import wiring; the start-up re-check gives none.
4. The desktop notifier and the click (by hand).
5. The setting.

### Logging

`notice.sent` (`kind`, `book`, `outcome`).

### Done when

- Preparing a book and switching to another application ends with one notification.
- Exporting an M4B of an unprepared book ends with one notification, not two.

---

## 6.3 Keep working in the background

**Size:** M

### Outcome

Close the window while a book is being prepared or an audiobook synced and Aoide offers to keep
working. A tray icon shows it is still there, with play and pause, and **Quit when done**.

### What exists

- Closing the window ends the process (Tauri's default; `lib.rs` has no window-event handler)
  and `RunEvent::Exit` runs `core.shutdown()` (saves the place, closes the output, stops the
  engine, the aligner and the browser). `AppSettings.resume_prepare` (`core/settings.rs`, on
  by default) carries on queued preparation at the next start (`core.rs:388`).
- Every job runs in the core, not in the window: generation and preparation
  (`core/generation.rs`), the sync runner, exports, imports, and the player (an output process,
  `core/output.rs`). A hidden window does not stop them.
- What "working" can be read from: `Preparation::current()` (the book `Preparing`, with
  done and total), recordings' stages, `Exports::exporting()`, the player's status. A
  discovery import keeps no flag of its own. Model and browser downloads emit
  `ModelDownload` and `BrowserDownload`.
- The sidebar's engine card (`EngineQueue.vue`) works its list out in the interface from
  `library.books` and the recordings store; Rust cannot use it.
- **The tray library is loaded at run time, not linked.** Tauri's `tray-icon` feature (tauri
  2.12.5 pulls `tray-icon` 0.25.1 with its `libappindicator` backend; neither is built today)
  needs nothing more at build time than GTK. `libappindicator-sys` 0.9 opens
  `libayatana-appindicator3.so.1` or `libappindicator3.so.1` with `libloading` the first time a
  tray is made and **panics** when neither is there. The README's build steps already list the
  appindicator packages (from Tauri's prerequisites).
- A library that loads is not a tray that shows: GNOME without the AppIndicator extension
  (Fedora's default) has no `org.kde.StatusNotifierWatcher` on the session bus, and the icon
  is simply not shown. On Linux the tray icon has no click event: everything is in its menu.
- MPRIS (`src-tauri/src/media/mpris.rs`) answers `CanRaise = false` and has an empty `raise`.
- Settings tabs (`SettingsTab` in `src/stores/navigation.ts`): appearance, engine, recordings,
  library, feeds, sites, highlighting, storage, about. No General tab; **Carry on preparing**
  is in `LibrarySettings.vue`.

### Design

**An explicit state, `core/presence.rs`:**

```
enum Presence { Window, Background, QuittingWhenDone }
enum Event { CloseRequested { working: bool }, Reopened, QuitChosen, QuitWhenDoneChosen, WorkEnded }
fn next(presence: Presence, event: Event, behaviour: CloseBehaviour, tray: bool) -> (Presence, Action)
enum CloseBehaviour { Ask, Quit, KeepWhileWorking, KeepAlways }
enum Action { None, HideWindow, MinimiseWindow, ShowWindow, AskUser, Quit }
```

- "Working" is a core view, `Core::working() -> Working`: a book is `Preparing` while the
  engine is not in error, a recording is `Waiting`, `Transcribing` or `Matching`, an export or
  a discovery import is running (the import gains a flag), a model or the browser is
  downloading, or audio is playing. A preparation stalled on a failed engine is not working,
  so **Quit when done** does not wait on it forever.
- **Without a tray, a hidden window would be unreachable.** When no tray is shown, "keep"
  minimises instead of hiding. `tray` is detected at start and is an input to the rule: the
  appindicator library opens (checked with `libloading` against the same two names *before*
  any tray is made, so a missing library can never panic) **and** the session bus has an owner
  for `org.kde.StatusNotifierWatcher` (`zbus`). Either missing: no tray. With 6.1, launching
  Aoide again also shows a hidden window.
- The first close while working asks once ("Keep working in the background?" with
  *Remember*). Tauri's `WindowEvent::CloseRequested` is handled in `lib.rs` with
  `api.prevent_close()` whenever the rule's action is not `Quit`. The question is the window's
  own dialog (the native one from `tauri-plugin-dialog` has no *Remember*): a new event
  `CoreEvent::CloseAsked` and a command `choose_close(keep: bool, remember: bool)`.
- `Quit` is `app.exit(0)`, so `RunEvent::Exit` still runs `core.shutdown()`.
- Background feed checks alone are not "working": Aoide does not stay alive just to poll
  feeds, unless `KeepAlways` is chosen.
- `WorkEnded`: after each event the tray refresh sees (below), the core reads `working()`
  again; false while `QuittingWhenDone` is `WorkEnded`.

**Tray menu.** The playing item with play or pause; what is being worked on, with progress;
Open Aoide; Quit when done; Quit. Built once in Rust (the AppIndicator backend cannot replace
a menu, only change its items' text and enabled state) and refreshed from core views. The
refresh comes from the core's own events, not the sidebar's: `Environment::desktop` wraps its
`Events` sink so `PlayerState`, `GenerationState`, `SyncState`, `ExportProgress`,
`DiscoveryImport` and `LibraryChanged` also reach the tray, which changes an item's text only
when it differs (`PlayerState` arrives for every word).

**MPRIS raise.** `CanRaise` becomes true and `Raise` shows the window, so GNOME's media
controls bring Aoide back while it plays, tray or not.

**With 6.1.** A second launch while in the background shows the window (`Reopened`).

**Build.** No Cargo feature: the tray needs nothing at build time, and the run-time check
above decides. The README's build steps already have the packages; add a line that the tray
needs the appindicator library at run time and, on GNOME, the AppIndicator extension.

**Settings.** `AppSettings.close: CloseBehaviour`, default `Ask` (with its `AppSettingsPatch`
field and `From` arm), in Settings › Library beside **Carry on preparing**.

### Slices

1. The transition table, including "no tray".
2. `Core::working()`: each kind of job, and a stalled preparation not counting. Seam: core
   test.
3. Close while preparing keeps the core running and hides or minimises the window; closing
   while idle quits. Seam: the rule; the window calls by hand.
4. Quit when done quits after the last job's final transition.
5. The tray check (library and watcher) and the tray menu with its refresh; MPRIS raise.
6. The close question in the window (contract, mock) and the setting in Settings › Library.

### Logging

`presence.changed` (`from`, `to`, `why`), `presence.tray` (`outcome`: `shown`, `no_library`,
`no_watcher`).

### Done when

- An overnight preparation is not lost to closing the window.
- On a desktop with no tray, Aoide can always be brought back.
- On a system without the appindicator library, Aoide starts and closes normally.

### Settled

- Start at login, hidden: not planned (plan's default).

---

## 6.4 The speech server Aoide starts

**Size:** M (about a week; slices 1–6 are the supervision, 7–8 the idle stop)

### Outcome

When a speech profile has a launch command, Aoide starts the server the first time speech is
wanted. The window says "Starting the voice…" while the model loads. If the server exits, the
window says so, with its exit code and the last lines it printed, and does not stay on "Offline".
If a server that was working crashes, Aoide starts it again a few times and then stops trying.
After a set time without use Aoide stops the server, which frees the graphics card's memory, and
starts it again on the next play. A server Aoide did not start is never stopped. One Aoide did
start does not outlive Aoide, even when Aoide crashes, is killed or is stopped with Ctrl+C.

### What exists

- `LaunchCommand { program, args, working_dir }` and `EngineProfile.launch`
  (`crates/tts/src/lib.rs:393`, `:414`). They cross IPC and are edited in
  `src/components/settings/EngineSettings.vue` (Program, Arguments, Working folder).
- `core/engine.rs`:
  - `Engine::launch` (`:464`) spawns the command with stdout and stderr sent to `/dev/null`. It
    logs `proc.spawn` and keeps the `Child` in `Active.launched`. Nothing ever waits on that
    child, so an exit is never noticed. On 2026-11-16 the launch command's script path no
    longer existed, and the window showed "Offline" forever.
  - `Engine::poll` (`:301`) runs every 3 s from `Core::engine_loop` (`core.rs:659`) and is
    skipped while generation is busy. It launches as soon as the engine answers `Offline`,
    which happens at app start even when nothing wants speech (`EngineStatusView.wanted`,
    `core/wire.rs:142`, is false then).
  - `launched_once: AtomicBool` allows one launch per run, so a server that crashes mid-book is
    never started again.
  - `Engine::switch` (`:114`) neither stops the launched server nor resets `launched_once`, so
    switching to another local profile never launches the new one.
  - `Engine::shutdown` (`:433`) calls `child.kill()`, which sends SIGKILL to the direct child
    only. A wrapper (`sh`, `uv run`, `conda run`) dies and the real Python server keeps running,
    holding the GPU memory and the port. It runs only from `RunEvent::Exit`
    (`src-tauri/src/lib.rs:162` → `Core::shutdown`, `core.rs:680`). A crash, `kill`, or Ctrl+C
    that does not get to `Exit` leaves the server running.
  - There is no start timeout. `EngineState` (`core/wire.rs:97`) is
    `Offline | Loading | Ready | Busy | Error`.
  - The test `an_offline_engine_is_launched_once_however_often_it_is_looked_at` runs an
    `sh -c` launch command against a `Scripted` engine (`core/engine_tests.rs`). New tests
    follow the same pattern.
- `core/browsing.rs` is the model for this component. It has its own state, a host trait
  (`BrowsingHost`: `now_ms`, settings), `idle()` driven by an injected clock, `run()` as a
  5-second loop, and `stop()` at quit, which blocks from its own thread for a few seconds at
  most. Its test `the_browser_is_shut_down_once_idle_or_turned_off_and_started_again_on_need`
  is the shape for the idle slice.
- `core/align.rs` `launch_aligner` (`:328`) and `stop_aligner` (`:375`) repeat the same
  spawn-and-kill pattern for the aligner. This plan does not change them (see Open questions).
- `crates/generation` `Backoff` waits up to 60 s after failed requests. Failures while a server
  is starting would push generation into that backoff and fill Settings › Speech engine ›
  Problems.
- `src/components/shell/EngineCard.vue` shows the engine only when
  `state === "error" && wanted`.
- Logs: `logging::init` keeps seven `aoide.*.jsonl` files and prunes only files with that
  prefix. The logs folder is `<data>/logs/` (`lib.rs` `start`).
- Dependencies: `rustix` 1.1.6 and `nix` 0.32 are in `Cargo.lock` only transitively, and none
  is a direct dependency of `src-tauri`. tokio has no `signal` feature.

### Design

**A component, `core/engine_process.rs` (`ServerProcess`).** It owns the server's state, the
child, the reader of its output, the pidfile and the server log. `Engine` keeps the speech side:
voices, model, and the status the window sees. `Core` holds both and wires them. `Engine::poll`
stops launching anything. It returns what the engine answered, and `engine_loop` passes that to
the process as an event.

**States** (decided 2026-11-16: one enum, one transition function, as AGENTS.md asks):

```rust
pub enum Server {
    /// None of ours runs. One is launched when speech is wanted.
    Stopped { why: StopWhy },              // Never, Idle, Exited, Switched, Retry, Quit
    /// Launched. Waiting for the engine to answer Ready.
    Starting { pid: u32, attempt: u32, since_ms: i64 },
    /// Ours, answering Ready.
    Running { pid: u32, last_used_ms: i64, ready_since_ms: i64 },
    /// SIGTERM sent to its process group. SIGKILL at `kill_at_ms`. Then `then`.
    Stopping { pid: u32, kill_at_ms: i64, then: After },   // After: Stopped(StopWhy) | Failed(Failure)
    /// Ours ended after it had been Ready. Launched again at `retry_at_ms` if still wanted.
    Waiting { attempt: u32, retry_at_ms: i64, exit: Exit },
    /// Given up until the user says Try again or saves the profile.
    Failed { why: Failure },
    /// Another program answers at the engine's address. Aoide uses it and never stops it.
    External,
}
pub enum Failure {
    Spawn { error: String },        // the program could not be started at all
    Exited(Exit),                   // ended before it was ever Ready
    PortTaken(Exit),                // ended saying its address is in use
    StartTimedOut,                  // no answer and no output for the start timeout
    TooManyRestarts(Exit),          // crashed more than the cap allows
}
pub struct Exit { code: Option<i32>, signal: Option<i32>, ran_ms: u64 }
```

| State | Our process | Engine status the window shows |
|---|---|---|
| `Stopped { Never }` | none | `Offline`, detail "Starts when needed" |
| `Stopped { Idle }` | none | `Offline`, detail "Stopped after N minutes unused; starts when needed" |
| `Starting` | running | `Loading`, detail "Starting the voice…" |
| `Running` | running | whatever the engine answers (`Ready`, `Busy`, `Error`) |
| `Stopping` | ending | `Offline`, detail "Stopping…" |
| `Waiting` | none | `Loading`, detail "The voice stopped; starting it again" |
| `Failed` | none | `Error`, detail naming the failure and exit code, e.g. "The speech server ended (exit code 2). Its last lines are in Settings › Speech engine." |
| `External` | none | whatever the engine answers |

A profile with no launch command has no `ServerProcess` state at all. `Engine` behaves as today,
except that it no longer launches.

**Events and actions:**

```rust
pub enum ServerEvent {
    Wanted,                          // want(): play, prepare, preview, voice listing
    Answered(EngineStatus),          // one poll's answer
    Spawned { pid: u32 },
    SpawnFailed { error: String },
    Exited(Exit),                    // the group leader was reaped
    Used,                            // a request was made (mark_busy)
    Tick,                            // timers: start timeout, idle, retry, kill grace
    Switched { same_launch: bool },  // another profile is in use
    RetryChosen,                     // "Try again" in Settings
    Quit,
}
pub enum ServerAction { None, Launch { why: LaunchWhy }, Terminate, Kill, Forget }
pub struct Ctx { now_ms: i64, wanted: bool, last_output_ms: Option<i64>, tail: Vec<String>, timing: Timing }

fn next(state: Server, event: ServerEvent, ctx: &Ctx) -> (Server, ServerAction)
```

The rules, each one a row of the table-driven test:

- `Stopped` + `Wanted` (when the last answer was `Offline`) → `Starting`, `Launch { wanted | idle_wake }`.
  `Stopped` + `Answered(Ready | Loading)` → `External`. Aoide never launches over a server that
  already answers. This is how "a server the user runs is theirs" is kept.
- `Starting` + `Answered(Ready)` → `Running`. `Starting` + `Answered(Loading)` stays in
  `Starting` and counts as a sign of life.
- `Starting` + `Tick`, with no answer and no output for `start_timeout` → `Stopping { then:
  Failed(StartTimedOut) }`, `Terminate`. The clock counts quiet time, not time since launch,
  because a first run that downloads model weights prints progress for minutes.
- `Starting` + `Exited` → `Failed(Exited)`, or `Failed(PortTaken)` when `port_taken(&tail)`
  finds "address already in use" or "EADDRINUSE" or errno 98 in the last lines. The action is
  `Forget` (see the group cleanup below). An exit before the server was ever Ready is not
  retried. A broken command or a missing script fails the same way every time (decided
  2026-11-16: no loop on a broken command).
- `Starting` + `SpawnFailed` → `Failed(Spawn)`.
- `Running` + `Used` → `Running` with `last_used_ms = now`.
- `Running` + `Exited` → `Waiting { attempt + 1, retry_at }`, with backoff 2 s, 10 s, 30 s. The
  cap is 3 restarts within 10 minutes; one more crash gives `Failed(TooManyRestarts)`. Five
  minutes Ready resets the count.
- `Waiting` + `Tick` at or after `retry_at`: `wanted` → `Starting`, `Launch { restart }`.
  Otherwise → `Stopped { Exited }`, and the next want launches it.
- `Running` + `Tick`, when `idle_minutes` is set, `!ctx.wanted`, and
  `now − last_used ≥ idle` → `Stopping { then: Stopped(Idle) }`, `Terminate`.
- `Stopping` + `Exited` → `then`. `Stopping` + `Tick` at or after `kill_at` → `Kill` (same
  state; the `Exited` that follows finishes it).
- `Failed` + `RetryChosen` → `Stopped { Retry }`, and the want that follows launches.
- `External` + `Answered(Offline)` → `Stopped { Never }`. The other program went away, so ours
  may start on the next want.
- `Switched { same_launch: false }` from `Starting | Running | Waiting` → `Stopping { then:
  Stopped(Switched) }`, and from anything else → `Stopped { Never }`. With `same_launch: true`
  (same launch command and base URL; only a voice or a parameter changed) the server keeps
  running.
- `Quit` from any state with a process → `Stopping { then: Stopped(Quit) }`.

**"Wanted" for idle** is an input (`ctx.wanted`) from the host, and the core decides it.
Speech is wanted while generation is busy or has work queued (`Generation::queued`), a book is
being prepared (`Preparation::queued`), speech that still needs audio is playing, or a preview
is running. Playing cached audio does not count, so a prepared book plays with the server
stopped.

**Launch on first want** (decided 2026-11-16). App start no longer launches anything. The first
`want()` does: play, prepare, a voice preview, or listing voices in Settings or a book's voice
menu, since choosing a voice means wanting speech. Until then the engine is polled as today,
which is how `External` is noticed. The polling is cheap and launches nothing.

**Waiting instead of failing while the server starts.** While the server is `Starting` or
`Waiting`, generation takes no work. It does not fail into `Backoff` or into Speech problems.
When the server becomes `Running`, `Core` re-plans (`PlanEvent::EngineReady`, which resets
`Backoff` like `NewPlan`). Check: `crates/generation`'s worker may need a gate (a `watch` the
worker reads before starting a request). Holding back the plan from `Core` may be simpler.

**Process handling (Unix; Windows and macOS are 6.5's).**

- **Own process group.** `CommandExt::process_group(0)` makes the server's pid its group id.
  A stop sends SIGTERM to the whole group (`kill(-pgid)`), waits for the grace period (10 s, or
  5 s at quit), then sends SIGKILL to the group. When the group leader exits by itself,
  whatever is left in the group is also sent SIGTERM and then SIGKILL. A dead `uv run` with a
  live Python child holding the port is the case this covers.
- **Ctrl+C.** In its own group the server no longer gets the terminal's SIGINT. Aoide therefore
  handles SIGINT and SIGTERM itself (tokio's `signal` feature). It calls `app.exit(0)`, so
  `RunEvent::Exit` → `Core::shutdown` runs. Today Ctrl+C reaches the server by accident, so
  this is part of the same slice.
- **Parent death (Linux).** In `pre_exec`, `prctl(PR_SET_PDEATHSIG, SIGTERM)`, then exit if the
  parent already changed (`getppid`), which closes the race. The signal fires when the
  spawning *thread* ends, not the process, so spawns happen on one dedicated thread that lives
  for the run (`aoide-speech-server`). That thread also does the blocking `wait()` and sends
  `Exited`, so an exit is noticed at once and nothing polls for it. The signal reaches only the
  direct child, which is why the pidfile below is needed too.
- **Pidfile.** `<data>/speech-server.pid` holds `pid`, `pgid`, the process's start time
  (field 22 of `/proc/<pid>/stat` on Linux) and the program's file name. It is written after the
  spawn and removed after a stop. At startup, before anything launches, a leftover file whose
  pid still runs with the same start time is a server Aoide started and lost in a crash or a
  `kill -9`. Its group is stopped (SIGTERM, grace, SIGKILL) and `engine.server.reclaim` is
  logged. A pid with a different start time was reused by another program and is left alone,
  and the file is removed. Off Linux the start time is not checked and nothing is reclaimed.
- **Dependency.** `rustix` with the `process` feature, as a Unix-only direct dependency of
  `src-tauri`: `kill_process_group`, `set_parent_process_death_signal`, `getppid`. It is
  already in the lock file.

**The server log.** stdout and stderr are piped. A reader thread appends each line to
`<data>/logs/speech-server.log` and keeps the last 40 lines in memory for the view and for
`port_taken`. It also stamps `last_output_ms` for the start timeout. At each launch the old file
becomes `speech-server.previous.log`. Writing stops at 10 MB with a closing line saying so.
The server may print the text it is speaking, so the file is the user's to read and is never
copied into `aoide.*.jsonl`. Tracing fields name it only as `file = "speech-server.log"`, and
the engine's `proc.exit` carries no `stderr_tail` (unlike ffmpeg's). `docs/logging.md` says this
under Where.

**Host trait:**

```rust
pub(super) trait ServerHost: Send + Sync {
    fn now_ms(&self) -> i64;
    /// Something needs speech now (see "Wanted" above).
    fn speech_wanted(&self) -> bool;
    /// The state changed. Core folds the view into the engine status and re-plans when Running.
    fn server_changed(&self, view: ServerView);
}
```

`ServerProcess::new(dir_for_pidfile, logs_dir, spawner_thread)` reclaims the pidfile.
`step(event, host)` applies `next` under its lock, logs the transition, releases the lock, then
carries out the action. `run(host)` ticks every second, like `Browsing::run`. `stop()` at quit
blocks for at most the quit grace. `log_tail() -> Vec<String>` serves Settings.

**Views and commands.**

- `EngineStatusView` gains `server: Option<ServerView>`, which is `None` without a launch
  command. `ServerView` has `state` (`stopped`, `starting`, `running`, `stopping`, `waiting`,
  `failed`, `external`), `failure` (the kind), `exitCode`, `idleMinutes`, `canRetry`, and
  `logFile` (the file name). The core fills `state` and `detail` of the existing view from the
  table above. The card's `error && wanted` rule then shows a failed server without UI changes.
- Commands: `retry_speech_server()`, `speech_server_log() -> Vec<String>` (the last 40 lines),
  and `show_speech_server_log()` (through `ShowInFolder`).
- Settings › Speech engine, under the launch command: the server's state line, "Stop when idle
  for [N] minutes", and, when failed, the last lines, **Try again** and **Show log**.

**Settings.** On `LaunchCommand` (crates/tts, `#[serde(default)]`, so saved profiles still load):

- `idle_minutes: Option<u32>`, offered from 5 to 240, default 30 (see Open questions).
- `start_timeout_seconds: u32`, default 180 seconds of quiet. It lives in the file, not the
  form.

Backoff steps, the restart cap and the grace periods are constants in `engine_process.rs`.

**Contract and fixtures.** `LaunchCommand` and `EngineStatusView` cross IPC:
`AOIDE_WRITE_CONTRACT=1 cargo test -p aoide contract`. A fixture for a failed server and one
for a starting server: `AOIDE_WRITE_FIXTURES=1 cargo test -p aoide fixture`. The mock serves
them as they are.

### Slices

Process tests use real `sh` scripts as fake servers: `exec sleep 300`, `echo boom >&2; exit 2`,
a wrapper that starts `sleep 300 &` and writes the child's pid to a file, and `trap '' TERM`.
Combine them with `engine_tests::Scripted` for the engine's answers (the test flips it to `Ready`
to play "the port opened"), and a host with a settable clock and `wanted`. Real servers stay out
of tests. Timings come from a `Timing` the test shrinks (grace 200 ms).

1. **The transition table.** Every rule above as a row: `(state, event, ctx) → (state,
   action)`, plus `port_taken` over sample tails. This is pure and needs no process. Seam:
   `next`.
2. **Launch on want, notice the exit.** `ServerProcess` with the spawner thread, the piped
   output and the log file. `Engine::poll` no longer launches, and `launched_once` is removed.
   Tests: polling with nothing wanted launches nothing. `want()` launches once. `echo boom >&2;
   exit 2` ends in `Failed(Exited)` with code 2, `log_tail()` holds "boom", and
   `speech-server.log` exists. A missing program ends in `Failed(Spawn)`. Seam:
   `ServerProcess::step` and `Engine::status()`. This slice fixes the 2026-11-16 bug.
3. **Starting → Running, and the start timeout.** The scripted engine answers `Loading` and then
   `Ready`, so the server is `Running` and the status reads `Starting the voice…` until then. A
   silent `sleep 300` with the clock moved past the timeout ends in `Failed(StartTimedOut)`,
   and the process is gone. Output keeps it in `Starting`. Generation takes no work while
   `Starting` and re-plans on `Running` (a `generation_tests.rs` test: no Speech problem is
   recorded while starting).
4. **Stopping the whole group.** The wrapper script's grandchild is gone after a stop.
   `trap '' TERM` is killed after the grace period. A leader that exits by itself leaves no
   member behind. `Core::shutdown` uses `stop()`. Seam: `kill(pid, 0)` on the pids the script
   wrote.
5. **Not outliving Aoide.** Pidfile reclaim: launch with one `ServerProcess`, drop it without
   stopping, and a new one over the same folder stops the leftover group. A pidfile naming a
   live process with another start time is left alone. Parent death: ending the spawner thread
   delivers SIGTERM to the child. SIGINT and SIGTERM go to `app.exit`; check that by hand,
   since it is outside core tests. Linux-only tests are `#[cfg(target_os = "linux")]`.
6. **Crashes, external servers, switching.** `Running` + exit → `Waiting` → relaunched, and the
   fourth crash in the window → `Failed(TooManyRestarts)`. **Try again** relaunches. A scripted
   engine that answers `Ready` before any want → `External`, nothing is launched, and
   `Quit` stops nothing. A script exiting with "Address already in use" → `Failed(PortTaken)`.
   `Engine::switch` to another launching profile stops the old group, and the new one launches
   on want. A voice-only change keeps it running.
7. **Idle stop.** With `idle_minutes` set, the clock past it and `wanted` false →
   `Stopped { Idle }` and the process is gone. While `wanted` (queued or prepared work) it is not
   stopped. The next `want()` launches with `why = idle_wake` and the status reads "Starting the
   voice…", never an error. Playing a cached book does not wake it.
8. **Settings and the window.** The form fields, the server line, the last lines, Try again and
   Show log. `ServerView` in the contract, and the two fixtures. A Vitest that the failed
   fixture shows the card and the log lines.

### Logging

Every new event gets a row in `docs/logging.md`, in **Preparing and speech**. The fields carry
no paths, only the log's file name, and never the server's output.

| Event | Level | Fields | Where |
|---|---|---|---|
| `engine.server.stage` | info (warn into `failed`, `waiting`) | `from`, `to`, `cause` (the event), `attempt`, `failure` (kind) | `core/engine_process.rs` `step`, where `next` is applied |
| `engine.server.stop` | info | `why` (`idle`, `switched`, `timeout`, `quit`, `leftover`), `outcome` (`terminated`, `killed` after the grace period), `took_ms` | `core/engine_process.rs`, a group stopped |
| `engine.server.reclaim` | warn | `pid`, `outcome` (`stopped`, `stale`: the pidfile named another program) | `core/engine_process.rs` `new` |
| `engine.server.log` | warn | `file`, `error` (the server log cannot be written) | `core/engine_process.rs` reader |
| `proc.spawn` (changed) | info, error | adds `why` (`wanted`, `restart`, `idle_wake`, `retry`) for `program` `engine` | moves from `core/engine.rs` to `core/engine_process.rs` |
| `proc.exit` (changed) | warn | for `program` `engine`: `code`, `signal`, `ran_ms`, `file` (`speech-server.log`); no `stderr_tail` | `core/engine_process.rs`, the spawner thread's `wait` |

Fix the stale "Where" of `engine.state` (`core/engine.rs` `set_status`) while there. The
original plan's `engine.idle_stop` is replaced by `engine.server.stop` with `why = idle`.

### Done when

- A launch command whose script does not exist shows "The speech server ended (exit code …)"
  with the script's error in Settings, within seconds of the first play. It is not shown at
  app start.
- Opening Aoide and only reading starts no server.
- `nvidia-smi` shows the memory freed after the idle time and taken again on the next play,
  which waits with "Starting the voice…" and no error.
- After `kill -9` of Aoide, the next start stops the leftover server and logs
  `engine.server.reclaim`. After Ctrl+C in the terminal, nothing is left (`pgrep -f` on the
  server's script).
- A server launched through `uv run` leaves no Python process behind after any stop.
- A server the user started themselves is used and never stopped.
- `cargo test -p aoide engine_process` passes without a GPU or a real server, and the
  docs/logging.md rows are added.

### Settled (2026-11-16)

- The launched server is its own component, with the explicit state enum and transition
  function above.
- It launches on first want, not at app start.
- An exit before ever Ready fails at once. Only crashes after Ready are retried, at most 3
  times in 10 minutes.
- The server's output goes to its own log file, never into the JSON log.
- Stops use the process group: SIGTERM, a grace period, then SIGKILL. Linux adds
  PR_SET_PDEATHSIG and a pidfile with a start-time check.
- A server Aoide did not start is never stopped. A profile switch stops ours only when the
  launch command or address changed.

### Also settled (decided 2026-11-16)

- **Idle stop is on by default, at 30 minutes.** The setting turns it off or changes the time.
- **Listing voices is not a want.** Voice menus show the voices last seen until something
  plays, prepares or previews; opening a menu never starts a stopped server.
- **The aligner uses the same component.** `ServerProcess` is written for any program; the
  speech server and the aligner (`core/align.rs`) both run through it, so the aligner gets the
  same exit notice, process-group stop and no-leftover rules. Add a slice after slice 5 that
  moves the aligner onto it.
- **Stop now.** The engine card and Settings › Speech engine offer **Stop now** for a server
  Aoide started: it frees the graphics card at once, as an idle stop does, and the next want
  starts it again.
- **Port check before launch.** Before launching, a plain TCP connect to the profile's address:
  if something accepts the connection but does not answer as the engine, the server is not
  launched and the state is `Failed(PortTaken)` with "something else listens at this address".
  The server's own "Address already in use" message stays a second way to reach that state.

---

## 6.5 Releases, Windows and macOS

**Not planned for now** (decided 2026-11-16).

**Size:** L. Linux packages are already on the roadmap.

### Linux packages

- `bundle.active: true` with targets AppImage, DEB and RPM; icons; a desktop file with the MIME
  types and the `aoide:` scheme (6.1); `productName` and the window title brought up to date
  ("EPUB Audiobooks" no longer describes it).
- **Runtime needs to declare or detect:** WebKitGTK; `ffmpeg` (M4B export runs it,
  `core/export.rs`); the app-indicator library if 6.3 is built; ALSA or PipeWire.
- **Speech recognition:** the default build has `whisper` on the processor. GPU builds stay
  build-from-source, or are separate packages; do not ship CUDA.
- **CI:** a GitHub Actions workflow that builds the three packages on a tag, runs the checks
  from [../reading-app/plan.md](../reading-app/plan.md) first, and attaches checksums. No
  auto-update: the README promises "no update check". Settings › About shows the version and a
  link to the releases page, opened only when clicked.
- **Flatpak** is the format most Linux users would look for. Its sandbox conflicts with
  launching a speech server, finding Chrome and scanning the home folder for books; it needs
  its own investigation and is not planned here.

### Windows and macOS

An audit to do before promising either, one issue each:

| Area | What to check |
|------|---------------|
| Media keys | `souvlaki` is already the path off Linux (`src-tauri/Cargo.toml`); it has never been run. |
| Paths | `core/environment.rs` and `Paths`: config, data and cache folders; `~/Documents/Aoide`. |
| Launch commands | Starting and stopping a speech server (`core/engine.rs`): process groups and signals are Unix-shaped. |
| The browser | Finding Chrome (`core/browsing.rs`), Chrome for Testing pins per platform (`core/chrome-for-testing.toml`). |
| Audio output | `crates/audio/src/output.rs` and the output commands in `core/output.rs`. |
| File watching | `notify` semantics differ; `core/watching.rs` filters inotify's open events. |
| ffmpeg | Not present by default; bundle it or detect and explain. |
| Signing | macOS notarisation needs a paid developer account; unsigned Windows builds warn. |
| Speech engines | Most local TTS servers assume Linux and NVIDIA. On macOS, Kokoro on the processor or an MLX server is the realistic route. |

### Slices (Linux)

1. Bundle configuration; a package installs and runs on a clean Fedora, Debian and Arch
   container (by hand or in CI).
2. Missing `ffmpeg` is detected and explained when exporting.
3. The workflow.
4. README: download first, build from source second.

### Done when

- A person can install Aoide without a compiler.

---

## 6.6 Listening on a phone

**Size:** a direction, not a plan. Each option needs its own design before any code.

M4B export already takes a book to a phone. What is missing is doing so without effort and
coming back to the same place. Three routes, cheapest first:

**A. Export without asking.** A setting: "When a book is prepared, write its M4B to this
folder." Pointed at a folder a sync tool or a phone app watches, prepared books appear on the
phone. Small (it is `core/export.rs` triggered by preparation's final transition). Places do not
come back.

**B. Send to a server you already run.** People who self-host their audiobooks (Audiobookshelf
is the common one) want the M4B uploaded there, and could have their place read back through
that server's API. Medium. It is a network feature to a server on the home network, so it
meets the same reach question as OPDS (4.6).

**C. A player served by Aoide on the home network.** The phone's browser opens a page served
by the desktop: the same read-along view, audio streamed from the cache, the place shared.
Large, and a real change of posture: Aoide would listen on the network for the first time. It
would need pairing, a clear on and off, and a review of everything the page can ask for.

Recommendation: build A when someone asks for it; treat C as a separate project with its own
security design.
