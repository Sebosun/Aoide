//! Articles fetched with the optional browser, through the core, with a scripted fetcher and
//! a scripted browser.

use std::sync::Arc;

use aoide_fetch::FetchError;
use aoide_tts::testing::FakeEngine;

use super::article_tests::{cut_short, Pages, LIGHTS, LIGHTS_URL};
use super::articles::ArticleSource;
use super::browsing::testing::{fake_program, ScriptedBrowser};
use super::browsing::{BrowserSettings, BrowserUse, BROWSER_OFF};
use super::fetching::{Reason, Stage};
use super::tests::{library_folder, start_browsing, until};
use super::{AppSettingsPatch, Core};

pub(super) struct Setup {
    pub(super) dir: tempfile::TempDir,
    pub(super) core: Arc<Core>,
    pub(super) pages: Arc<Pages>,
    pub(super) browser: Arc<ScriptedBrowser>,
}

/// A core with the browser used as `use_`, and `rules` in site-rules.toml.
pub(super) fn setup(use_: BrowserUse, rules: &str) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let config = dir.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join(aoide_config::SITE_RULES_FILE), rules).unwrap();
    let program = fake_program(dir.path());
    let (pages, browser) = (Pages::with(&[]), Arc::new(ScriptedBrowser::default()));
    let (core, _) = start_browsing(
        dir.path(),
        &FakeEngine::new(),
        pages.clone(),
        browser.clone(),
    );
    core.update_app_settings(AppSettingsPatch {
        browser: Some(BrowserSettings {
            use_,
            path: Some(program.to_string_lossy().into_owned()),
            idle_seconds: 60,
        }),
        ..Default::default()
    })
    .unwrap();
    Setup {
        dir,
        core,
        pages,
        browser,
    }
}

fn stage(core: &Core, id: &str) -> Option<Stage> {
    core.article(id).ok().map(|view| view.stage)
}

async fn settled(core: &Core, id: &str) -> Stage {
    until("the fetch settles", || {
        !matches!(
            stage(core, id),
            Some(Stage::Queued | Stage::Fetching) | None
        )
    })
    .await;
    stage(core, id).unwrap()
}

#[tokio::test]
async fn with_the_browser_off_it_is_never_started_even_for_a_site_that_asks() {
    let s = setup(BrowserUse::Off, "[\"quay.example\"]\nfetch = \"browser\"\n");
    s.pages.set(LIGHTS_URL, Ok(cut_short()));
    s.browser.set(LIGHTS_URL, Ok(LIGHTS));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(
        settled(&s.core, &id).await,
        Stage::NeedsYou(Reason::CutShort)
    );
    let view = s.core.article(&id).unwrap();
    assert_eq!(view.via, Some(ArticleSource::Http));
    assert!(view
        .error
        .unwrap()
        .contains("asks for the browser, which is off"));
    assert_eq!(
        s.core.refetch_article(&id, ArticleSource::Browser),
        Err(BROWSER_OFF.into())
    );
    assert_eq!(s.browser.renders(), 0);
    assert!(s.browser.started.lock().unwrap().is_empty());
}

#[tokio::test]
async fn as_a_fallback_the_browser_renders_what_plain_fetching_cut_short() {
    let s = setup(BrowserUse::Fallback, "");
    s.pages.set(LIGHTS_URL, Ok(cut_short()));
    s.browser.set(
        LIGHTS_URL,
        Ok(&LIGHTS.replace("lights the lamps", "lights the oil lamps")),
    );
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(settled(&s.core, &id).await, Stage::Ready);
    let view = s.core.article(&id).unwrap();
    assert_eq!(view.via, Some(ArticleSource::Browser));
    assert_eq!(view.error, None);
    assert_eq!((s.pages.asks(), s.browser.renders()), (1, 1));
    let kept = std::fs::read_to_string(
        s.dir
            .path()
            .join(format!("data/articles/{id}/snapshot.html")),
    );
    assert!(kept.unwrap().contains("lights the oil lamps"));
    // The program set in Settings was the one started.
    assert!(s.browser.started.lock().unwrap()[0].ends_with("fake-chrome"));

    // A page plain fetching gets whole is not rendered.
    s.pages.set("https://quay.example/whole", Ok(LIGHTS.into()));
    let whole = s.core.save_article("https://quay.example/whole").unwrap();
    assert_eq!(settled(&s.core, &whole).await, Stage::Ready);
    assert_eq!(
        s.core.article(&whole).unwrap().via,
        Some(ArticleSource::Http)
    );
    assert_eq!(s.browser.renders(), 1);
}

