//! The optional browser for articles plain HTTP cannot read: whether it is used at all
//! ([`BrowserUse`], off by default), which routes one fetch walks ([`plan_fetch`]), the
//! per-site rules, and the browser itself, started on first need and shut down when idle.
//! With `Off`, nothing here runs, downloads or starts a browser.

use super::browser_download::{BrowserDownloadView, Download, DownloadHost, Source};

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_article::Completeness;
use aoide_browser::RenderRequest;
use aoide_config::{Fallback, SiteData, SiteFetch, SiteRule, SiteRules};
use aoide_fetch::Snapshot;
use serde::{Deserialize, Serialize};
use url::Url;

use super::lock;

/// When articles are fetched with the browser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum BrowserUse {
    /// Never: no browser is looked for, downloaded or started.
    #[default]
    Off,
    /// When plain fetching does not get the whole article, and for sites whose rule says so.
    Fallback,
    /// For every article.
    First,
}

/// The browser's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BrowserSettings {
    #[serde(rename = "use")]
    pub use_: BrowserUse,
    /// The Chrome or Chromium to run; found on its own when unset.
    pub path: Option<String>,
    /// Seconds without work after which the browser is shut down.
    pub idle_seconds: u32,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            use_: BrowserUse::Off,
            path: None,
            idle_seconds: 60,
        }
    }
}

/// Idle times Settings offers, in seconds.
pub const IDLE_SECONDS: std::ops::RangeInclusive<u32> = 10..=3600;

/// One way of getting an article's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Http,
    Browser,
    /// An archived copy.
    Archive,
}

/// The routes an article's fetch walks, in order, under the global choice `use_`, the site's
/// `rule` and how complete the copy fetched before was (`previous`). The worker stops at the
/// first complete result and keeps the best copy seen. A site's rule overrides the global
/// choice either way, except that `Off` always wins.
pub fn plan_fetch(
    use_: BrowserUse,
    rule: Option<&SiteRule>,
    previous: Option<Completeness>,
) -> Vec<Route> {
    let fetch = rule.and_then(|r| r.fetch);
    let mut routes = match (use_, fetch) {
        (BrowserUse::Off, _) | (_, Some(SiteFetch::Http)) => vec![Route::Http],
        (_, Some(SiteFetch::Browser)) | (BrowserUse::First, None) => vec![Route::Browser],
        // Plain HTTP fell short before: straight to the browser.
        (BrowserUse::Fallback, None) => match previous {
            Some(c) if c != Completeness::Complete => vec![Route::Browser],
            _ => vec![Route::Http, Route::Browser],
        },
    };
    if rule.is_some_and(|r| r.fallbacks.contains(&Fallback::Archive)) {
        routes.push(Route::Archive);
    }
    routes
}

/// Whether the site's rule asks for the browser while it is off (the article says so).
pub fn browser_wanted_but_off(use_: BrowserUse, rule: Option<&SiteRule>) -> bool {
    use_ == BrowserUse::Off && rule.is_some_and(|r| r.fetch == Some(SiteFetch::Browser))
}

/// What the browser is asked to render for `url` under the site's `rule`.
pub fn render_request(url: &Url, rule: Option<&SiteRule>) -> RenderRequest {
    let mut request = RenderRequest::new(url.clone());
    if let Some(rule) = rule {
        request.clear_site_data = rule.site_data == SiteData::ClearBeforeFetch;
        request.block = rule.block.clone();
        request.wait_for = rule.wait_for.clone();
        request.user_agent = rule.user_agent.clone();
    }
    request
}

/// Said when the browser is asked for while it is off.
pub const BROWSER_OFF: &str = "The browser is off. Turn it on in Settings › Sites.";

/// A boxed future, as the browser's seams return.
pub type Pending<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// How to start the browser: its program, its profile, how long a page may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub binary: PathBuf,
    pub profile_dir: PathBuf,
    pub page_timeout: Duration,
}

/// A visible window the user signs in with.
pub trait BrowserWindow: Send + Sync {
    /// Its page and address now.
    fn snapshot(&self) -> Pending<'_, Result<Snapshot, String>>;
    /// Whether the user closed it.
    fn closed(&self) -> Pending<'_, bool>;
    /// Brings it to the front.
    fn show(&self) -> Pending<'_, ()>;
    fn close(self: Box<Self>) -> Pending<'static, ()>;
}

