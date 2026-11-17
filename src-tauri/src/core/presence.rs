//! Whether Aoide is in its window or working on in the background, and what closing the window
//! does. One explicit [`Presence`], changed only by [`next`] on an [`Event`], which also says
//! what the window should do ([`Action`]).
//!
//! Without a tray a hidden window could not be brought back, so then "keep" minimises instead.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::{lock, CoreEvent};

/// What Aoide is doing that closing the window would cut short. Background feed checks are not
/// work: Aoide does not stay just to poll feeds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Working {
    /// The title of what is playing.
    pub playing: Option<String>,
    /// The book being prepared, its segments made and in all; not while the engine has failed
    /// (a stalled preparation would keep Aoide waiting for ever).
    pub preparing: Option<(String, usize, usize)>,
    /// Recordings waiting to sync or syncing.
    pub syncing: usize,
    /// The title of the book being exported.
    pub exporting: Option<String>,
    pub importing: bool,
    /// A speech model or the browser.
    pub downloading: bool,
}

impl Working {
    pub fn any(&self) -> bool {
        self.playing.is_some()
            || self.preparing.is_some()
            || self.syncing > 0
            || self.exporting.is_some()
            || self.importing
            || self.downloading
    }

    /// One line on the work other than playing, for the tray.
    pub fn summary(&self) -> Option<String> {
        if let Some((title, done, total)) = &self.preparing {
            return Some(format!("Preparing {title}: {done} of {total}"));
        }
        if let Some(title) = &self.exporting {
            return Some(format!("Exporting {title}"));
        }
        match self.syncing {
            0 => {}
            1 => return Some("Syncing an audiobook".into()),
            n => return Some(format!("Syncing {n} audiobooks")),
        }
        if self.importing {
            return Some("Adding books".into());
        }
        self.downloading.then(|| "Downloading".into())
    }
}

/// Where Aoide is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Presence {
    /// In its window (also while asking whether to keep working).
    Window,
    /// The window is hidden (or minimised, with no tray) and the core works on.
    Background,
    /// In the background, quitting once the work is done.
    QuittingWhenDone,
}

/// What can happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The window's close button; `working`: a job is under way ([`super::Working`]).
    CloseRequested {
        working: bool,
    },
    /// The answer to "Keep working in the background?".
    CloseAnswered {
        keep: bool,
    },
    /// The window is shown again: from the tray, a second launch, the media controls, or
    /// raised from the taskbar.
    Reopened,
    QuitChosen,
    QuitWhenDoneChosen {
        working: bool,
    },
    /// No job is under way any more.
    WorkEnded,
}

/// What closing the window while working does (Settings › Library).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum CloseBehaviour {
    /// Always quit.
    Quit,
    /// Keep working in the background while a job is under way; quit when idle.
    KeepWhileWorking,
    /// Always stay in the background.
    KeepAlways,
    /// Ask each time the window is closed while working. Also used for a choice this version
    /// does not know.
    #[default]
    #[serde(other)]
    Ask,
}

/// What the window should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    HideWindow,
    /// "Keep" with no tray: a hidden window could not be brought back.
    MinimiseWindow,
    ShowWindow,
    /// Ask whether to keep working ([`super::CoreEvent::CloseAsked`]).
    AskUser,
    Quit,
}

/// The whole rule. `tray`: a tray icon is shown.
pub fn next(
    presence: Presence,
    event: Event,
    behaviour: CloseBehaviour,
    tray: bool,
) -> (Presence, Action) {
    use Presence::*;
    let keep = if tray {
        Action::HideWindow
    } else {
        Action::MinimiseWindow
    };
    match (presence, event) {
        (_, Event::QuitChosen) => (presence, Action::Quit),
        (Window, Event::CloseRequested { working }) => match behaviour {
            CloseBehaviour::Quit => (Window, Action::Quit),
            CloseBehaviour::KeepAlways => (Background, keep),
            CloseBehaviour::KeepWhileWorking if working => (Background, keep),
            CloseBehaviour::Ask if working => (Window, Action::AskUser),
            CloseBehaviour::KeepWhileWorking | CloseBehaviour::Ask => (Window, Action::Quit),
        },
        // Only a minimised window (no tray) can be closed from the background: it quits, so
        // the window can always be closed.
        (Background | QuittingWhenDone, Event::CloseRequested { .. }) => (presence, Action::Quit),
        (Window, Event::CloseAnswered { keep: true }) => (Background, keep),
        (Window, Event::CloseAnswered { keep: false }) => (Window, Action::Quit),
        (Background | QuittingWhenDone, Event::Reopened) => (Window, Action::ShowWindow),
        (Window, Event::Reopened) => (Window, Action::ShowWindow),
        (_, Event::QuitWhenDoneChosen { working: false }) => (presence, Action::Quit),
        (Window, Event::QuitWhenDoneChosen { working: true }) => (QuittingWhenDone, keep),
        (Background | QuittingWhenDone, Event::QuitWhenDoneChosen { working: true }) => {
            (QuittingWhenDone, Action::None)
        }
        (QuittingWhenDone, Event::WorkEnded) => (QuittingWhenDone, Action::Quit),
        (presence, _) => (presence, Action::None),
    }
}

