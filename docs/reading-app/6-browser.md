# Phase 6: the optional browser

Read [plan.md](plan.md) first. Mockups: `design/NeedsYou.dc.html`, `design/SignIn.dc.html`,
`design/Sites.dc.html`.

## Outcome

For pages plain HTTP cannot read (web apps that render in the browser, sites that challenge
automated requests, sites the user is signed in to), Aoide can drive a real Chrome: render the
page, take the DOM as the snapshot, and let the user sign in or pass a check once in a visible
window. It is **off by default**. A user who never turns it on has no browser downloaded, no
browser process, and the phase-4 behaviour.

## What exists after phase 5

`Snapshot { .., via }` and `HttpFetcher` in `aoide-fetch`; `Completeness` and wall detection
in `aoide-article`; the fetch stage with `NeedsYou(reason)` and the worker in
`core/articles.rs`; the notice component; feeds that open entries through the same worker.

## The choice (settings)

```
enum BrowserUse { Off, Fallback, First }     // AppSettings.browser.use_, default Off
struct BrowserSettings { use_: BrowserUse, path: Option<PathBuf>, idle_seconds: u32 /* 60 */ }
```

- `Off`: the browser is never started. The notice offers "Turn on the browser…" (opens
  Settings) in place of the sign-in action.
- `Fallback`: HTTP first; the browser is used when the HTTP result is not `Complete`, and for
  sites whose rule says `browser`.
- `First`: the browser for every article fetch. Feeds themselves are always fetched over HTTP.
- A per-site rule (below) overrides the global choice in either direction, except that `Off`
  always wins: with `Off`, site rules asking for the browser are ignored and the item says so.
- Per article: the reader's menu and the notice offer "Fetch with the browser" when the
  setting is not `Off`.

Which fetcher runs is decided by one function,
`plan_fetch(use_, site_rule, previous: Option<Completeness>) -> Vec<Route>` where
`Route { Http, Browser, Archive }`, tested as a table. The worker walks the routes in order
and stops at the first `Complete` result, keeping the best snapshot seen (phase 4's
"never replace a better copy" rule).

## New crate: `crates/browser` (`aoide-browser`)

Drives Chrome over the DevTools protocol (evaluate `chromiumoxide`, see plan.md; wrap it so
the rest of the app sees only this API).

```
pub struct BrowserOptions {
    pub binary: PathBuf, pub profile_dir: PathBuf,
    pub page_timeout: Duration,        // 45 s
    pub reach: Reach,                  // same rule as aoide-fetch
}
pub struct RenderRequest {
    pub url: Url,
    pub clear_site_data: bool,
    pub block: Vec<String>,            // URL patterns not to load
    pub wait_for: Option<String>,      // CSS selector that marks the article as present
    pub user_agent: Option<String>,
}
impl Browser {
    pub async fn launch(options: BrowserOptions) -> Result<Browser, BrowserError>;
    pub async fn render(&self, request: &RenderRequest) -> Result<Snapshot, BrowserError>;  // via: Browser
    pub async fn open_window(&self, url: &Url) -> Result<Window, BrowserError>;             // visible
    pub async fn clear_site(&self, host: &str) -> Result<(), BrowserError>;
    pub async fn shutdown(self);
}
impl Window {
    pub async fn snapshot(&self) -> Result<Snapshot, BrowserError>;   // current DOM and URL
    pub async fn closed(&self) -> bool;
    pub async fn close(self);
}
pub fn find_binary(configured: Option<&Path>, path_dirs: &[PathBuf], downloaded: &Path) -> Option<FoundBinary>
```

Rules:

- **Full Chrome or Chromium**, run with `--headless=new` for rendering and with a normal
  window for sign-in. The old headless shell is not used: it is the easiest build to detect.
- **One profile**, `<data>/browser/profile`, kept between runs, separate from the user's own
  browser. Cookies and challenge clearances persist there. This is deliberate: a fresh profile
  is challenged on every fetch.
- **Readiness** (`ready.rs`, a pure state machine over observed events, unit-tested without a
  browser): wait for the load event; then for the network to be quiet for 500 ms; then, if
  `wait_for` is set, for that selector, else for the text length of `body` to be unchanged
  across two samples 300 ms apart; give up at `page_timeout` and return what is there. The
  snapshot is `document.documentElement.outerHTML` plus the final URL.
- **Blocking**: URLs matching `block` are not loaded. Downloads are denied. Permission prompts
  (location, notifications, camera, microphone, clipboard) are denied.
