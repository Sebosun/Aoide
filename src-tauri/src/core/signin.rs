//! Signing in to a site, or passing its "checking your browser" page, in a visible browser
//! window that shares the browser's profile, then fetching the article with what that left
//! (cookies). One flow at a time; its stage is an explicit enum moved by [`next`].

use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_article::Completeness;
use aoide_fetch::Snapshot;
use serde::Serialize;
use tokio::sync::mpsc;
use url::Url;

use super::browsing::{BrowserWindow, Pending};
use super::fetching::{Reason, Stage as FetchStage};
use super::lock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SignInStage {
    /// The window is being opened.
    Opening,
    /// The user signs in (or passes the check) in the window.
    Waiting,
    /// The article is being fetched with the session.
    Fetching,
    Done,
    Cancelled,
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignInEvent {
    WindowOpened,
    /// The window shows the whole article on the article's site.
    ArticleSeen,
    /// "I'm signed in, fetch again".
    UserSaidDone,
    WindowClosed,
    Fetched,
    Cancelled,
    BrowserFailed(String),
}

/// The one place a sign-in's stage changes.
pub fn next(stage: &SignInStage, event: SignInEvent) -> SignInStage {
    use SignInEvent as E;
    use SignInStage as S;
    match (stage, event) {
        (S::Opening, E::WindowOpened) => S::Waiting,
        (S::Opening | S::Waiting, E::Cancelled) => S::Cancelled,
        (S::Waiting, E::ArticleSeen | E::UserSaidDone) => S::Fetching,
        (S::Waiting, E::WindowClosed) => S::Cancelled,
        (S::Fetching, E::Fetched) => S::Done,
        (S::Opening | S::Fetching, E::BrowserFailed(error)) => S::Failed { error },
        (stage, _) => stage.clone(),
    }
}

impl SignInStage {
    fn name(&self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Waiting => "waiting",
            Self::Fetching => "fetching",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed { .. } => "failed",
        }
    }

    fn finished(&self) -> bool {
        matches!(self, Self::Done | Self::Cancelled | Self::Failed { .. })
    }
}

/// A sign-in, as its dialog shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SignInView {
    pub article_id: String,
    /// The site signed in to (`www.` dropped).
    pub site: String,
    /// What the article needed when the flow started.
    pub reason: Option<Reason>,
    pub stage: SignInStage,
    /// Keep the site's cookies after the fetch ("Stay signed in to this site").
    pub stay_signed_in: bool,
    /// The article's fetch stage once fetched.
    pub result: Option<FetchStage>,
}

/// What signing in needs from the rest of the app.
pub(super) trait SignInHost: Send + Sync {
    /// The article's address and what it needs from the user now.
    fn article_page(&self, id: &str) -> Result<(Url, Option<Reason>), String>;
    fn open_window<'a>(
        &'a self,
        url: &'a Url,
    ) -> Pending<'a, Result<Box<dyn BrowserWindow>, String>>;
    /// The page rendered headless with the profile as it is (its data is not cleared first).
    fn render_page<'a>(&'a self, url: &'a Url) -> Pending<'a, Result<Snapshot, String>>;
    /// Takes `snapshot` as the article's fetch; returns the stage it ends in.
    fn receive<'a>(
        &'a self,
        id: &'a str,
        snapshot: Snapshot,
    ) -> Pending<'a, Result<Option<FetchStage>, String>>;
    /// The user signed in to `site` (or passed its check, by `reason`).
    fn session_kept(&self, site: &str, reason: Option<Reason>);
    fn clear_site<'a>(&'a self, site: &'a str) -> Pending<'a, Result<(), String>>;
    fn sign_in_changed(&self, view: SignInView);
}

/// What the user asks of a running flow.
enum Ask {
    Done,
    Cancel,
    Show,
}

/// How often the window is looked at for the article.
const POLL: Duration = if cfg!(test) {
    Duration::from_millis(30)
} else {
    Duration::from_secs(2)
};

struct Flow {
    view: SignInView,
    asks: mpsc::UnboundedSender<Ask>,
}

#[derive(Default)]
pub(super) struct SignIn {
    flow: Mutex<Option<Flow>>,
}

impl SignIn {
    pub fn view(&self) -> Option<SignInView> {
        lock(&self.flow).as_ref().map(|f| f.view.clone())
    }

    /// Sets up a flow for article `id`; `run` then runs it. Refused while another is open.
    fn begin(
        &self,
        host: &dyn SignInHost,
        id: &str,
    ) -> Result<(Url, mpsc::UnboundedReceiver<Ask>), String> {
        let (url, reason) = host.article_page(id)?;
        let site = super::articles::site_of(url.as_str()).ok_or("That is not a web address.")?;
        let mut flow = lock(&self.flow);
        if flow.as_ref().is_some_and(|f| !f.view.stage.finished()) {
            return Err("A sign-in is already open. Finish or cancel it first.".into());
        }
        let (asks, receiver) = mpsc::unbounded_channel();
        let view = SignInView {
            article_id: id.into(),
            site,
            reason,
            stage: SignInStage::Opening,
            stay_signed_in: true,
            result: None,
        };
        *flow = Some(Flow {
            view: view.clone(),
            asks,
        });
        drop(flow);
        tracing::info!(
            event = "signin.start",
            book = crate::logging::book(id),
            "Opened a sign-in"
        );
        host.sign_in_changed(view);
        Ok((url, receiver))
    }