/// What [`Presences`] needs from the rest of the app.
pub(super) trait PresenceHost {
    fn working(&self) -> Working;
    fn behaviour(&self) -> CloseBehaviour;
    fn emit(&self, event: CoreEvent);
}

/// Brings the window to the front (set by the desktop shell).
pub type Raise = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Aoide's [`Presence`], whether a tray icon is shown, and how to raise the window.
pub(super) struct Presences {
    at: Mutex<Presence>,
    tray: AtomicBool,
    raise: Mutex<Option<Raise>>,
}

impl Default for Presences {
    fn default() -> Self {
        Self {
            at: Mutex::new(Presence::Window),
            tray: AtomicBool::new(false),
            raise: Mutex::new(None),
        }
    }
}

impl Presences {
    pub fn presence(&self) -> Presence {
        *lock(&self.at)
    }

    pub fn set_tray(&self, shown: bool) {
        self.tray.store(shown, Ordering::SeqCst);
    }

    /// Moves on `event` (`why`, for the log) and says what the window should do.
    pub fn step(&self, host: &impl PresenceHost, event: Event, why: &'static str) -> Action {
        let tray = self.tray.load(Ordering::SeqCst);
        let action = {
            let mut at = lock(&self.at);
            let (to, action) = next(*at, event, host.behaviour(), tray);
            if to != *at {
                tracing::info!(
                    event = "presence.changed",
                    from = ?*at,
                    to = ?to,
                    why,
                    "Aoide moved between its window and the background"
                );
                *at = to;
            }
            action
        };
        if action == Action::AskUser {
            host.emit(CoreEvent::CloseAsked);
        }
        action
    }

    /// The window's close button.
    pub fn close_requested(&self, host: &impl PresenceHost) -> Action {
        let working = host.working().any();
        self.step(host, Event::CloseRequested { working }, "close")
    }

    pub fn quit_when_done(&self, host: &impl PresenceHost) -> Action {
        let working = host.working().any();
        self.step(
            host,
            Event::QuitWhenDoneChosen { working },
            "quit_when_done",
        )
    }

    /// Something that may have ended a job happened: quitting when done quits once none is left.
    pub fn check_work(&self, host: &impl PresenceHost) -> Action {
        if self.presence() != Presence::QuittingWhenDone || host.working().any() {
            return Action::None;
        }
        self.step(host, Event::WorkEnded, "work_ended")
    }
}

impl PresenceHost for super::Core {
    fn working(&self) -> Working {
        super::Core::working(self)
    }

    fn behaviour(&self) -> CloseBehaviour {
        self.app_settings().close
    }

    fn emit(&self, event: CoreEvent) {
        super::Core::emit(self, event);
    }
}

impl super::Core {
    /// Where Aoide is: its window or the background.
    pub fn presence(&self) -> Presence {
        self.presence.presence()
    }

    /// Whether a tray icon is shown (without one, "keep working" minimises the window).
    pub fn set_tray(&self, shown: bool) {
        self.presence.set_tray(shown);
    }

    /// How the window is brought to the front, for those outside it (the media controls).
    pub fn on_raise(&self, raise: Raise) {
        *lock(&self.presence.raise) = Some(raise);
    }

    /// Brings the window to the front, if the shell said how; false when it cannot.
    pub fn raise_window(&self) -> bool {
        let raise = lock(&self.presence.raise).clone();
        raise.map(|raise| raise()).is_some()
    }

    /// The window's close button was pressed: what the window does.
    pub fn close_requested(&self) -> Action {
        self.presence.close_requested(self)
    }

    /// The answer to "Keep working in the background?"; `remember` keeps it for next time.
    pub fn choose_close(&self, keep: bool, remember: bool) -> Result<Action, String> {
        if remember {
            let close = if keep {
                CloseBehaviour::KeepWhileWorking
            } else {
                CloseBehaviour::Quit
            };
            self.update_app_settings(super::AppSettingsPatch {
                close: Some(close),
                ..Default::default()
            })?;
        }
        Ok(self
            .presence
            .step(self, Event::CloseAnswered { keep }, "answered"))
    }