/// Drives a browser: Chrome in the app, a script in tests. It starts the browser when a call
/// needs one (headless to render, with a window to sign in) and keeps it until shut down.
pub trait BrowserDriver: Send + Sync {
    fn render<'a>(
        &'a self,
        launch: &'a Launch,
        request: &'a RenderRequest,
    ) -> Pending<'a, Result<Snapshot, String>>;
    fn open_window<'a>(
        &'a self,
        launch: &'a Launch,
        url: &'a Url,
    ) -> Pending<'a, Result<Box<dyn BrowserWindow>, String>>;
    /// Forgets `host`'s cookies and storage in the profile.
    fn clear_site<'a>(
        &'a self,
        launch: &'a Launch,
        host: &'a str,
    ) -> Pending<'a, Result<(), String>>;
    /// Whether a browser process runs.
    fn running(&self) -> bool;
    /// Ends the browser process, if one runs.
    fn shutdown(&self) -> Pending<'_, ()>;
}

/// What the browser component needs from the rest of the app.
pub(super) trait BrowsingHost: DownloadHost {
    fn now_ms(&self) -> i64;
    fn browser_settings(&self) -> BrowserSettings;
}

/// The browser Aoide would run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum BrowserBinaryView {
    #[serde(rename_all = "camelCase")]
    Found {
        path: String,
        /// Its version (`None` while the browser is off: finding out runs it).
        version: Option<String>,
        /// Whether it is the one Aoide downloaded.
        downloaded: bool,
    },
    Missing,
}

/// The browser's state, as Settings › Sites shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BrowserStatusView {
    #[serde(rename = "use")]
    pub use_: BrowserUse,
    pub binary: BrowserBinaryView,
    /// The download under way or last made in this run.
    pub download: Option<BrowserDownloadView>,
    /// The version "Download Chrome" would get; `None` where none is offered.
    pub offered_version: Option<String>,
    /// Whether a browser was downloaded (it can be removed).
    pub downloaded: bool,
}

/// How long a page may take to be ready.
const PAGE_TIMEOUT: Duration = Duration::from_secs(45);

/// The browser and the site rules.
pub(super) struct Browsing {
    driver: Arc<dyn BrowserDriver>,
    /// `<data>/browser`: the profile, and a downloaded browser.
    dir: PathBuf,
    rules_path: PathBuf,
    rules: Mutex<SiteRules>,
    /// When the browser last did something.
    last_used: Mutex<Option<i64>>,
    /// Calls on the browser not yet answered; it is not idle while any are.
    busy: Arc<AtomicUsize>,
    /// Where the browser's tasks run, to shut it down from outside them.
    runtime: tokio::runtime::Handle,
    /// The downloaded browser.
    download: Download,
    /// The last program asked for its version, and its answer.
    version: Mutex<Option<(PathBuf, Option<String>)>>,
}

/// A window that keeps the browser in use until it is closed or dropped.
struct HeldWindow {
    window: Option<Box<dyn BrowserWindow>>,
    busy: Arc<AtomicUsize>,
}

impl HeldWindow {
    fn new(window: Box<dyn BrowserWindow>, busy: Arc<AtomicUsize>) -> Self {
        busy.fetch_add(1, Ordering::SeqCst);
        Self {
            window: Some(window),
            busy,
        }
    }

    fn window(&self) -> &dyn BrowserWindow {
        self.window.as_deref().expect("open until closed")
    }
}

impl Drop for HeldWindow {
    fn drop(&mut self) {
        self.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

impl BrowserWindow for HeldWindow {
    fn snapshot(&self) -> Pending<'_, Result<Snapshot, String>> {
        self.window().snapshot()
    }

    fn closed(&self) -> Pending<'_, bool> {
        self.window().closed()
    }

    fn show(&self) -> Pending<'_, ()> {
        self.window().show()
    }

    fn close(mut self: Box<Self>) -> Pending<'static, ()> {
        let window = self.window.take().expect("open until closed");
        Box::pin(async move {
            window.close().await;
            drop(self);
        })
    }
}

/// Marks the browser in use for as long as it lives, and when it was last used.
struct InUse<'a> {
    browsing: &'a Browsing,
    host: &'a dyn BrowsingHost,
}

impl<'a> InUse<'a> {
    fn new(browsing: &'a Browsing, host: &'a dyn BrowsingHost) -> Self {
        browsing.busy.fetch_add(1, Ordering::SeqCst);
        *lock(&browsing.last_used) = Some(host.now_ms());
        Self { browsing, host }
    }
}

