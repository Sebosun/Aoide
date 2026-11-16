# 6. The desktop: being a good application

Read [plan.md](plan.md) first.

The Tauri side is `src-tauri/src/lib.rs` (one window, the dialog plugin, the command list) and
`src-tauri/tauri.conf.json` (`bundle.active` is false: Aoide is built from source and run from
`target/release/aoide`). `src-tauri/capabilities/default.json` grants `core:default` and webview
zoom only. Aoide reads no command-line arguments today.

---

## 6.1 Open with Aoide, one running copy

**Size:** M. 5.2 needs it; file associations in 6.5 need it.

### Outcome

Double-click an EPUB in the file manager and it opens in Aoide, in the window that is already
running if there is one. `aoide book.epub` and `aoide https://…` on the command line do the
same.

### Design

**Pure rule first: `src-tauri/src/launch.rs`.**

```
pub enum Request { OpenFile(PathBuf), SaveLink(Url), Show }
pub fn requests(args: &[String], cwd: &Path) -> Vec<Request>
```

- An argument that is an existing file of a supported format is `OpenFile` (relative paths
  resolved against `cwd`, `file://` addresses accepted).
- `http(s)://…` and `aoide://save?url=…` are `SaveLink` (validated as in 5.2).
- Anything else is ignored and logged. No arguments at all is `Show`.
- Flags are not interpreted: an argument starting with `-` is ignored, so nothing a web page
  or a file name supplies can become an option.

**One running copy.** `tauri-plugin-single-instance`: a second launch hands its arguments and
working directory to the first and exits; the first runs `requests` on them, raises its window
and acts.

**The address scheme.** `tauri-plugin-deep-link` registers `aoide:`. On Linux that is a line in
the desktop file (`MimeType=x-scheme-handler/aoide;application/epub+zip;…`), which exists only
once Aoide is installed (6.5). For a build from source, add a small script or an `aoide
--install-desktop-file` action that writes `~/.local/share/applications/aoide.desktop` pointing
at the binary; document it in the README.

**Acting on a request** (a component `core/launching.rs`, or methods on `Shelf` if that is all
it is):

- `OpenFile`: import through the normal path. A file whose content is already in the library
  is not copied again; either way the item's page opens (`CoreEvent::ItemRequested { book_id,
  then: Open }`; check how the interface is told to navigate today and reuse that).
- `SaveLink`: 5.2.
- Requests that arrive before the interface is ready are held and delivered when it asks
  (`pending_requests()` on start), so a cold start with a file still opens it.

**Capabilities.** Add only what the two plugins need.

### Slices

1. `requests`: files, relative paths, `file://`, links, `aoide://`, flags ignored, unsupported
   files ignored.
2. A request to open a file already in the library opens it without a second copy. Seam: core
   test.
3. A new file is imported, then opened.
4. Requests before the interface is ready are held.
5. Single instance wired; a second launch raises the window (by hand: the plugin is not
   testable in unit tests; add a line to `src-tauri/src/smoke.rs` if it helps).
6. The desktop file writer and the README.

### Logging

`launch.request` (`kind`, `outcome`, `file` name only), `launch.second_instance`.

### Done when

- "Open with → Aoide" works from GNOME Files and Dolphin after the desktop file is installed.
- Starting Aoide twice never gives two windows or two writers on the library.

---

## 6.2 Notifications

**Size:** S

### Outcome

When a long job ends while Aoide is not in front, the desktop says so: a book is prepared, an
audiobook is synced, an export is written, or a job needs you.

### What exists

- Jobs with stages: preparation (`core/preparation.rs`), recording sync (`core/syncing.rs`),
  export (`core/export.rs`), article and feed items that need the user (`NeedsYouNotice.vue`).
- Toasts inside the window (`ToastStack.vue`).

### Design

**Pure rule: `core/notices.rs`.**

```
pub enum Happened { Prepared(BookId), PrepareFailed(BookId), Synced(BookId), SyncFailed(BookId),
                    Exported(BookId), ImportFinished { added: u32 }, BackupFailed }
pub fn notice(happened: &Happened, focused: bool, settings: &NoticeSettings) -> Option<Notice>
pub struct Notice { pub title: String, pub body: String, pub opens: Option<BookId> }
```

- Nothing when the window is focused (the toast is enough).
- Nothing for things the user did a moment ago: only jobs that ran longer than a minute.
- Feed arrivals never notify. A reading app that pings is a worse reading app.
- Text carries the item's title, which is the user's own data shown on their own desktop; it
  is not logged.

**Sending.** `tauri-plugin-notification` behind a host trait `Notifier`, so the rule and the
wiring are tested with a recording notifier. Clicking a notification raises the window and
opens the item (the same `ItemRequested` event as 6.1). Where the desktop has no notification
service the call fails quietly and is logged once.

