//! Renders web pages in a real Chrome or Chromium, for pages plain HTTP cannot read, and opens
//! a visible window for signing in. Nothing here runs unless the app asks for it.
//!
//! The browser runs with its own profile (kept between runs, so sign-ins and passed checks
//! stay), its sandbox on, downloads and permission prompts denied, and every request sent
//! through [`proxy::Proxy`], which enforces [`Reach`]. The DevTools connection is a loopback
//! port the system picks. The rest of the app sees only this crate's types.

pub mod find;
pub mod patterns;
pub mod profile;
pub mod proxy;
pub mod ready;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub use aoide_fetch::{Reach, Snapshot, Via};
use chromiumoxide::cdp::browser_protocol::{browser as cdp_browser, fetch, network, page, storage};
use chromiumoxide::{BrowserConfig, Page};
use futures::StreamExt;
use url::Url;

use ready::{ReadyEvent, ReadyRules, Stage, Watch};

#[derive(Debug, Clone)]
pub struct BrowserOptions {
    pub binary: PathBuf,
    pub profile_dir: PathBuf,
    /// The longest a page is waited for; what is there by then is taken.
    pub page_timeout: Duration,
    pub reach: Reach,
    /// A normal window (for signing in) instead of headless.
    pub visible: bool,
}

/// One page to render.
#[derive(Debug, Clone)]
pub struct RenderRequest {
    pub url: Url,
    /// Clear the site's cookies and storage before loading it.
    pub clear_site_data: bool,
    /// Address patterns not to load ([`patterns::matches`]).
    pub block: Vec<String>,
    /// A CSS selector that marks the article as present.
    pub wait_for: Option<String>,
    pub user_agent: Option<String>,
}