impl Drop for InUse<'_> {
    fn drop(&mut self) {
        *lock(&self.browsing.last_used) = Some(self.host.now_ms());
        self.browsing.busy.fetch_sub(1, Ordering::SeqCst);
    }
}

impl Browsing {
    /// Must be called inside the async runtime.
    pub fn new(
        driver: Arc<dyn BrowserDriver>,
        dir: PathBuf,
        config_dir: &Path,
        download: Option<Source>,
    ) -> Self {
        let rules_path = config_dir.join(aoide_config::SITE_RULES_FILE);
        let rules = SiteRules::load(&rules_path).unwrap_or_else(|error| {
            tracing::warn!(event = "config.problem", %error, "Cannot read the site rules");
            SiteRules::default()
        });
        Self {
            driver,
            rules_path,
            rules: Mutex::new(rules),
            last_used: Mutex::new(None),
            busy: Arc::default(),
            runtime: tokio::runtime::Handle::current(),
            download: Download::new(download, dir.clone()),
            version: Mutex::new(None),
            dir,
        }
    }

    pub fn profile_dir(&self) -> PathBuf {
        self.dir.join("profile")
    }

    /// Where a downloaded browser's program is.
    pub fn downloaded_binary(&self) -> PathBuf {
        self.download.program()
    }

    /// What browser would run, and the download.
    pub fn status(&self, host: &dyn BrowsingHost) -> BrowserStatusView {
        let settings = host.browser_settings();
        let configured = settings.path.as_deref().map(Path::new);
        let found = aoide_browser::find::find_binary(
            configured,
            &aoide_browser::find::path_dirs(),
            &self.downloaded_binary(),
        );
        let binary = match found {
            Some(found) => {
                let version = if found.downloaded {
                    self.download.installed_version()
                } else if settings.use_ == BrowserUse::Off {
                    None
                } else {
                    self.version_of(&found.path)
                };
                BrowserBinaryView::Found {
                    path: found.path.to_string_lossy().into_owned(),
                    version,
                    downloaded: found.downloaded,
                }
            }
            None => BrowserBinaryView::Missing,
        };
        BrowserStatusView {
            use_: settings.use_,
            binary,
            download: self.download.view(),
            offered_version: self.download.offered().map(str::to_owned),
            downloaded: self.download.folder().exists(),
        }
    }

    /// `binary --version`, asked once per program.
    fn version_of(&self, binary: &Path) -> Option<String> {
        if let Some((path, version)) = lock(&self.version).as_ref() {
            if path == binary {
                return version.clone();
            }
        }
        let version = aoide_browser::find::version(binary);
        *lock(&self.version) = Some((binary.to_owned(), version.clone()));
        version
    }

    /// Downloads the pinned browser (not while the browser is off).
    pub async fn download(&self, host: &dyn BrowsingHost) -> Result<(), String> {
        if host.browser_settings().use_ == BrowserUse::Off {
            return Err(BROWSER_OFF.into());
        }
        self.download.download(host).await
    }

    pub fn cancel_download(&self) {
        self.download.cancel();
    }

    /// Deletes the downloaded browser, shutting it down first.
    pub async fn remove_download(&self) -> Result<(), String> {
        if self.driver.running() {
            self.driver.shutdown().await;
            *lock(&self.last_used) = None;
            tracing::info!(
                event = "browser.stop",
                outcome = "removed",
                "Shut the browser down"
            );
        }
        self.download.remove()
    }

    /// The rule for `host`, if any covers it.
    pub fn rule_for(&self, host: &str) -> Option<SiteRule> {
        lock(&self.rules).rule_for(host).map(|(_, rule)| rule)
    }

    /// The hosts with a rule.
    pub fn rule_hosts(&self) -> Vec<String> {
        lock(&self.rules).hosts()
    }

    /// Sets the parts of `host`'s rule Settings edits, keeping the rest, and saves the file.
    pub fn set_rule(
        &self,
        host: &str,
        fetch: Option<SiteFetch>,
        site_data: SiteData,
    ) -> Result<(), String> {
        let mut rules = lock(&self.rules);
        rules.set(host, fetch, site_data);
        rules.save(&self.rules_path).map_err(|e| e.to_string())
    }