- **`clear_site_data`**: cookies and storage for the request's site are cleared before
  navigation.
- **`Reach::Public`**: navigations and subresource requests to loopback, private and
  link-local addresses are refused, as in `aoide-fetch`.
- **Process**: the DevTools connection uses a pipe, or a loopback port chosen by the OS. The
  browser's own sandbox stays on. Child processes are killed when `Browser` is dropped and at
  app shutdown; a stale profile lock from a crashed run is cleared at launch when no browser
  process owns it. One page renders at a time.
- **`find_binary`** order: the configured path; then `google-chrome`, `google-chrome-stable`,
  `chromium`, `chromium-browser` on `PATH`; then the downloaded build. Brave and other
  Chromium derivatives are used only when configured explicitly (Brave's headless mode is
  known to hang on the development machine). Tested over temporary directories with fake
  executables.

Tests:

- Pure parts (readiness machine, `find_binary`, request building) run in `cargo test -p
  aoide-browser` with no browser.
- Real-browser tests are opt-in (`AOIDE_BROWSER_TESTS=1`, binary from `AOIDE_BROWSER`), in
  `tests/live_browser.rs`, against pages served from loopback with `Reach::AnyAddress`: a
  static page; a page whose article is inserted by script after 1 s; a page that never goes
  quiet (timeout returns partial content); a blocked script; cookies kept across two renders
  and cleared by `clear_site_data`; a denied download.

## Getting a browser

- Settings shows what `find_binary` found. With nothing found, "Download Chrome" fetches a
  pinned Chrome for Testing build for the platform into `<data>/browser/chrome/`, with
  progress, cancel and a checksum check. Model it on the speech-recognition model download
  (`download_model`, `CoreEvent::ModelDownload`, `crates/transcribe/src/models.rs`): an
  explicit stage enum, an event with progress, resumable.
- The pinned version and its checksums are data in one file. Check the current Chrome for
  Testing download endpoints and their terms when building this.
- The download counts in the storage report; "Remove downloaded browser" deletes it.

## Per-site rules

One rule per host (`www.` stripped; a rule for `example.com` covers its subdomains unless a
more specific rule exists).

```
struct SiteRule {
    fetch: Option<Route>,              // Http | Browser; None = follow the global setting
    site_data: SiteData,               // Keep (default) | ClearBeforeFetch
    block: Vec<String>,
    wait_for: Option<String>,
    user_agent: Option<String>,
    fallbacks: Vec<Fallback>,          // JsonLd (always tried by extraction), Archive
}
```

- Stored as a user-editable data file, `~/.config/aoide/site-rules.toml`, next to
  `settings.toml`, read through `crates/config`. It ships empty: no built-in rules for named
  sites. The file is re-read when it changes.
- Settings edits `fetch` and `site_data`; the other fields are edited in the file. Rules rot as
  sites change, so they are data, not code.
- Parsing and host matching are pure functions in `crates/config`, tested there.

## Data (library v11)

```
CREATE TABLE sites (
  host TEXT PRIMARY KEY NOT NULL,
  signed_in_at INTEGER,          -- a sign-in flow completed for this host
  check_passed_at INTEGER,       -- a verification check was passed
  last_wall TEXT, last_wall_at INTEGER   -- the last Reason seen, for the Sites list
);
```

A row appears when a site first needs the user or first gets a rule.

## State: the sign-in flow (`src-tauri/src/core/signin.rs`, new)

```
enum Stage { Opening, Waiting, Fetching, Done, Cancelled, Failed(String) }
enum Event { WindowOpened, ArticleSeen, UserSaidDone, WindowClosed, Fetched(Completeness),
             Cancelled, BrowserFailed(String) }
fn next(stage: Stage, event: Event) -> Stage
```

- `start_sign_in(article_id)` opens a visible window on the article's URL in the Aoide
  profile → `Waiting`.
- While `Waiting`, every 2 s the core takes the window's snapshot and runs `extract`. When the
  window is on the article's host and the result is `Complete` → `ArticleSeen` → `Fetching`.
  The user pressing "I'm signed in, fetch again" → `UserSaidDone` → `Fetching`.
- `Fetching`: the window's own snapshot is used when it is `Complete`; otherwise one headless
  render. The result goes through the normal article stage machine. Then the window is closed
  → `Done`.
- The user closing the window while `Waiting` → `Cancelled`. Nothing about the article
  changes.