impl RenderRequest {
    pub fn new(url: Url) -> Self {
        Self {
            url,
            clear_site_data: false,
            block: Vec::new(),
            wait_for: None,
            user_agent: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BrowserError {
    #[error("The browser could not be started: {0}")]
    Launch(String),
    #[error("The page could not be loaded: {0}")]
    Load(String),
    #[error("Only web addresses can be opened in the browser.")]
    NotWeb,
    #[error("The browser stopped: {0}")]
    Protocol(String),
    #[error("A window can only be opened by a browser started to show one.")]
    Headless,
}

fn protocol(error: impl std::fmt::Display) -> BrowserError {
    BrowserError::Protocol(error.to_string())
}

/// A running browser. Dropping it kills the process; [`Browser::shutdown`] closes it first.
pub struct Browser {
    inner: chromiumoxide::Browser,
    handler: tokio::task::JoinHandle<()>,
    _proxy: proxy::Proxy,
    options: BrowserOptions,
    /// One page renders at a time.
    rendering: tokio::sync::Mutex<()>,
}

impl Browser {
    pub async fn launch(options: BrowserOptions) -> Result<Self, BrowserError> {
        std::fs::create_dir_all(&options.profile_dir)
            .map_err(|e| BrowserError::Launch(e.to_string()))?;
        if profile::clear_stale_lock(&options.profile_dir) {
            tracing::info!(
                event = "browser.profile",
                outcome = "unlocked",
                "Cleared a lock a crashed browser left"
            );
        }
        let proxy = proxy::Proxy::start(options.reach)
            .await
            .map_err(|e| BrowserError::Launch(e.to_string()))?;
        let proxy_server = format!("http://{}", proxy.address);
        let mut args: Vec<(&str, Option<&str>)> = vec![
            ("proxy-server", Some(&proxy_server)),
            // Loopback addresses go through the proxy too, which decides.
            ("proxy-bypass-list", Some("<-loopback>")),
            (
                "force-webrtc-ip-handling-policy",
                Some("disable_non_proxied_udp"),
            ),
            ("deny-permission-prompts", None),
            ("no-first-run", None),
            ("no-default-browser-check", None),
            ("disable-background-networking", None),
            ("disable-component-update", None),
            ("disable-default-apps", None),
            ("disable-sync", None),
            ("disable-breakpad", None),
            ("disable-features", Some("Translate,MediaRouter")),
            ("password-store", Some("basic")),
            ("use-mock-keychain", None),
        ];
        let mut config = BrowserConfig::builder()
            .chrome_executable(&options.binary)
            .user_data_dir(&options.profile_dir)
            .disable_default_args()
            .respect_https_errors()
            .launch_timeout(Duration::from_secs(30))
            .request_timeout(Duration::from_secs(30));
        if options.visible {
            config = config.with_head().viewport(None);
        } else {
            config = config
                .new_headless_mode()
                .window_size(1280, 1000)
                .viewport(None);
            args.push(("mute-audio", None));
        }
        for (key, value) in args {
            config = match value {
                Some(value) => config.arg((key, value)),
                None => config.arg(key),
            };
        }
        let config = config.build().map_err(BrowserError::Launch)?;
        let (inner, mut events) = chromiumoxide::Browser::launch(config)
            .await
            .map_err(|e| BrowserError::Launch(e.to_string()))?;
        let handler = tokio::spawn(async move { while events.next().await.is_some() {} });
        let browser = Self {
            inner,
            handler,
            _proxy: proxy,
            options,
            rendering: tokio::sync::Mutex::new(()),
        };
        browser
            .inner
            .execute(
                cdp_browser::SetDownloadBehaviorParams::builder()
                    .behavior(cdp_browser::SetDownloadBehaviorBehavior::Deny)
                    .build()
                    .map_err(BrowserError::Launch)?,
            )
            .await
            .map_err(protocol)?;
        Ok(browser)
    }

    /// The browser's version, as it reports it.
    pub async fn version(&self) -> Result<String, BrowserError> {
        let version = self.inner.version().await.map_err(protocol)?;
        Ok(version.product)
    }

    /// Loads `request.url` in a new tab, waits until it is ready (or the page timeout), and
    /// returns its DOM and final address.
    pub async fn render(&self, request: &RenderRequest) -> Result<Snapshot, BrowserError> {
        web(&request.url)?;
        aoide_fetch::check_reach(&request.url, self.options.reach)
            .await
            .map_err(|e| BrowserError::Load(e.to_string()))?;
        let _one = self.rendering.lock().await;
        if request.clear_site_data {
            if let Some(host) = request.url.host_str() {
                self.clear_site(host).await?;
                // Its exact origin too, when it names a port.
                self.clear_origin(&request.url.origin().ascii_serialization())
                    .await;
            }
        }
        let page = self.inner.new_page("about:blank").await.map_err(protocol)?;
        let result = self.load(&page, request).await;
        let _ = page.close().await;
        result
    }

    async fn load(&self, page: &Page, request: &RenderRequest) -> Result<Snapshot, BrowserError> {
        if let Some(agent) = &request.user_agent {
            page.set_user_agent(agent.as_str())
                .await
                .map_err(protocol)?;
        }
        let blocker = if request.block.is_empty() {
            None
        } else {
            Some(block(page, request.block.clone()).await?)
        };
        let started = Instant::now();
        let at = move || started.elapsed().as_millis() as u64;
        let rules = ReadyRules {
            timeout_ms: self.options.page_timeout.as_millis() as u64,
            selector: request.wait_for.is_some(),
            ..ReadyRules::default()
        };
        let (sender, mut observed) = tokio::sync::mpsc::unbounded_channel::<ReadyEvent>();
        let watchers = watch_page(page, sender.clone(), at).await?;
        let status = Arc::new(std::sync::Mutex::new(None::<u16>));
        let statuses = watch_status(page, Arc::clone(&status)).await?;
        let navigated = page
            .execute(page::NavigateParams::new(request.url.as_str()))
            .await
            .map_err(protocol)?;
        if let Some(error) = &navigated.result.error_text {
            for task in [watchers, statuses].into_iter().chain(blocker) {
                task.abort();
            }
            return Err(BrowserError::Load(load_error(error)));
        }
        let mut watch = Watch::start(0);
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        while !watch.stage.is_over() {
            tokio::select! {
                Some(event) = observed.recv() => watch = ready::next(watch, event, &rules),
                _ = tick.tick() => {
                    watch = ready::next(watch, ReadyEvent::Tick { at: at() }, &rules);
                    if watch.wants_sample() {
                        let (found, text_len) = sample(page, request.wait_for.as_deref()).await;
                        watch = ready::next(watch, ReadyEvent::Sampled { at: at(), found, text_len }, &rules);
                    }
                }
            }
        }
        for task in [watchers, statuses].into_iter().chain(blocker) {
            task.abort();
        }
        if watch.stage == Stage::TimedOut {
            tracing::info!(
                event = "browser.render",
                outcome = "timed_out",
                "The page did not settle; taking what is there"
            );
        }
        let mut snapshot = snapshot(page, &request.url).await?;
        if let Some(status) = *status.lock().unwrap_or_else(|e| e.into_inner()) {
            snapshot.status = status;
        }
        Ok(snapshot)
    }

    /// Opens `url` in a visible tab, for the user to sign in or pass a check.
    pub async fn open_window(&self, url: &Url) -> Result<Window, BrowserError> {
        web(url)?;
        aoide_fetch::check_reach(url, self.options.reach)
            .await
            .map_err(|e| BrowserError::Load(e.to_string()))?;
        if !self.options.visible {
            return Err(BrowserError::Headless);
        }
        let page = self.inner.new_page(url.as_str()).await.map_err(protocol)?;
        let _ = page.bring_to_front().await;
        Ok(Window {
            page,
            requested: url.clone(),
        })
    }

    /// Forgets `host`'s cookies (its subdomains' too) and storage.
    pub async fn clear_site(&self, host: &str) -> Result<(), BrowserError> {
        let host = host.trim_start_matches("www.").to_ascii_lowercase();
        let cookies = self
            .inner
            .execute(storage::GetCookiesParams::default())
            .await
            .map_err(protocol)?;
        let doomed: Vec<_> = cookies
            .result
            .cookies
            .iter()
            .filter(|cookie| {
                let domain = cookie.domain.trim_start_matches('.').to_ascii_lowercase();
                domain == host || domain.ends_with(&format!(".{host}"))
            })
            .collect();
        if !doomed.is_empty() {
            // Cookies are deleted one by one through a page (the browser itself cannot).
            let page = self.inner.new_page("about:blank").await.map_err(protocol)?;
            for cookie in doomed {
                let scheme = if cookie.secure { "https" } else { "http" };
                let domain = cookie.domain.trim_start_matches('.');
                let delete = network::DeleteCookiesParams {
                    name: cookie.name.clone(),
                    url: Some(format!("{scheme}://{domain}{}", cookie.path)),
                    domain: None,
                    path: None,
                    partition_key: None,
                };
                if let Err(error) = page.execute(delete).await {
                    let _ = page.close().await;
                    return Err(protocol(error));
                }
            }
            let _ = page.close().await;
        }
        let mut names = vec![host.clone()];
        if host.parse::<std::net::IpAddr>().is_err() {
            names.push(format!("www.{host}"));
        }
        for name in names {
            for scheme in ["https", "http"] {
                self.clear_origin(&format!("{scheme}://{name}")).await;
            }
        }
        Ok(())
    }

    /// Clears one origin's storage; an origin the browser does not take is passed over.
    async fn clear_origin(&self, origin: &str) {
        let cleared = self
            .inner
            .execute(storage::ClearDataForOriginParams::new(origin, "all"))
            .await;
        if let Err(error) = cleared {
            tracing::debug!(event = "browser.clear", %error, "Could not clear an origin's storage");
        }
    }

    /// Whether the browser process is still there.
    pub async fn running(&self) -> bool {
        !self.handler.is_finished() && self.inner.version().await.is_ok()
    }

    /// Closes the browser and waits for its process to end.
    pub async fn shutdown(mut self) {
        let closed = tokio::time::timeout(Duration::from_secs(5), self.inner.close()).await;
        if !matches!(closed, Ok(Ok(_))) {
            let _ = self.inner.kill().await;
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), self.inner.wait()).await;
        self.handler.abort();
    }
}

/// A visible tab the user works in.
pub struct Window {
    page: Page,
    requested: Url,
}

impl Window {
    /// The tab's DOM and address now.
    pub async fn snapshot(&self) -> Result<Snapshot, BrowserError> {
        snapshot(&self.page, &self.requested).await
    }

    /// Whether the user closed it (or the browser).
    pub async fn closed(&self) -> bool {
        tokio::time::timeout(Duration::from_secs(2), self.page.url())
            .await
            .map_or(true, |url| url.is_err())
    }

    pub async fn show(&self) {
        let _ = self.page.bring_to_front().await;
    }

    pub async fn close(self) {
        let _ = self.page.close().await;
    }
}

fn web(url: &Url) -> Result<(), BrowserError> {
    if matches!(url.scheme(), "http" | "https") {
        Ok(())
    } else {
        Err(BrowserError::NotWeb)
    }
}

/// What a navigation error means to the user.
fn load_error(error: &str) -> String {
    match error {
        "net::ERR_TUNNEL_CONNECTION_FAILED" | "net::ERR_PROXY_CONNECTION_FAILED" => {
            "The address is on this computer or its local network, or does not answer.".into()
        }
        "net::ERR_NAME_NOT_RESOLVED" => "The site's name does not resolve.".into(),
        other => other.trim_start_matches("net::").to_owned(),
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// The page's DOM and address, as a snapshot of `requested`.
async fn snapshot(page: &Page, requested: &Url) -> Result<Snapshot, BrowserError> {
    let html = page.content().await.map_err(protocol)?;
    let final_url = page
        .url()
        .await
        .map_err(protocol)?
        .and_then(|u| Url::parse(&u).ok())
        .filter(|u| matches!(u.scheme(), "http" | "https"))
        .ok_or_else(|| BrowserError::Load("the page did not open".into()))?;
    Ok(Snapshot {
        requested: requested.clone(),
        final_url,
        status: 200,
        html,
        fetched_at_ms: now_ms(),
        via: Via::Browser,
    })
}

/// Whether `selector` is on the page, and how long its text is.
async fn sample(page: &Page, selector: Option<&str>) -> (bool, usize) {
    let selector = serde_json_string(selector.unwrap_or(""));
    let script = format!(
        "(() => {{ const s = {selector}; let found = false; \
         try {{ found = s !== '' && document.querySelector(s) !== null; }} catch (e) {{}} \
         const len = document.body ? document.body.innerText.length : 0; \
         return [found, len]; }})()"
    );
    let Ok(result) = page.evaluate(script).await else {
        return (false, 0);
    };
    let value: Option<(bool, usize)> = result.into_value().ok();
    value.unwrap_or((false, 0))
}

/// `text` as a JavaScript string literal.
fn serde_json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Sends the page's load and network events to `sender`, timed by `at`.
async fn watch_page(
    page: &Page,
    sender: tokio::sync::mpsc::UnboundedSender<ReadyEvent>,
    at: impl Fn() -> u64 + Send + Sync + Copy + 'static,
) -> Result<tokio::task::JoinHandle<()>, BrowserError> {
    let mut loads = page
        .event_listener::<page::EventLoadEventFired>()
        .await
        .map_err(protocol)?;
    let mut starts = page
        .event_listener::<network::EventRequestWillBeSent>()
        .await
        .map_err(protocol)?;
    let mut finishes = page
        .event_listener::<network::EventLoadingFinished>()
        .await
        .map_err(protocol)?;
    let mut failures = page
        .event_listener::<network::EventLoadingFailed>()
        .await
        .map_err(protocol)?;
    Ok(tokio::spawn(async move {
        loop {
            let event = tokio::select! {
                Some(_) = loads.next() => ReadyEvent::Loaded { at: at() },
                Some(_) = starts.next() => ReadyEvent::RequestStarted { at: at() },
                Some(_) = finishes.next() => ReadyEvent::RequestEnded { at: at() },
                Some(_) = failures.next() => ReadyEvent::RequestEnded { at: at() },
                else => break,
            };
            if sender.send(event).is_err() {
                break;
            }
        }
    }))
}

/// Keeps the main document's status in `status`.
async fn watch_status(
    page: &Page,
    status: Arc<std::sync::Mutex<Option<u16>>>,
) -> Result<tokio::task::JoinHandle<()>, BrowserError> {
    let mut responses = page
        .event_listener::<network::EventResponseReceived>()
        .await
        .map_err(protocol)?;
    let main = page.mainframe().await.map_err(protocol)?;
    Ok(tokio::spawn(async move {
        while let Some(event) = responses.next().await {
            let document = event.r#type == network::ResourceType::Document;
            if document && (main.is_none() || event.frame_id == main) {
                *status.lock().unwrap_or_else(|e| e.into_inner()) =
                    u16::try_from(event.response.status).ok();
            }
        }
    }))
}

/// Fails the page's requests whose address matches one of `patterns`.
async fn block(
    page: &Page,
    patterns: Vec<String>,
) -> Result<tokio::task::JoinHandle<()>, BrowserError> {
    let mut paused = page
        .event_listener::<fetch::EventRequestPaused>()
        .await
        .map_err(protocol)?;
    page.execute(
        fetch::EnableParams::builder()
            .pattern(fetch::RequestPattern::builder().url_pattern("*").build())
            .build(),
    )
    .await
    .map_err(protocol)?;
    let page = page.clone();
    Ok(tokio::spawn(async move {
        while let Some(event) = paused.next().await {
            let url = &event.request.url;
            let blocked = patterns.iter().any(|p| patterns::matches(p, url));
            let id = event.request_id.clone();
            let _ = if blocked {
                page.execute(fetch::FailRequestParams::new(
                    id,
                    network::ErrorReason::BlockedByClient,
                ))
                .await
                .map(drop)
            } else {
                page.execute(fetch::ContinueRequestParams::new(id))
                    .await
                    .map(drop)
            };
        }
    }))
}