#[tokio::test]
async fn first_renders_every_article_with_the_sites_rule_and_skips_plain_fetching() {
    let rules = "[\"quay.example\"]\nblock = [\"*/ads/*\"]\nwait_for = \"article\"\n\
                 site_data = \"clear_before_fetch\"\n";
    let s = setup(BrowserUse::First, rules);
    s.browser.set(LIGHTS_URL, Ok(LIGHTS));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(settled(&s.core, &id).await, Stage::Ready);
    assert_eq!(s.pages.asks(), 0);
    let request = s.browser.rendered.lock().unwrap()[0].clone();
    assert_eq!(request.block, vec!["*/ads/*".to_owned()]);
    assert_eq!(request.wait_for.as_deref(), Some("article"));
    assert!(request.clear_site_data);
}

#[tokio::test]
async fn when_the_browser_fails_the_plain_copy_is_kept_and_the_failure_said() {
    let s = setup(BrowserUse::Fallback, "");
    s.pages.set(LIGHTS_URL, Ok(cut_short()));
    s.browser
        .set(LIGHTS_URL, Err("The page did not load: timed out"));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(
        settled(&s.core, &id).await,
        Stage::NeedsYou(Reason::CutShort)
    );
    let view = s.core.article(&id).unwrap();
    assert_eq!(view.via, Some(ArticleSource::Http));
    assert_eq!(
        view.error.as_deref(),
        Some("The browser could not get it: The page did not load: timed out")
    );
    assert!(s.core.chapter(&id, 0).is_ok());

    // Fetched again once plain fetching fell short: straight to the browser, and its whole
    // copy replaces the partial one.
    s.browser.set(LIGHTS_URL, Ok(LIGHTS));
    s.core.refetch_article(&id, ArticleSource::Http).unwrap();
    until("the browser is asked again", || s.browser.renders() == 2).await;
    assert_eq!(settled(&s.core, &id).await, Stage::Ready);
    assert_eq!(s.pages.asks(), 1);
    let view = s.core.article(&id).unwrap();
    assert_eq!((view.via, view.error), (Some(ArticleSource::Browser), None));
}

#[tokio::test]
async fn a_failed_plain_fetch_falls_back_to_the_browser_and_asked_for_it_goes_straight_there() {
    let s = setup(BrowserUse::Fallback, "");
    s.pages.set(LIGHTS_URL, Err(FetchError::Status(403)));
    s.browser.set(LIGHTS_URL, Ok(LIGHTS));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(settled(&s.core, &id).await, Stage::Ready);
    // The plain failure does not matter once the browser got it all.
    assert_eq!(s.core.article(&id).unwrap().error, None);

    s.core.refetch_article(&id, ArticleSource::Browser).unwrap();
    until("the browser is asked again", || s.browser.renders() == 2).await;
    assert_eq!(settled(&s.core, &id).await, Stage::Ready);
    assert_eq!(s.pages.asks(), 1);
}

#[tokio::test]
async fn the_status_names_the_browser_and_runs_it_for_its_version_only_once_turned_on() {
    use super::browsing::BrowserBinaryView;
    let s = setup(BrowserUse::Off, "");
    let status = s.core.browser_status();
    let BrowserBinaryView::Found {
        path,
        version,
        downloaded,
    } = status.binary
    else {
        panic!("the configured program is found: {status:?}")
    };
    assert!(path.ends_with("fake-chrome"));
    assert_eq!((version, downloaded), (None, false));
    assert_eq!(status.download, None);
    assert!(!status.downloaded);
    assert_eq!(s.core.download_browser().await, Err(BROWSER_OFF.into()));

    let mut settings = s.core.app_settings().browser;
    settings.use_ = BrowserUse::Fallback;
    s.core
        .update_app_settings(AppSettingsPatch {
            browser: Some(settings),
            ..Default::default()
        })
        .unwrap();
    let BrowserBinaryView::Found { version, .. } = s.core.browser_status().binary else {
        panic!("still found")
    };
    assert_eq!(version.as_deref(), Some("Chromium 1.0"));
    // No download is offered in tests.
    assert!(s.core.download_browser().await.is_err());
    assert_eq!(s.core.browser_status().offered_version, None);
}