- On `Done` with a complete article: `sites.signed_in_at` (reason was `SignIn` or `Paywall`)
  or `check_passed_at` (reason was `Check`) is set. With "Stay signed in to this site"
  unticked, the site's data is cleared after the fetch.
- One flow at a time. `CoreEvent::SignInState(SignInView)` (`signin://state`).

## Core API

| Command | Result |
|---|---|
| `browser_status()` | `BrowserStatusView { use_, binary: Found { path, version, downloaded } \| Missing, download: Option<DownloadView> }` |
| `download_browser()` / `cancel_browser_download()` / `remove_browser()` | |
| `refetch_article(id, Browser)` | extends phase 4's command |
| `start_sign_in(article_id)` / `finish_sign_in()` / `cancel_sign_in()` / `show_sign_in_window()` | |
| `sites()` | `Vec<SiteView { host, session: SignedIn { at } \| CheckPassed { at } \| AskedFor { reason, at } \| None, fetch, site_data }>` |
| `set_site_rule(host, fetch, site_data)` | writes `site-rules.toml` |
| `sign_out_site(host)` / `forget_site(host)` | clears the site's browser data; `forget` also drops the row and rule |
| `clear_browser_data()` | removes the whole profile and every `sites` session field |

The browser is started on first need and shut down after `idle_seconds` without work. Every
entry point checks `BrowserUse`: with `Off`, nothing here launches a process (tested).

## Interface

- **Settings › Sites** (new tab in `SettingsDialog`) from `Sites.dc.html`:
  - "Use a browser for articles": Off / When plain fetching fails / Always, with one line each
    explaining the cost (a download of about 150 MB; pages run in a real browser on this
    computer);
  - Browser for fetching: found path and version, Change…, Download Chrome, Remove;
  - the sites table: host, session state, site-data policy (Keep / Clear before each fetch),
    fetch route (Automatic / Plain / Browser), and Sign in… / Sign out / Forget;
  - Remove all site data.
  With the setting `Off`, the table and browser rows are shown disabled under the switch.
- **Notice** (`NeedsYouNotice.vue`): the primary action per reason becomes "Sign in to
  <site>…" (`SignIn`, `Paywall`), "Open the check in a window…" (`Check`), "Open in a
  window…" (`CutShort`, `Empty`); all start the sign-in flow. With the setting `Off`, the
  primary action is "Turn on the browser…".
- **Sign-in dialog** (`src/components/article/SignInDialog.vue`) from `SignIn.dc.html`: the
  three steps rendered from `SignInView.stage`, "Stay signed in to this site", Cancel, Show
  the window, "I'm signed in, fetch again". It closes itself on `Done` and reports the result
  as a toast.
- **Library and Feeds**: the "Needs you" marker on cards (phase 4) also appears on Home rows
  and feed entries whose item is in that stage.
- Preview params: `settings=sites`, `signin=waiting|fetching`, `browser=off|missing|ready`.

## Slices

### 6.1 Settings and `plan_fetch`
Test (core): the route table for every combination of `BrowserUse`, site rule and previous
result; with `Off`, a scripted browser is never called.

### 6.2 Site rules file
Test (`crates/config`): parse, host matching with subdomains, write-back preserving fields
the UI does not edit, reload on change.

### 6.3 Readiness machine and `find_binary`
Test (`aoide-browser`, no browser): each readiness path including the timeout; binary search
order and the explicit-only rule for derivatives.

### 6.4 Render with a real browser (opt-in tests)
The live tests listed above pass locally with `AOIDE_BROWSER_TESTS=1`. Record the browser and
version used in "Notes from implementation".

### 6.5 The worker uses the browser
Test (core, scripted `Browser` behind a trait, as the fetcher is): `Fallback` renders after a
cut-short HTTP result and ends `Ready` with `via: Browser`; `First` skips HTTP; a browser
failure leaves the HTTP snapshot and stage in place with a message; the browser is shut down
after the idle time (supplied clock).

### 6.6 Browser download
Test (core, loopback server standing in for the download host): progress events, cancel,
resume, checksum mismatch, remove.

### 6.7 Library v11 and sites
Test (`library_v11.rs`; core): a wall creates the site row; `sites()` joins rows and rules;
`sign_out_site` calls `clear_site` and clears the session fields.

### 6.8 Sign-in flow
Test (core, scripted browser whose window snapshot changes over time): the transition table;
auto-detection moves to `Fetching` when the article appears; "I'm signed in" does the same;
closing the window cancels and leaves the article untouched; success records the session;
"stay signed in" off clears the site afterwards.