    /// The window is to be shown again (the tray, a second launch, the media controls).
    pub fn reopened(&self) -> Action {
        self.presence.step(self, Event::Reopened, "reopened")
    }

    /// The window came to the front by other means (raised from the taskbar).
    pub fn window_raised(&self) {
        if self.presence() != Presence::Window {
            self.presence.step(self, Event::Reopened, "raised");
        }
    }

    pub fn quit_chosen(&self) -> Action {
        self.presence.step(self, Event::QuitChosen, "quit")
    }

    /// Quit once every job is done (the tray).
    pub fn quit_when_done(&self) -> Action {
        self.presence.quit_when_done(self)
    }

    /// Something that may have ended a job happened.
    pub fn check_work(&self) -> Action {
        self.presence.check_work(self)
    }
}

impl super::Core {
    /// What Aoide is doing that closing the window would cut short.
    pub fn working(&self) -> Working {
        let title = |id: &str| {
            self.shown_record(id)
                .and_then(|r| r.title)
                .unwrap_or_else(|| "a book".into())
        };
        let player = self.player_state();
        let playing = matches!(
            player.status,
            super::PlaybackStatus::Playing | super::PlaybackStatus::Buffering
        )
        .then(|| player.book_id.as_deref().map(title))
        .flatten();
        let engine_failed = self.engine.status().state == super::EngineState::Error;
        let preparing = self
            .generation
            .preparing_now()
            .filter(|_| !engine_failed)
            .map(|(id, done, total)| (title(&id), done, total));
        let (syncing, models) = self.recordings.busy();
        Working {
            playing,
            preparing,
            syncing,
            exporting: self.exports.exporting().map(|id| title(&id)),
            importing: self.discovery.importing(),
            downloading: models || self.browsing.downloading(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Action::*;
    use CloseBehaviour::{Ask, KeepAlways, KeepWhileWorking};
    use Event::*;
    use Presence::*;

    #[test]
    fn any_job_is_work_and_the_tray_names_it() {
        let idle = Working::default();
        assert!(!idle.any());
        assert_eq!(idle.summary(), Option::None);
        let jobs = [
            Working {
                playing: Some("Tides".into()),
                ..Working::default()
            },
            Working {
                preparing: Some(("Tides".into(), 3, 10)),
                ..Working::default()
            },
            Working {
                syncing: 2,
                ..Working::default()
            },
            Working {
                exporting: Some("Tides".into()),
                ..Working::default()
            },
            Working {
                importing: true,
                ..Working::default()
            },
            Working {
                downloading: true,
                ..Working::default()
            },
        ];
        assert!(jobs.iter().all(Working::any));
        // Playing shows in the tray's own line, not the work line.
        assert_eq!(jobs[0].summary(), Option::None);
        assert_eq!(
            jobs[1].summary().as_deref(),
            Some("Preparing Tides: 3 of 10")
        );
        assert_eq!(jobs[2].summary().as_deref(), Some("Syncing 2 audiobooks"));
    }

    #[tokio::test]
    async fn an_idle_core_is_not_working() {
        use crate::core::article_tests::Pages;
        let dir = tempfile::tempdir().unwrap();
        crate::core::tests::library_folder(dir.path(), "");
        let engine = aoide_tts::testing::FakeEngine::new();
        let (core, _) = crate::core::tests::start_fetching(dir.path(), &engine, Pages::with(&[]));
        core.rescan().unwrap();
        assert_eq!(core.working(), Working::default());
        // Asked, kept, remembered: the next close keeps working without asking.
        assert_eq!(core.choose_close(true, true), Ok(MinimiseWindow));
        assert_eq!(core.app_settings().close, KeepWhileWorking);
        assert_eq!(core.presence(), Background);
        assert_eq!(core.reopened(), ShowWindow);
        assert_eq!(core.choose_close(false, false), Ok(Quit));
        assert_eq!(core.app_settings().close, KeepWhileWorking);
    }

    /// A window, and a core whose work is set by hand.
    #[derive(Default)]
    struct Host {
        working: Mutex<Working>,
        behaviour: Mutex<CloseBehaviour>,
        asked: Mutex<usize>,
    }

    impl PresenceHost for Host {
        fn working(&self) -> Working {
            lock(&self.working).clone()
        }
        fn behaviour(&self) -> CloseBehaviour {
            *lock(&self.behaviour)
        }
        fn emit(&self, event: CoreEvent) {
            if matches!(event, CoreEvent::CloseAsked) {
                *lock(&self.asked) += 1;
            }
        }
    }

    fn preparing() -> Working {
        Working {
            preparing: Some(("Tides".into(), 3, 10)),
            ..Working::default()
        }
    }

    #[test]
    fn closing_while_preparing_works_on_hidden_and_closing_while_idle_quits() {
        let (host, presence) = (Host::default(), Presences::default());
        presence.set_tray(true);
        // Idle: closing quits, nothing asked.
        assert_eq!(presence.close_requested(&host), Quit);
        assert_eq!(*lock(&host.asked), 0);
        // Preparing: the window asks, and keeping hides it.
        *lock(&host.working) = preparing();
        assert_eq!(presence.close_requested(&host), AskUser);
        assert_eq!(*lock(&host.asked), 1);
        assert_eq!(
            presence.step(&host, CloseAnswered { keep: true }, "test"),
            HideWindow
        );
        assert_eq!(presence.presence(), Background);
        // Remembered as keep while working: no question, and with no tray, minimised.
        let (host, presence) = (Host::default(), Presences::default());
        *lock(&host.behaviour) = KeepWhileWorking;
        *lock(&host.working) = preparing();
        assert_eq!(presence.close_requested(&host), MinimiseWindow);
        assert_eq!(*lock(&host.asked), 0);
    }

    #[test]
    fn quit_when_done_quits_after_the_last_job_ends() {
        let (host, presence) = (Host::default(), Presences::default());
        presence.set_tray(true);
        *lock(&host.working) = preparing();
        assert_eq!(presence.quit_when_done(&host), HideWindow);
        assert_eq!(presence.check_work(&host), None);
        *lock(&host.working) = Working {
            syncing: 1,
            ..Working::default()
        };
        assert_eq!(presence.check_work(&host), None);
        *lock(&host.working) = Working::default();
        assert_eq!(presence.check_work(&host), Quit);
        // In the window, work ending quits nothing.
        let presence = Presences::default();
        assert_eq!(presence.check_work(&host), None);
    }

    #[test]
    fn closing_while_working_keeps_working_as_the_setting_says() {
        let close = |working| CloseRequested { working };
        // (behaviour, working) → what closing does, with a tray.
        let cases = [
            (Ask, true, (Window, AskUser)),
            (Ask, false, (Window, Quit)),
            (CloseBehaviour::Quit, true, (Window, Quit)),
            (KeepWhileWorking, true, (Background, HideWindow)),
            (KeepWhileWorking, false, (Window, Quit)),
            (KeepAlways, false, (Background, HideWindow)),
        ];
        for (behaviour, working, then) in cases {
            assert_eq!(
                next(Window, close(working), behaviour, true),
                then,
                "{behaviour:?}, working {working}"
            );
        }
        // The answer: keep, or quit.
        assert_eq!(
            next(Window, CloseAnswered { keep: true }, Ask, true),
            (Background, HideWindow)
        );
        assert_eq!(
            next(Window, CloseAnswered { keep: false }, Ask, true),
            (Window, Quit)
        );
    }

    #[test]
    fn with_no_tray_keeping_minimises_so_the_window_can_be_brought_back() {
        assert_eq!(
            next(
                Window,
                CloseRequested { working: true },
                KeepWhileWorking,
                false
            ),
            (Background, MinimiseWindow)
        );
        assert_eq!(
            next(Window, CloseAnswered { keep: true }, Ask, false),
            (Background, MinimiseWindow)
        );
        // Raised from the taskbar, it is in its window again; closing a minimised window
        // that was never raised quits, so it can always be closed.
        assert_eq!(next(Background, Reopened, Ask, false), (Window, ShowWindow));
        assert_eq!(
            next(
                Background,
                CloseRequested { working: true },
                KeepAlways,
                false
            ),
            (Background, Quit)
        );
    }

    #[test]
    fn quit_when_done_quits_once_the_work_ends() {
        assert_eq!(
            next(Background, QuitWhenDoneChosen { working: true }, Ask, true),
            (QuittingWhenDone, None)
        );
        assert_eq!(
            next(Window, QuitWhenDoneChosen { working: true }, Ask, true),
            (QuittingWhenDone, HideWindow)
        );
        assert_eq!(
            next(Window, QuitWhenDoneChosen { working: false }, Ask, true),
            (Window, Quit)
        );
        assert_eq!(
            next(QuittingWhenDone, WorkEnded, Ask, true),
            (QuittingWhenDone, Quit)
        );
        // Work ending in the background alone does not quit (a pause is not a reason to go).
        assert_eq!(next(Background, WorkEnded, Ask, true), (Background, None));
        // Coming back cancels quitting when done.
        assert_eq!(
            next(QuittingWhenDone, Reopened, Ask, true),
            (Window, ShowWindow)
        );
        for presence in [Window, Background, QuittingWhenDone] {
            assert_eq!(next(presence, QuitChosen, Ask, true).1, Quit);
        }
    }
}