#[tokio::test]
async fn a_site_that_needs_the_user_is_listed_with_its_rule_and_can_be_signed_out_or_forgotten() {
    use super::sites::SiteSession;
    use aoide_config::{SiteData, SiteFetch};
    let s = setup(
        BrowserUse::Fallback,
        "[\"lantern.example\"]\nfetch = \"http\"\n",
    );
    s.pages.set(LIGHTS_URL, Ok(cut_short()));
    s.browser.set(LIGHTS_URL, Err("timed out"));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(
        settled(&s.core, &id).await,
        Stage::NeedsYou(Reason::CutShort)
    );
    let sites = s.core.sites().unwrap();
    let hosts: Vec<_> = sites.iter().map(|v| v.host.as_str()).collect();
    assert_eq!(hosts, ["lantern.example", "quay.example"]);
    assert_eq!(sites[0].fetch, Some(SiteFetch::Http));
    assert!(matches!(
        sites[1].session,
        SiteSession::AskedFor {
            reason: Reason::CutShort,
            ..
        }
    ));
    assert_eq!(sites[1].waiting_article.as_deref(), Some(id.as_str()));

    // A rule set in Settings is written to the file, keeping what was written by hand.
    s.core
        .set_site_rule(
            "www.quay.example",
            Some(SiteFetch::Browser),
            SiteData::ClearBeforeFetch,
        )
        .unwrap();
    let file = std::fs::read_to_string(s.dir.path().join("config/site-rules.toml")).unwrap();
    assert!(
        file.contains("[\"quay.example\"]") && file.contains("clear_before_fetch"),
        "{file}"
    );
    assert!(file.contains("[\"lantern.example\"]"));
    let quay = s.core.sites().unwrap().pop().unwrap();
    assert_eq!(
        (quay.fetch, quay.site_data),
        (Some(SiteFetch::Browser), SiteData::ClearBeforeFetch)
    );

    s.core.sign_out_site("quay.example").await.unwrap();
    s.core.forget_site("lantern.example").await.unwrap();
    assert_eq!(
        *s.browser.cleared.lock().unwrap(),
        ["quay.example", "lantern.example"]
    );
    let hosts: Vec<_> = s
        .core
        .sites()
        .unwrap()
        .into_iter()
        .map(|v| v.host)
        .collect();
    assert_eq!(hosts, ["quay.example"]);
    let file = std::fs::read_to_string(s.dir.path().join("config/site-rules.toml")).unwrap();
    assert!(!file.contains("lantern.example"), "{file}");
}

#[tokio::test]
async fn with_the_browser_off_sites_are_not_cleared_and_clearing_everything_removes_the_profile() {
    let s = setup(BrowserUse::Off, "");
    assert_eq!(
        s.core.sign_out_site("quay.example").await,
        Err(BROWSER_OFF.into())
    );
    assert!(s.browser.cleared.lock().unwrap().is_empty());

    let profile = s.dir.path().join("data/browser/profile");
    std::fs::create_dir_all(profile.join("Default")).unwrap();
    std::fs::write(profile.join("Default/Cookies"), "kept").unwrap();
    s.core.clear_browser_data().await.unwrap();
    assert!(!profile.exists());
}

pub(super) const SIGN_IN_PAGE: &str =
    "<html><head><title>Sign in</title></head><body><p>Sign in to keep reading.</p></body></html>";

/// An article left cut short, with the browser on as a fallback that could not get it.
pub(super) async fn needing_sign_in() -> (Setup, String) {
    let s = setup(BrowserUse::Fallback, "");
    s.pages.set(LIGHTS_URL, Ok(cut_short()));
    s.browser.set(LIGHTS_URL, Err("timed out"));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    assert_eq!(
        settled(&s.core, &id).await,
        Stage::NeedsYou(Reason::CutShort)
    );
    (s, id)
}

pub(super) async fn sign_in_ends(core: &Core) -> super::signin::SignInView {
    use super::signin::SignInStage as S;
    until("the sign-in ends", || {
        core.sign_in()
            .is_some_and(|v| matches!(v.stage, S::Done | S::Cancelled | S::Failed { .. }))
    })
    .await;
    core.sign_in().unwrap()
}