    /// Drops `host`'s rule and saves the file.
    pub fn forget_rule(&self, host: &str) -> Result<(), String> {
        let mut rules = lock(&self.rules);
        rules.remove(host);
        rules.save(&self.rules_path).map_err(|e| e.to_string())
    }

    /// Deletes the whole profile (every sign-in and all site data), shutting the browser down
    /// first.
    pub async fn clear_profile(&self) -> Result<(), String> {
        if self.driver.running() {
            self.driver.shutdown().await;
            *lock(&self.last_used) = None;
            tracing::info!(
                event = "browser.stop",
                outcome = "cleared",
                "Shut the browser down"
            );
        }
        match std::fs::remove_dir_all(self.profile_dir()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => {
                tracing::info!(
                    event = "browser.profile",
                    outcome = "cleared",
                    "Removed all browser data"
                );
                Ok(())
            }
        }
    }

    /// Reads the rules file again after it changed on disk.
    pub fn reload_rules(&self) {
        match lock(&self.rules).reload(&self.rules_path) {
            Ok(true) => tracing::info!(
                event = "config.reload",
                file = aoide_config::SITE_RULES_FILE,
                "Read the site rules again"
            ),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(event = "config.problem", %error, "Cannot read the site rules")
            }
        }
    }

    /// How the browser would be started now; an error when none is found.
    fn launch(&self, settings: &BrowserSettings) -> Result<Launch, String> {
        let configured = settings.path.as_deref().map(Path::new);
        let found = aoide_browser::find::find_binary(
            configured,
            &aoide_browser::find::path_dirs(),
            &self.downloaded_binary(),
        )
        .ok_or_else(|| match configured {
            Some(path) => format!("The browser at {} cannot be run.", path.display()),
            None => "No Chrome or Chromium was found. Download one in Settings › Sites.".into(),
        })?;
        Ok(Launch {
            binary: found.path,
            profile_dir: self.profile_dir(),
            page_timeout: PAGE_TIMEOUT,
        })
    }

    /// The browser's setup, unless it is off.
    fn ready(&self, host: &dyn BrowsingHost) -> Result<Launch, String> {
        let settings = host.browser_settings();
        if settings.use_ == BrowserUse::Off {
            return Err(BROWSER_OFF.into());
        }
        self.launch(&settings)
    }

    /// Renders `request` in the browser (started when none runs).
    pub async fn render(
        &self,
        host: &dyn BrowsingHost,
        request: &RenderRequest,
    ) -> Result<Snapshot, String> {
        let launch = self.ready(host)?;
        let _in_use = InUse::new(self, host);
        let started = std::time::Instant::now();
        let result = self.driver.render(&launch, request).await;
        tracing::info!(
            event = "browser.render",
            outcome = if result.is_ok() { "ok" } else { "failed" },
            took_ms = started.elapsed().as_millis() as u64,
            error = result.as_ref().err(),
            "Rendered a page in the browser"
        );
        result
    }

    /// Opens a visible window on `url`.
    pub async fn open_window(
        &self,
        host: &dyn BrowsingHost,
        url: &Url,
    ) -> Result<Box<dyn BrowserWindow>, String> {
        let launch = self.ready(host)?;
        let _in_use = InUse::new(self, host);
        let window = self.driver.open_window(&launch, url).await?;
        // The browser is in use, never idle, while the window is open.
        Ok(Box::new(HeldWindow::new(window, Arc::clone(&self.busy))))
    }

    /// Forgets `host`'s browser data.
    pub async fn clear_site(&self, host: &dyn BrowsingHost, site: &str) -> Result<(), String> {
        let launch = self.ready(host)?;
        let _in_use = InUse::new(self, host);
        self.driver.clear_site(&launch, site).await
    }

    /// Shuts the browser down when it has had nothing to do for the idle time, or was
    /// turned off.
    pub async fn idle(&self, host: &dyn BrowsingHost) {
        if !self.driver.running() {
            return;
        }
        let settings = host.browser_settings();
        if settings.use_ == BrowserUse::Off {
            self.driver.shutdown().await;
            *lock(&self.last_used) = None;
            tracing::info!(
                event = "browser.stop",
                outcome = "off",
                "Shut the browser down"
            );
            return;
        }
        let idle_ms = i64::from(settings.idle_seconds) * 1000;
        let last = *lock(&self.last_used);
        let due = last.map_or(true, |at| host.now_ms() - at >= idle_ms);
        if due && self.busy.load(Ordering::SeqCst) == 0 {
            self.driver.shutdown().await;
            *lock(&self.last_used) = None;
            tracing::info!(
                event = "browser.stop",
                outcome = "idle",
                "Shut the browser down"
            );
        }
    }

    /// Shuts the browser down before the app quits, waiting for it (a few seconds at most).
    pub fn stop(&self) {
        if !self.driver.running() {
            return;
        }
        let (driver, runtime) = (Arc::clone(&self.driver), self.runtime.clone());
        // From a thread of its own: this may be called on one of the runtime's.
        let stopped = std::thread::spawn(move || runtime.block_on(driver.shutdown())).join();
        *lock(&self.last_used) = None;
        tracing::info!(
            event = "browser.stop",
            outcome = if stopped.is_ok() { "quit" } else { "failed" },
            "Shut the browser down"
        );
    }

    /// Shuts the browser down whenever it falls idle, for as long as the app runs.
    pub async fn run(&self, host: &dyn BrowsingHost) {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            self.idle(host).await;
        }
    }
}