### 6.9 Settings › Sites
Test (`settings.test.ts`): the switch, the three browser states, editing a site's policy and
route, sign out, forget, remove all (with the confirm dialog).

### 6.10 Notice and sign-in dialog
Test (`reader.test.ts`, new `signin.test.ts`): the notice's action per reason and per
`BrowserUse`; the dialog's steps per stage; its buttons call the commands; it closes on done.

### 6.11 Docs and conventions
`AGENTS.md`: where browser code lives; browser tests are opt-in; the browser is never
launched with `BrowserUse::Off`. `README.md`: what turning the browser on does, what is
stored in the profile, how to remove it. `docs/roadmap.md`.

## Done when

- All slices committed, all checks pass; the opt-in browser tests pass locally.
- With the setting `Off` on a fresh smoke home: no browser binary is looked for, nothing is
  downloaded, no browser process ever starts, and articles behave as at the end of phase 5.
- With `Fallback`: a script-rendered page that plain HTTP read as "needs you" is fetched
  complete; a site the user signs in to once is fetched complete afterwards, including from
  its feed; signing out makes it "needs you" again.
- Quitting Aoide leaves no browser process behind (check the process list).

## Notes from implementation

- The schema is library **v12**, not v11 (the ledger has been one ahead since phase 2). The
  older migration tests drop `sites` so they still describe their own version.
- 6.4: `Reach` is enforced by a proxy, not by intercepting requests over DevTools. The
  browser's only way out is an HTTP proxy on a loopback port (`proxy.rs`) that resolves each
  name itself and connects only to an address it checked, so pages, frames, workers and
  names that resolve differently a second time cannot reach this computer or its network.
  DevTools uses a loopback port the system picks. The live tests passed with Chromium
  154.0.8037.97 (Fedora 44), and again on 2026-11-15 at the end of the phase.
- 6.5: `walk` follows the routes from `plan_fetch`, keeps the better copy and stops at the
  first `Complete`. A browser that is busy is never stopped for being idle; turning the
  setting `Off` stops a running one; `Core::shutdown` stops it.
- 6.6: Chrome for Testing **157.0.8103.78** is pinned in `core/chrome-for-testing.toml`
  (linux64, linux-arm64, mac-arm64, mac-x64, win64: size, SHA-256 and program path). Each
  hash was computed from the zip and cross-checked against the host's MD5. The download is
  about 200 MB, not 150. It comes from Google's public bucket to each user's own copy of
  Aoide (nothing is redistributed); the data file and the Settings note say its use is under
  Google's Chrome terms. A host that ignores `Range` makes the download start over. The zip
  is unpacked into `chrome.unpacking` and renamed into place, so it is there whole or not
  at all.
- 6.7: `site.*` log events carry no host (AGENTS.md: fields carry book ids and file names
  only). `SiteView.waitingArticle` names an article the site holds back, so the Sites
  table can offer "Sign in…" with it.
- 6.8: one task owns the window and takes asks (done, cancel, show) over a channel. While a
  window is open the browser counts as busy, and renders reuse that visible browser (one
  process shares the profile). The article is taken in through `Articles::receive`: the
  window's own page when it shows the whole article, else one render with the profile as it
  is.
- 6.10: "Open original" moved out of the notice (it stays in the article header). Feed entries
  carry `EntryView.needsYou` from the core. `?article=needsyou-check` was added to the
  preview.
- With `Off`, nothing looks for a browser on its own. Settings › Sites asks for the status,
  which checks whether the program file exists (without running it) so the tab can say what
  would run. The version is only read once the setting is not `Off`.
- Done-check (native smoke runs, `AOIDE_SMOKE_ARTICLE`, 2026-11-15). With `Off` on a fresh
  home, an article was fetched over HTTP (Ready, 1112 words): no browser process ran and no
  `browser/` folder was made. A page whose text is built by script (quotes.toscrape.com/js)
  came back over HTTP as "needs you · empty" (2 words). With `Fallback`, the browser started,
  rendered it in about 3 s and the article was Ready via Browser (190 words). The browser
  stopped on quit and no process was left. The sign-in path (a site signed in to once
  fetched complete afterwards, also from its feed; signing out makes it "needs you" again)
  needs a real account in a visible window. It was covered by the core tests with a scripted
  window, not checked against a live site.