**Settings.** `AppSettings.notices { enabled: bool }`, on by default, one switch. Per-kind
switches only if someone asks.

### Slices

1. `notice`: each event, focused and not, short jobs silent.
2. Wiring from each job's final transition (where its `next` is applied) to the notifier.
3. Click opens the item.
4. The setting.

### Logging

`notice.sent` (`kind`, `book`, `outcome`).

### Done when

- Preparing a book and switching to another application ends with one notification.

---

## 6.3 Keep working in the background

**Size:** M

### Outcome

Close the window while a book is being prepared or an audiobook synced and Aoide offers to keep
working. A tray icon shows it is still there, with play and pause, and **Quit when done**.

### What exists

- Closing the window ends the process. `AppSettings.resume_prepare` carries on queued
  preparation at the next start.
- Tauri has a `tray-icon` feature. On Linux it needs an app-indicator library at build and run
  time, and GNOME shows no tray without an extension.

### Design

**An explicit state, `core/presence.rs`:**

```
enum Presence { Window, Background, QuittingWhenDone }
enum Event { CloseRequested { working: bool }, Reopened, QuitChosen, QuitWhenDoneChosen, WorkEnded }
fn next(presence: Presence, event: Event, behaviour: CloseBehaviour, tray: bool) -> (Presence, Action)
enum CloseBehaviour { Ask, Quit, KeepWhileWorking, KeepAlways }
enum Action { None, HideWindow, MinimiseWindow, ShowWindow, AskUser, Quit }
```

- "Working" is: something is being prepared, synced, exported, imported or backed up, or
  audio is playing.
- **Without a tray, a hidden window would be unreachable.** When no tray is available, "keep"
  minimises instead of hiding. `tray` is detected at start and is an input to the rule.
- The first close while working asks once ("Keep working in the background?" with *Remember*).
- Background feed checks alone are not "working": Aoide does not stay alive just to poll
  feeds, unless `KeepAlways` is chosen.

**Tray menu.** The playing item with play or pause; what is being worked on, with progress;
Open Aoide; Quit when done; Quit. Built from core views, refreshed on the events the sidebar's
engine card already listens to.

**With 6.1.** A second launch while in the background shows the window.

**Build.** The tray is a Cargo feature (`tray`), on by default, so Aoide still builds where the
indicator library is missing. Add the packages to the README's build steps for each
distribution.

### Slices

1. The transition table, including "no tray".
2. Close while preparing keeps the core running and hides or minimises the window; closing
   while idle quits. Seam: the rule; the window calls by hand.
3. Quit when done quits after the last job's final transition.
4. The tray menu and its refresh.
5. Settings › About or a new General tab: what closing the window does.

### Logging

`presence.changed` (`from`, `to`, `why`).

### Done when

- An overnight preparation is not lost to closing the window.
- On a desktop with no tray, Aoide can always be brought back.

### Open questions

- Start at login, hidden? Not planned.

---

## 6.4 Stop an idle speech server

**Size:** S

### Outcome

If Aoide started the speech server, it stops it after a while without use and starts it again
when needed. A speech model no longer holds the graphics card's memory all evening.

### What exists

- `EngineProfile` has a `LaunchCommand` (`crates/tts/src/lib.rs:414`); `core/engine.rs` starts
  the server, polls it (`poll(busy)`, `Polled`), tracks use (`mark_busy`, `want`) and stops it
  on exit (`shutdown`). Read `engine.rs` and `engine_tests.rs` first: the engine's states are
  already there, and this adds one.
- Only a server Aoide launched can be stopped. One the user runs is theirs.

### Design

- `EngineProfile.stop_after_idle_minutes: Option<u32>` (off by default; offered only when the
  profile has a launch command).
- The engine's state gains "stopped for being idle", entered when Aoide launched the server,
  nothing has been made for the set time, nothing is queued and nothing is playing from speech
  that still needs audio. The next `want()` starts it as a first start does. The status shown
  is "Starting the voice…", never an error.
- Stopping uses the same path as `shutdown` (check that it ends the whole process group: a
  Python server often leaves a child holding the memory).
- Listening to cached audio does not count as use: a prepared book plays with the server
  stopped.

### Slices

1. The idle transition and its timer, with an injected clock (as the engine tests do).
2. `want()` after an idle stop starts the server and generation resumes without an error
   reaching the user.
3. A server not launched by Aoide is never stopped.
4. The whole process group is gone after a stop. Seam: a test launch command that spawns a
   child (opt-in if it is flaky on CI).
5. The setting in Settings › Speech engine.

### Logging

`engine.idle_stop` (`after_s`, `outcome`), `engine.start` gains `why` = `idle_wake`.

### Done when

- `nvidia-smi` shows the memory freed after the set time and taken again on the next play.

---

## 6.5 Releases, Windows and macOS

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