    fn step(&self, host: &dyn SignInHost, event: SignInEvent) -> SignInStage {
        let view = {
            let mut flow = lock(&self.flow);
            let Some(flow) = flow.as_mut() else {
                return SignInStage::Cancelled;
            };
            let to = next(&flow.view.stage, event);
            if to != flow.view.stage {
                tracing::info!(
                    event = "signin.stage",
                    book = crate::logging::book(&flow.view.article_id),
                    from = flow.view.stage.name(),
                    to = to.name(),
                    "Sign-in moved on"
                );
            }
            flow.view.stage = to;
            flow.view.clone()
        };
        let stage = view.stage.clone();
        host.sign_in_changed(view);
        stage
    }

    fn ask(&self, ask: Ask) -> Result<(), String> {
        let flow = lock(&self.flow);
        let flow = flow
            .as_ref()
            .filter(|f| !f.view.stage.finished())
            .ok_or("No sign-in is open.")?;
        let _ = flow.asks.send(ask);
        Ok(())
    }

    /// "I'm signed in, fetch again".
    pub fn finish(&self) -> Result<(), String> {
        self.ask(Ask::Done)
    }

    pub fn cancel(&self) -> Result<(), String> {
        self.ask(Ask::Cancel)
    }

    pub fn show(&self) -> Result<(), String> {
        self.ask(Ask::Show)
    }

    /// Whether the site's cookies are kept after the fetch.
    pub fn stay_signed_in(&self, host: &dyn SignInHost, stay: bool) -> Result<(), String> {
        let view = {
            let mut flow = lock(&self.flow);
            let flow = flow
                .as_mut()
                .filter(|f| !f.view.stage.finished())
                .ok_or("No sign-in is open.")?;
            flow.view.stay_signed_in = stay;
            flow.view.clone()
        };
        host.sign_in_changed(view);
        Ok(())
    }

    /// Runs the flow `begin` set up, until it is done, cancelled or failed.
    async fn run(&self, host: &dyn SignInHost, url: Url, mut asks: mpsc::UnboundedReceiver<Ask>) {
        let Some(view) = self.view() else { return };
        let window = match host.open_window(&url).await {
            Ok(window) => window,
            Err(error) => {
                self.step(host, SignInEvent::BrowserFailed(error));
                return;
            }
        };
        self.step(host, SignInEvent::WindowOpened);
        let seen = loop {
            tokio::select! {
                ask = asks.recv() => match ask {
                    Some(Ask::Done) => {
                        self.step(host, SignInEvent::UserSaidDone);
                        break None;
                    }
                    Some(Ask::Show) => window.show().await,
                    Some(Ask::Cancel) | None => {
                        self.step(host, SignInEvent::Cancelled);
                        window.close().await;
                        return;
                    }
                },
                () = tokio::time::sleep(POLL) => {
                    if window.closed().await {
                        self.step(host, SignInEvent::WindowClosed);
                        return;
                    }
                    if let Some(page) = whole_article(&*window, &view.site).await {
                        self.step(host, SignInEvent::ArticleSeen);
                        break Some(page);
                    }
                }
            }
        };
        // The window's own page when it shows the whole article; else one render with what
        // the window left in the profile.
        let page = match seen {
            Some(page) => Some(page),
            None => whole_article(&*window, &view.site).await,
        };
        window.close().await;
        let page = match page {
            Some(page) => page,
            None => match host.render_page(&url).await {
                Ok(page) => page,
                Err(error) => {
                    self.step(host, SignInEvent::BrowserFailed(error));
                    return;
                }
            },
        };
        let result = match host.receive(&view.article_id, page).await {
            Ok(stage) => stage,
            Err(error) => {
                self.step(host, SignInEvent::BrowserFailed(error));
                return;
            }
        };
        let stay = self.view().is_some_and(|v| v.stay_signed_in);
        if result == Some(FetchStage::Ready) {
            host.session_kept(&view.site, view.reason);
        }
        if !stay {
            if let Err(error) = host.clear_site(&view.site).await {
                tracing::warn!(event = "signin.clear", %error, "Could not clear the site after signing in");
            }
        }
        if let Some(flow) = lock(&self.flow).as_mut() {
            flow.view.result = result;
        }
        self.step(host, SignInEvent::Fetched);
    }
}