#[tokio::test]
async fn the_article_is_fetched_once_the_window_shows_it_whole_and_the_sign_in_is_kept() {
    use super::signin::SignInStage as S;
    use super::sites::SiteSession;
    let (s, id) = needing_sign_in().await;
    let window = &s.browser.window;
    *window.page.lock().unwrap() = SIGN_IN_PAGE.into();
    let view = s.core.start_sign_in(&id).unwrap();
    assert_eq!(
        (view.site.as_str(), view.reason),
        ("quay.example", Some(Reason::CutShort))
    );
    until("the window is open", || {
        s.core.sign_in().unwrap().stage == S::Waiting
    })
    .await;
    assert!(s.core.start_sign_in(&id).is_err(), "one at a time");
    s.core.show_sign_in_window().unwrap();
    until("the window is shown", || {
        window.shown.load(std::sync::atomic::Ordering::SeqCst) == 1
    })
    .await;

    // The sign-in page on another site (the login host) is not the article.
    window.shows("https://login.example/", LIGHTS);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(s.core.sign_in().unwrap().stage, S::Waiting);
    window.shows(LIGHTS_URL, LIGHTS);
    let done = sign_in_ends(&s.core).await;
    assert_eq!((done.stage, done.result), (S::Done, Some(Stage::Ready)));
    assert!(window.closed.load(std::sync::atomic::Ordering::SeqCst));
    let view = s.core.article(&id).unwrap();
    assert_eq!(
        (view.stage, view.via),
        (Stage::Ready, Some(ArticleSource::Browser))
    );
    assert_eq!(s.browser.renders(), 1, "the window's own page was used");
    let site = s.core.sites().unwrap().pop().unwrap();
    assert!(
        matches!(site.session, SiteSession::SignedIn { .. }),
        "{site:?}"
    );
    assert!(
        s.browser.cleared.lock().unwrap().is_empty(),
        "stays signed in"
    );
}

#[tokio::test]
async fn saying_signed_in_renders_the_page_and_not_staying_clears_the_site_after() {
    use super::signin::SignInStage as S;
    let (s, id) = needing_sign_in().await;
    *s.browser.window.page.lock().unwrap() = SIGN_IN_PAGE.into();
    s.core.start_sign_in(&id).unwrap();
    until("the window is open", || {
        s.core.sign_in().unwrap().stage == S::Waiting
    })
    .await;
    s.core.stay_signed_in(false).unwrap();
    s.browser.set(LIGHTS_URL, Ok(LIGHTS));
    s.core.finish_sign_in().unwrap();
    let done = sign_in_ends(&s.core).await;
    assert_eq!((done.stage, done.result), (S::Done, Some(Stage::Ready)));
    assert_eq!(s.browser.renders(), 2, "one render after the window");
    let request = s.browser.rendered.lock().unwrap()[1].clone();
    assert!(!request.clear_site_data);
    assert_eq!(*s.browser.cleared.lock().unwrap(), ["quay.example"]);
    assert_eq!(s.core.article(&id).unwrap().stage, Stage::Ready);
}

#[tokio::test]
async fn closing_the_window_cancels_and_leaves_the_article_as_it_was() {
    use super::signin::SignInStage as S;
    let (s, id) = needing_sign_in().await;
    let before = s.core.article(&id).unwrap();
    s.core.start_sign_in(&id).unwrap();
    until("the window is open", || {
        s.core.sign_in().unwrap().stage == S::Waiting
    })
    .await;
    s.browser
        .window
        .closed_by_user
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(sign_in_ends(&s.core).await.stage, S::Cancelled);
    assert_eq!(s.core.article(&id).unwrap(), before);
    assert_eq!(s.browser.renders(), 1);
    assert!(s.core.finish_sign_in().is_err(), "nothing open");

    // Cancelled from the dialog: the same, and the window is closed by Aoide.
    s.browser
        .window
        .closed_by_user
        .store(false, std::sync::atomic::Ordering::SeqCst);
    s.core.start_sign_in(&id).unwrap();
    until("the window is open", || {
        s.core.sign_in().unwrap().stage == S::Waiting
    })
    .await;
    s.core.cancel_sign_in().unwrap();
    assert_eq!(sign_in_ends(&s.core).await.stage, S::Cancelled);
    assert!(s
        .browser
        .window
        .closed
        .load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(s.core.article(&id).unwrap(), before);
}

#[tokio::test]
async fn signing_in_is_refused_while_the_browser_is_off_and_fails_when_the_window_cannot_open() {
    let s = setup(BrowserUse::Off, "");
    s.pages.set(LIGHTS_URL, Ok(cut_short()));
    let id = s.core.save_article(LIGHTS_URL).unwrap();
    settled(&s.core, &id).await;
    assert_eq!(s.core.start_sign_in(&id).err(), Some(BROWSER_OFF.into()));
    assert_eq!(s.core.sign_in(), None);

    let (s, id) = needing_sign_in().await;
    *s.browser.window.fails.lock().unwrap() = Some("Chrome did not start".into());
    s.core.start_sign_in(&id).unwrap();
    let ended = sign_in_ends(&s.core).await;
    assert_eq!(
        ended.stage,
        super::signin::SignInStage::Failed {
            error: "Chrome did not start".into()
        }
    );
}
