# Handoff: building docs/next (stopped 2026-11-17)

Goal: build every in-scope feature in `docs/next/plan.md`. Out of scope (decided 2026-11-16):
3.4 Dictionary, 4.6 Calibre/OPDS, 5.4 Podcasts, 6.5 Windows/macOS; 6.6 is direction only.
Delete this file when the remaining work is done.

## Where it stands

Built, independently reviewed (all checks green) and pushed, through `a91e619`:
6.4, 1.1, 1.4, 2.1, 2.2, 2.7, 6.2, 1.2, 1.3, 3.3, 3.1, 3.2, 6.1, 5.2, 4.7, 4.3, 2.3, 2.4, 2.5,
3.7. Each plan section has a `**Built:** 2026-11-17 (<first>..<last>)` line.

**In progress, 4.4 Goals** (stopped by hand mid-build):
- Slice 1 (`progress` in `src-tauri/src/core/goals.rs`) is committed on the machine where this
  was built ("The core can tell how today and the year are going against a daily and a yearly
  goal…"), **local only, not pushed, not reviewed**. On another machine it is not there: redo it.
- Slice 2 (settings and patch: `AppSettings.goals`, `GoalSettings::validate`, contract) is
  **uncommitted** in that machine's working tree (config.rs, contract.rs, goals.rs, settings.rs, tests.rs,
  contract.ts, mock.ts, features.test.ts). Finish it: run the checks, commit, then do slices 3–4.

**Not started**, in the order planned: 5.3, 5.5, 4.1, 4.2 (needs 4.1), 6.3, 4.8, 5.1, 3.5, 2.6,
1.5, 3.6, 4.5 (every format the section lists).

## How the work was run

The workflow script
`~/.claude/projects/-home-bes-Code-projects-aoide/c6091402-3b73-4e8c-8ce2-8bc4f1f19487/workflows/scripts/build-features-wf_759787ae-9d8.js`
takes `args: {features:[{id,title,file,extra?}]}`. It runs features one at a time: a build agent
(TDD slices, one commit per slice on main, no push), then a review agent that checks the plan's
slices and Done-when items, runs every check, fixes, and pushes only if green. The batch stops at
a review that is not green. Its rules worth keeping by hand too:
- Full checks: `npm run typecheck; npm run test:ui; npx prettier --check src; cargo fmt --all
  --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace
  --exclude aoide; cargo test -p aoide`.
- Never run the desktop app; never touch `~/.local/share/com.aoide.audiobooks`, `~/.config/aoide`,
  `~/Documents/Aoide`, or the Breeze server on 127.0.0.1:8883.
- Dev servers on ports 5300–5399, killed by PID (a `pkill -f` pattern can kill its own shell).
- Screenshots off-screen only (never a visible window): headless Chromium via playwright-core,
  or WebKitWebDriver + `/usr/libexec/webkit2gtk-4.1/MiniBrowser --automation` under
  `Xvfb :99` with `GDK_BACKEND=x11`. Playwright's own WebKit does not run on Fedora 44.
- Stage files by name; never commit screenshots or logs.

## Checks only a person can do (the app was never run)

- 6.4: Ctrl+C on the real app; nvidia-smi after the idle stop; a real `uv run` server.
- 6.1: a second start brings the window forward; `aoide --install-desktop-file`, then
  Open with → Aoide in Files/Dolphin; `aoide some.epub` from a terminal.
- 5.2: drag Save to Aoide from Settings › Sites to the bookmarks bar and click it with Aoide
  running and closed (untested whether WebKitGTK lets the link be dragged out; Copy is the fallback).
- 6.2: clicking a desktop notification.
- 3.3: compare `ui.frames` scrolling with Justify and Hyphenate on.
- 3.7: F11, F, Esc and the window manager's fullscreen shortcut on Wayland and X11.

## Open decisions for the user

- 6.4: Stop now while work is pending relaunches the server on the next tick (pending work counts
  as a want). An aligner failure has no Try again in the UI. The failed line in Settings › Speech
  engine points to the page you are already on.
- 1.1: Say this again with the server offline leaves the book marked prepared without that audio.
- 1.2: a word selected with its full stop ("Ren.") makes a rule matching only "Ren.".
  BookVoice.vue shows Regenerate audio twice (older bug).
- 1.3: "X left" on Home and library cards still counts left-out chapters (BookSummary.wordCount).
- 2.2: Book details (BookListening.vue) still estimates time left from words.
- 2.4: an end-of-chapter timer rings when any other chapter plays, can carry to the next book,
  and is not cancelled on finish or stop (older behaviour).
- 2.5: the queue panel and Home's suggestions are both called "Up next". A queued feed article
  saved to the library is archived on finish even with "Archive articles when I finish them" off.
- 2.7: speed limits 0.5x–3x are repeated in `useShortcuts.ts`.
- 3.1: the old Say this differently button at the top of the reader is still there beside the
  menu entry.
- 3.2: clicking inside a highlight opens its card instead of playing. `highlight.detached` logs on
  every listing.
- 4.3: Storage › Delete audio of archived items was not built (the plan says "can"). An archived
  article a feed still lists returns to the shelf still archived.
- 4.7: books already in the inbox folder are added too; with Downloads as inbox, every text PDF
  becomes a book; failed files are only logged.
- 6.1: files opened from outside don't get the audiobook lookup that dropped files get.

## Machine state

- `~/.local/bin/aoide` is a CUDA release build from before all of this; it has none of these
  features. Rebuild when the user asks.
- `~/.config/aoide/settings.toml` `[engine.launch]` points to the deleted
  `/home/bes/Code/projects/slop/tools/breeze-tts/aoide_breeze_tts.py`; the script now lives at
  `tools/breeze-tts/aoide_breeze_tts.py` in this repo. The user's file, not changed.
- The Breeze server was started by hand from this session: `cd ~/apps/breeze-tts && venv/bin/python
  -I <repo>/tools/breeze-tts/aoide_breeze_tts.py --breeze ~/apps/breeze-tts --model
  ~/apps/breeze-tts-2 --port 8883 --fast`.