/// The window's page when it is on `site` and holds the whole article.
async fn whole_article(window: &dyn BrowserWindow, site: &str) -> Option<Snapshot> {
    let page = window.snapshot().await.ok()?;
    let here = page.final_url.host_str().map(aoide_config::site_host)?;
    let on_site = here == site || here.ends_with(&format!(".{site}"));
    let whole = super::articles::extract(&page.html, &page.final_url).completeness
        == Completeness::Complete;
    (on_site && whole).then_some(page)
}

impl SignInHost for super::Core {
    fn article_page(&self, id: &str) -> Result<(Url, Option<Reason>), String> {
        let view = self.articles.view(id)?;
        let url = Url::parse(&view.canonical_url).map_err(|e| e.to_string())?;
        let reason = match view.stage {
            FetchStage::NeedsYou(reason) => Some(reason),
            _ => None,
        };
        Ok((url, reason))
    }

    fn open_window<'a>(
        &'a self,
        url: &'a Url,
    ) -> Pending<'a, Result<Box<dyn BrowserWindow>, String>> {
        Box::pin(self.browsing.open_window(self, url))
    }

    fn render_page<'a>(&'a self, url: &'a Url) -> Pending<'a, Result<Snapshot, String>> {
        Box::pin(async move {
            let rule = url.host_str().and_then(|h| self.browsing.rule_for(h));
            let mut request = super::browsing::render_request(url, rule.as_ref());
            // The session just made is what this fetch is for.
            request.clear_site_data = false;
            self.browsing.render(self, &request).await
        })
    }

    fn receive<'a>(
        &'a self,
        id: &'a str,
        snapshot: Snapshot,
    ) -> Pending<'a, Result<Option<FetchStage>, String>> {
        Box::pin(self.articles.receive(id, snapshot, self))
    }

    fn session_kept(&self, site: &str, reason: Option<Reason>) {
        let library = lock(&self.library);
        let kept = match reason {
            Some(Reason::Check) => library.site_check_passed(site, super::now_ms()),
            _ => library.site_signed_in(site, super::now_ms()),
        };
        if let Err(error) = kept {
            tracing::warn!(event = "library.save", %error, "Could not note the site's sign-in");
        }
    }

    fn clear_site<'a>(&'a self, site: &'a str) -> Pending<'a, Result<(), String>> {
        Box::pin(self.browsing.clear_site(self, site))
    }

    fn sign_in_changed(&self, view: SignInView) {
        self.emit(super::CoreEvent::SignInState(view));
    }
}

impl super::Core {
    /// Opens a window on article `id`'s page for the user to sign in or pass a check; the
    /// article is fetched again once the window shows it whole or the user says so.
    pub fn start_sign_in(self: &Arc<Self>, id: &str) -> Result<SignInView, String> {
        if self.app_settings().browser.use_ == super::browsing::BrowserUse::Off {
            return Err(super::browsing::BROWSER_OFF.into());
        }
        let (url, asks) = self.sign_in.begin(&**self, id)?;
        let core = Arc::clone(self);
        tokio::spawn(async move { core.sign_in.run(&*core, url, asks).await });
        self.sign_in
            .view()
            .ok_or_else(|| "No sign-in is open.".into())
    }

    pub fn sign_in(&self) -> Option<SignInView> {
        self.sign_in.view()
    }

    /// "I'm signed in, fetch again".
    pub fn finish_sign_in(&self) -> Result<(), String> {
        self.sign_in.finish()
    }

    pub fn cancel_sign_in(&self) -> Result<(), String> {
        self.sign_in.cancel()
    }

    pub fn show_sign_in_window(&self) -> Result<(), String> {
        self.sign_in.show()
    }

    pub fn stay_signed_in(&self, stay: bool) -> Result<(), String> {
        self.sign_in.stay_signed_in(self, stay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use SignInEvent as E;
    use SignInStage as S;

    #[test]
    fn a_sign_in_moves_only_on_its_events() {
        let failed = || S::Failed {
            error: "gone".into(),
        };
        let table = [
            (S::Opening, E::WindowOpened, S::Waiting),
            (S::Opening, E::BrowserFailed("gone".into()), failed()),
            (S::Opening, E::Cancelled, S::Cancelled),
            (S::Waiting, E::ArticleSeen, S::Fetching),
            (S::Waiting, E::UserSaidDone, S::Fetching),
            (S::Waiting, E::WindowClosed, S::Cancelled),
            (S::Waiting, E::Cancelled, S::Cancelled),
            (S::Waiting, E::Fetched, S::Waiting),
            (S::Fetching, E::Fetched, S::Done),
            (S::Fetching, E::BrowserFailed("gone".into()), failed()),
            (S::Fetching, E::Cancelled, S::Fetching),
            (S::Fetching, E::WindowClosed, S::Fetching),
            (S::Done, E::Cancelled, S::Done),
            (S::Cancelled, E::WindowOpened, S::Cancelled),
        ];
        for (from, event, to) in table {
            assert_eq!(next(&from, event.clone()), to, "{from:?} + {event:?}");
        }
    }
}