impl DownloadHost for super::Core {
    fn download_changed(&self, view: BrowserDownloadView) {
        self.emit(super::CoreEvent::BrowserDownload(view));
    }
}

impl super::Core {
    /// Refuses what needs the browser while it is off.
    pub(super) fn require_browser(&self) -> Result<(), String> {
        if self.app_settings().browser.use_ == BrowserUse::Off {
            return Err(BROWSER_OFF.into());
        }
        Ok(())
    }

    /// What browser would run, and the download.
    pub fn browser_status(&self) -> BrowserStatusView {
        self.browsing.status(self)
    }

    /// Downloads the pinned Chrome for Testing; returns once it is done, cancelled or failed.
    pub async fn download_browser(&self) -> Result<(), String> {
        self.browsing.download(self).await
    }

    pub fn cancel_browser_download(&self) {
        self.browsing.cancel_download();
    }

    /// Deletes the downloaded browser.
    pub async fn remove_browser(&self) -> Result<(), String> {
        self.browsing.remove_download().await
    }
}

impl BrowsingHost for super::Core {
    fn now_ms(&self) -> i64 {
        super::now_ms()
    }

    fn browser_settings(&self) -> BrowserSettings {
        self.app_settings().browser
    }
}

/// The app's driver: Chrome or Chromium through `aoide-browser`, one process at a time
/// (headless to render, with a window to sign in; they share the profile).
#[derive(Default)]
pub struct ChromeDriver {
    current: tokio::sync::Mutex<Option<Running>>,
    /// Whether a process runs, readable without waiting for `current`.
    alive: std::sync::atomic::AtomicBool,
}

impl ChromeDriver {
    /// The running browser, started (or restarted) with a window when `visible`. A visible
    /// one also renders (in a tab of its own), so a sign-in window stays open meanwhile.
    async fn browser(
        &self,
        launch: &Launch,
        visible: bool,
    ) -> Result<tokio::sync::MappedMutexGuard<'_, aoide_browser::Browser>, String> {
        let mut current = self.current.lock().await;
        let usable = match current.as_ref() {
            Some(running) => (running.visible || !visible) && running.browser.running().await,
            None => false,
        };
        if !usable {
            if let Some(old) = current.take() {
                old.browser.shutdown().await;
            }
            let browser = aoide_browser::Browser::launch(aoide_browser::BrowserOptions {
                binary: launch.binary.clone(),
                profile_dir: launch.profile_dir.clone(),
                page_timeout: launch.page_timeout,
                reach: aoide_fetch::Reach::Public,
                visible,
            })
            .await
            .map_err(|e| e.to_string());
            tracing::info!(
                event = "browser.start",
                visible,
                outcome = if browser.is_ok() { "ok" } else { "failed" },
                error = browser.as_ref().err(),
                "Started the browser"
            );
            let browser = browser?;
            *current = Some(Running { browser, visible });
            self.alive.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        tokio::sync::MutexGuard::try_map(current, |c| c.as_mut().map(|r| &mut r.browser))
            .map_err(|_| "the browser is not running".to_owned())
    }
}

/// The browser process the driver runs, and whether it has a window (it was started to sign
/// in) or is headless.
struct Running {
    browser: aoide_browser::Browser,
    visible: bool,
}

struct ChromeWindow(aoide_browser::Window);

impl BrowserWindow for ChromeWindow {
    fn snapshot(&self) -> Pending<'_, Result<Snapshot, String>> {
        Box::pin(async move { self.0.snapshot().await.map_err(|e| e.to_string()) })
    }

    fn closed(&self) -> Pending<'_, bool> {
        Box::pin(self.0.closed())
    }

    fn show(&self) -> Pending<'_, ()> {
        Box::pin(self.0.show())
    }

    fn close(self: Box<Self>) -> Pending<'static, ()> {
        Box::pin(self.0.close())
    }
}

impl BrowserDriver for ChromeDriver {
    fn render<'a>(
        &'a self,
        launch: &'a Launch,
        request: &'a RenderRequest,
    ) -> Pending<'a, Result<Snapshot, String>> {
        Box::pin(async move {
            let browser = self.browser(launch, false).await?;
            browser.render(request).await.map_err(|e| e.to_string())
        })
    }

    fn open_window<'a>(
        &'a self,
        launch: &'a Launch,
        url: &'a Url,
    ) -> Pending<'a, Result<Box<dyn BrowserWindow>, String>> {
        Box::pin(async move {
            let browser = self.browser(launch, true).await?;
            let window = browser.open_window(url).await.map_err(|e| e.to_string())?;
            Ok(Box::new(ChromeWindow(window)) as Box<dyn BrowserWindow>)
        })
    }

    fn clear_site<'a>(
        &'a self,
        launch: &'a Launch,
        host: &'a str,
    ) -> Pending<'a, Result<(), String>> {
        Box::pin(async move {
            // Whichever browser runs; a headless one when none does.
            let browser = self.browser(launch, false).await?;
            browser.clear_site(host).await.map_err(|e| e.to_string())
        })
    }

    fn running(&self) -> bool {
        self.alive.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn shutdown(&self) -> Pending<'_, ()> {
        Box::pin(async move {
            if let Some(running) = self.current.lock().await.take() {
                running.browser.shutdown().await;
            }
            self.alive.store(false, std::sync::atomic::Ordering::SeqCst);
        })
    }
}

/// A browser that answers from a script, for tests.
#[cfg(test)]
pub(super) mod testing {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    use aoide_fetch::Via;

    use super::*;

    /// Renders each address as scripted, and counts what it was asked to do.
    #[derive(Default)]
    pub struct ScriptedBrowser {
        pages: Mutex<HashMap<String, Result<String, String>>>,
        /// Each render asked for, in order.
        pub rendered: Mutex<Vec<RenderRequest>>,
        /// The program each start ran.
        pub started: Mutex<Vec<PathBuf>>,
        /// The sites whose data was cleared.
        pub cleared: Mutex<Vec<String>>,
        /// The window the user signs in with.
        pub window: Arc<WindowScript>,
        pub shutdowns: AtomicUsize,
        running: AtomicBool,
    }

    impl ScriptedBrowser {
        pub fn set(&self, url: &str, page: Result<&str, &str>) {
            let page = page.map(str::to_owned).map_err(str::to_owned);
            self.pages.lock().unwrap().insert(url.into(), page);
        }

        pub fn renders(&self) -> usize {
            self.rendered.lock().unwrap().len()
        }
    }

    impl BrowserDriver for ScriptedBrowser {
        fn render<'a>(
            &'a self,
            launch: &'a Launch,
            request: &'a RenderRequest,
        ) -> Pending<'a, Result<Snapshot, String>> {
            Box::pin(async move {
                if !self.running.swap(true, Ordering::SeqCst) {
                    self.started.lock().unwrap().push(launch.binary.clone());
                }
                self.rendered.lock().unwrap().push(request.clone());
                let page = self
                    .pages
                    .lock()
                    .unwrap()
                    .get(request.url.as_str())
                    .cloned();
                let html = page.unwrap_or_else(|| Err("Nothing is scripted here.".into()))?;
                Ok(Snapshot {
                    requested: request.url.clone(),
                    final_url: request.url.clone(),
                    status: 200,
                    html,
                    fetched_at_ms: 2_000,
                    via: Via::Browser,
                })
            })
        }

        fn open_window<'a>(
            &'a self,
            _: &'a Launch,
            url: &'a Url,
        ) -> Pending<'a, Result<Box<dyn BrowserWindow>, String>> {
            Box::pin(async move {
                let script = &self.window;
                if let Some(error) = script.fails.lock().unwrap().clone() {
                    return Err(error);
                }
                self.running.store(true, Ordering::SeqCst);
                *script.at.lock().unwrap() = Some(url.clone());
                script.opened.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(ScriptedWindow(Arc::clone(script))) as Box<dyn BrowserWindow>)
            })
        }

        fn clear_site<'a>(
            &'a self,
            _: &'a Launch,
            host: &'a str,
        ) -> Pending<'a, Result<(), String>> {
            Box::pin(async move {
                self.cleared.lock().unwrap().push(host.to_owned());
                Ok(())
            })
        }

        fn running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }

        fn shutdown(&self) -> Pending<'_, ()> {
            Box::pin(async move {
                self.running.store(false, Ordering::SeqCst);
                self.shutdowns.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    /// What the sign-in window shows, and what was done with it.
    #[derive(Default)]
    pub struct WindowScript {
        /// Why opening it fails.
        pub fails: Mutex<Option<String>>,
        /// The address it is on.
        pub at: Mutex<Option<Url>>,
        /// The page it shows (a blank one until set).
        pub page: Mutex<String>,
        /// The user closed it.
        pub closed_by_user: AtomicBool,
        pub opened: AtomicUsize,
        pub shown: AtomicUsize,
        /// Aoide closed it.
        pub closed: AtomicBool,
    }

    impl WindowScript {
        /// The user moved to `url`, which shows `html`.
        pub fn shows(&self, url: &str, html: &str) {
            *self.at.lock().unwrap() = Some(Url::parse(url).unwrap());
            *self.page.lock().unwrap() = html.to_owned();
        }
    }

    struct ScriptedWindow(Arc<WindowScript>);

    impl BrowserWindow for ScriptedWindow {
        fn snapshot(&self) -> Pending<'_, Result<Snapshot, String>> {
            Box::pin(async move {
                let at = self.0.at.lock().unwrap().clone().ok_or("no page")?;
                Ok(Snapshot {
                    requested: at.clone(),
                    final_url: at,
                    status: 200,
                    html: self.0.page.lock().unwrap().clone(),
                    fetched_at_ms: 3_000,
                    via: Via::Browser,
                })
            })
        }

        fn closed(&self) -> Pending<'_, bool> {
            Box::pin(async move { self.0.closed_by_user.load(Ordering::SeqCst) })
        }

        fn show(&self) -> Pending<'_, ()> {
            Box::pin(async move {
                self.0.shown.fetch_add(1, Ordering::SeqCst);
            })
        }

        fn close(self: Box<Self>) -> Pending<'static, ()> {
            Box::pin(async move {
                self.0.closed.store(true, Ordering::SeqCst);
            })
        }
    }

    /// A file that runs, standing in for Chrome's program.
    pub fn fake_program(dir: &Path) -> PathBuf {
        let path = dir.join("fake-chrome");
        std::fs::write(&path, "#!/bin/sh\necho Chromium 1.0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_article::WallKind;

    fn rule(fetch: Option<SiteFetch>, archive: bool) -> SiteRule {
        SiteRule {
            fetch,
            fallbacks: if archive {
                vec![Fallback::Archive]
            } else {
                vec![]
            },
            ..SiteRule::default()
        }
    }

    #[test]
    fn routes_follow_the_choice_the_site_and_what_was_got_before() {
        use BrowserUse::*;
        use Route::*;
        let cut = Some(Completeness::CutShort {
            captured: 10,
            declared: 100,
        });
        let wall = Some(Completeness::Wall(WallKind::SignIn));
        let whole = Some(Completeness::Complete);
        let http = Some(rule(Some(SiteFetch::Http), false));
        let browser = Some(rule(Some(SiteFetch::Browser), false));
        let auto = Some(rule(None, false));
        let archive = Some(rule(None, true));
        #[allow(clippy::type_complexity)]
        let table: Vec<(
            BrowserUse,
            &Option<SiteRule>,
            Option<Completeness>,
            Vec<Route>,
        )> = vec![
            // Off: never the browser, whatever the site says.
            (Off, &None, None, vec![Http]),
            (Off, &None, cut, vec![Http]),
            (Off, &browser, None, vec![Http]),
            (Off, &http, None, vec![Http]),
            (Off, &archive, None, vec![Http, Archive]),
            // Fallback: HTTP, then the browser; straight to it once HTTP fell short.
            (Fallback, &None, None, vec![Http, Browser]),
            (Fallback, &None, whole, vec![Http, Browser]),
            (Fallback, &None, cut, vec![Browser]),
            (Fallback, &None, wall, vec![Browser]),
            (Fallback, &auto, None, vec![Http, Browser]),
            (Fallback, &http, cut, vec![Http]),
            (Fallback, &browser, None, vec![Browser]),
            (Fallback, &archive, None, vec![Http, Browser, Archive]),
            // First: the browser, unless the site wants plain HTTP.
            (First, &None, None, vec![Browser]),
            (First, &None, whole, vec![Browser]),
            (First, &http, None, vec![Http]),
            (First, &browser, cut, vec![Browser]),
            (First, &archive, None, vec![Browser, Archive]),
        ];
        for (use_, rule, previous, routes) in table {
            assert_eq!(
                plan_fetch(use_, rule.as_ref(), previous),
                routes,
                "{use_:?}, {rule:?}, {previous:?}"
            );
        }
    }

    #[test]
    fn a_site_asking_for_the_browser_while_it_is_off_is_noticed() {
        let browser = rule(Some(SiteFetch::Browser), false);
        assert!(browser_wanted_but_off(BrowserUse::Off, Some(&browser)));
        assert!(!browser_wanted_but_off(
            BrowserUse::Fallback,
            Some(&browser)
        ));
        assert!(!browser_wanted_but_off(BrowserUse::Off, None));
    }

    /// A clock and settings the test sets.
    struct Host {
        now: Mutex<i64>,
        settings: Mutex<BrowserSettings>,
    }

    impl DownloadHost for Host {
        fn download_changed(&self, _: BrowserDownloadView) {}
    }

    impl BrowsingHost for Host {
        fn now_ms(&self) -> i64 {
            *self.now.lock().unwrap()
        }

        fn browser_settings(&self) -> BrowserSettings {
            self.settings.lock().unwrap().clone()
        }
    }

    #[tokio::test]
    async fn the_browser_is_shut_down_once_idle_or_turned_off_and_started_again_on_need() {
        use std::sync::atomic::Ordering;
        let dir = tempfile::tempdir().unwrap();
        let driver = Arc::new(testing::ScriptedBrowser::default());
        let browsing = Browsing::new(driver.clone(), dir.path().join("browser"), dir.path(), None);
        let host = Host {
            now: Mutex::new(1_000_000),
            settings: Mutex::new(BrowserSettings {
                use_: BrowserUse::Fallback,
                path: Some(testing::fake_program(dir.path()).to_string_lossy().into()),
                idle_seconds: 30,
            }),
        };
        let page = RenderRequest::new(Url::parse("https://quay.example/").unwrap());
        let render = || async { browsing.render(&host, &page).await };
        let _ = render().await;
        assert!(driver.running());

        *host.now.lock().unwrap() += 29_999;
        browsing.idle(&host).await;
        assert!(driver.running(), "not idle long enough");
        *host.now.lock().unwrap() += 1;
        browsing.idle(&host).await;
        assert!(!driver.running());
        assert_eq!(driver.shutdowns.load(Ordering::SeqCst), 1);

        // Used again: started again, and kept while in use however long that takes.
        let _ = render().await;
        assert_eq!(driver.started.lock().unwrap().len(), 2);
        browsing.busy.fetch_add(1, Ordering::SeqCst);
        *host.now.lock().unwrap() += 3_600_000;
        browsing.idle(&host).await;
        assert!(driver.running(), "busy");
        browsing.busy.fetch_sub(1, Ordering::SeqCst);

        // Turned off: shut down at once, and not started for anything.
        host.settings.lock().unwrap().use_ = BrowserUse::Off;
        *host.now.lock().unwrap() += 1;
        let _ = render().await;
        browsing.idle(&host).await;
        assert!(!driver.running());
        assert_eq!(render().await, Err(BROWSER_OFF.into()));
        assert_eq!(driver.started.lock().unwrap().len(), 2);
        assert_eq!(driver.renders(), 2);
    }

    #[test]
    fn settings_saved_before_the_browser_existed_keep_it_off() {
        let settings: super::super::AppSettings =
            serde_json::from_str(r#"{"readingWpm":250}"#).unwrap();
        assert_eq!(settings.browser, BrowserSettings::default());
        assert_eq!(settings.browser.use_, BrowserUse::Off);
        let first: BrowserSettings = serde_json::from_str(r#"{"use":"first"}"#).unwrap();
        assert_eq!((first.use_, first.idle_seconds), (BrowserUse::First, 60));
    }
}
