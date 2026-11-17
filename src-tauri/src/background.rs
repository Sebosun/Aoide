//! Keeping on working with the window closed: the window's close button, what the core says the
//! window should do about it, and looking again whether work is left after each event that can
//! end a job (so Quit when done quits, and the tray stays current). The rule is the core's
//! (`core/presence.rs`); this is only the window's side.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::core::{Core, CoreEvent, Events, WindowAction};

/// Does what the core says the window should do.
pub fn act(app: &AppHandle, action: WindowAction) {
    let window = app.get_webview_window("main");
    match action {
        WindowAction::None | WindowAction::AskUser => {}
        WindowAction::HideWindow => {
            if let Some(window) = window {
                let _ = window.hide();
            }
        }
        WindowAction::MinimiseWindow => {
            if let Some(window) = window {
                let _ = window.minimize();
            }
        }
        WindowAction::ShowWindow => {
            if let Some(window) = window {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }
        WindowAction::Quit => app.exit(0),
    }
}

/// The close button asks the core; the window coming to the front tells it.
pub fn watch_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let app = app.clone();
    window.on_window_event(move |event| {
        let Some(core) = app.try_state::<Arc<Core>>() else {
            return;
        };
        match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // The core decides; quitting is `app.exit`, so shutdown still runs.
                api.prevent_close();
                act(&app, core.close_requested());
            }
            tauri::WindowEvent::Focused(true) => core.window_raised(),
            _ => {}
        }
    });
}

/// The events after which a job may have ended (or the tray's lines changed).
fn may_end_work(name: &str) -> bool {
    [
        CoreEvent::PLAYER_STATE,
        CoreEvent::GENERATION_STATE,
        CoreEvent::SYNC_STATE,
        CoreEvent::EXPORT_PROGRESS,
        CoreEvent::DISCOVERY_IMPORT,
        CoreEvent::MODEL_DOWNLOAD,
        CoreEvent::BROWSER_DOWNLOAD,
        CoreEvent::LIBRARY_CHANGED,
        CoreEvent::ENGINE_STATUS,
    ]
    .contains(&name)
}

/// The core's events, passed on to the window, with a poke for those that may end a job.
pub struct Watched {
    inner: Arc<dyn Events>,
    poke: Mutex<Sender<()>>,
}

impl Events for Watched {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.inner.emit(name, payload);
        if may_end_work(name) {
            let _ = crate::core::lock(&self.poke).send(());
        }
    }
}

/// Wraps `events`; the returned receiver is handed to [`check_work`] once the core runs.
pub fn watched(events: Arc<dyn Events>) -> (Arc<dyn Events>, Receiver<()>) {
    let (poke, pokes) = channel();
    let events = Watched {
        inner: events,
        poke: Mutex::new(poke),
    };
    (Arc::new(events), pokes)
}

/// After each poke (a burst counts once: `PlayerState` comes for every word), looks whether
/// the work is done and refreshes `refresh` (the tray).
pub fn check_work(app: AppHandle, pokes: Receiver<()>, refresh: impl Fn(&Core) + Send + 'static) {
    std::thread::Builder::new()
        .name("aoide-presence".into())
        .spawn(move || {
            while pokes.recv().is_ok() {
                std::thread::sleep(Duration::from_millis(400));
                while pokes.try_recv().is_ok() {}
                let Some(core) = app.try_state::<Arc<Core>>() else {
                    continue;
                };
                refresh(&core);
                act(&app, core.check_work());
            }
        })
        .map(drop)
        .unwrap_or_else(|error| {
            tracing::warn!(event = "presence.watch", %error, "Cannot watch for work ending");
        });
}

/// Why no tray is shown, or that one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCheck {
    Shown,
    NoLibrary,
    NoWatcher,
}

impl TrayCheck {
    fn outcome(self) -> &'static str {
        match self {
            TrayCheck::Shown => "shown",
            TrayCheck::NoLibrary => "no_library",
            TrayCheck::NoWatcher => "no_watcher",
        }
    }
}

/// The appindicator libraries the tray loads at run time, in the order it tries them.
#[cfg(target_os = "linux")]
const INDICATOR_LIBRARIES: [&str; 2] = ["libayatana-appindicator3.so.1", "libappindicator3.so.1"];

/// Whether a tray can be shown: its library opens (looked for here, because the tray's own
/// loader panics without it) and something on the session bus shows tray icons (GNOME without
/// the AppIndicator extension has nothing).
fn tray_check() -> TrayCheck {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: opening the appindicator library runs only its own initialisers, which the
        // tray would run anyway; it is closed again at once.
        let opens = INDICATOR_LIBRARIES
            .iter()
            .any(|name| unsafe { libloading::Library::new(name) }.is_ok());
        if !opens {
            return TrayCheck::NoLibrary;
        }
        let watched = zbus::blocking::Connection::session()
            .and_then(|bus| zbus::blocking::fdo::DBusProxy::new(&bus))
            .and_then(|bus| {
                let name = zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher")
                    .map_err(zbus::Error::from)?;
                bus.name_has_owner(name).map_err(Into::into)
            });
        if !matches!(watched, Ok(true)) {
            return TrayCheck::NoWatcher;
        }
    }
    TrayCheck::Shown
}

/// The tray's menu items whose text or state changes.
pub struct Tray {
    playing: tauri::menu::MenuItem<tauri::Wry>,
    play: tauri::menu::MenuItem<tauri::Wry>,
    work: tauri::menu::MenuItem<tauri::Wry>,
    quit_when_done: tauri::menu::MenuItem<tauri::Wry>,
    /// The text each item shows, so an unchanged line is not set again.
    shown: Mutex<[String; 3]>,
}

/// Shows the tray when the desktop can (else Aoide minimises instead of hiding, so it can
/// always be brought back). Its menu: what plays with play or pause, the work under way, Open
/// Aoide, Quit when done and Quit. There is no click on the icon itself on Linux.
pub fn start_tray(app: &AppHandle, core: &Core) -> Option<Arc<Tray>> {
    let check = tray_check();
    let tray = match check {
        TrayCheck::Shown => build_tray(app).map_err(|e| e.to_string()),
        _ => Err(String::new()),
    };
    let (outcome, error) = match &tray {
        Ok(_) => (check.outcome(), None),
        Err(e) if e.is_empty() => (check.outcome(), None),
        Err(e) => ("failed", Some(e.clone())),
    };
    tracing::info!(event = "presence.tray", outcome, error, "Looked for a tray");
    core.set_tray(tray.is_ok());
    let tray = tray.ok().map(Arc::new);
    if let Some(tray) = &tray {
        tray.refresh(core);
    }
    tray
}

fn build_tray(app: &AppHandle) -> tauri::Result<Tray> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let item = |id: &str, text: &str, enabled: bool| {
        MenuItem::with_id(app, id, text, enabled, None::<&str>)
    };
    let tray = Tray {
        playing: item("playing", "Nothing playing", false)?,
        play: item("play", "Play", false)?,
        work: item("work", "Nothing in progress", false)?,
        quit_when_done: item("quit_when_done", "Quit when done", false)?,
        shown: Mutex::default(),
    };
    let menu = Menu::with_items(
        app,
        &[
            &tray.playing,
            &tray.play,
            &PredefinedMenuItem::separator(app)?,
            &tray.work,
            &PredefinedMenuItem::separator(app)?,
            &item("open", "Open Aoide", true)?,
            &tray.quit_when_done,
            &item("quit", "Quit", true)?,
        ],
    )?;
    let mut icon = tauri::tray::TrayIconBuilder::with_id("aoide")
        .tooltip("Aoide")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let Some(core) = app.try_state::<Arc<Core>>() else {
                return;
            };
            match event.id.as_ref() {
                "play" => core.toggle(),
                "open" => crate::show_window(app),
                "quit_when_done" => act(app, core.quit_when_done()),
                "quit" => act(app, core.quit_chosen()),
                _ => {}
            }
        });
    if let Some(image) = app.default_window_icon() {
        icon = icon.icon(image.clone());
    }
    icon.build(app)?;
    Ok(tray)
}

impl Tray {
    /// Sets the menu's lines from the core's views, changing only those that differ.
    pub fn refresh(&self, core: &Core) {
        let working = core.working();
        let player = core.player_state();
        let playing = matches!(
            player.status,
            crate::core::PlaybackStatus::Playing | crate::core::PlaybackStatus::Buffering
        );
        let title = player
            .book_id
            .as_deref()
            .and_then(|id| core.shown_record(id))
            .and_then(|r| r.title);
        let lines = [
            title.clone().unwrap_or_else(|| "Nothing playing".into()),
            if playing { "Pause" } else { "Play" }.to_owned(),
            working
                .summary()
                .unwrap_or_else(|| "Nothing in progress".into()),
        ];
        let mut shown = crate::core::lock(&self.shown);
        for ((item, line), was) in [&self.playing, &self.play, &self.work]
            .into_iter()
            .zip(&lines)
            .zip(shown.iter_mut())
        {
            if line != was {
                let _ = item.set_text(line);
                *was = line.clone();
            }
        }
        let _ = self.play.set_enabled(title.is_some());
        let _ = self.quit_when_done.set_enabled(working.any());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Seen(Mutex<Vec<String>>);

    impl Events for Seen {
        fn emit(&self, name: &str, _: serde_json::Value) {
            crate::core::lock(&self.0).push(name.to_owned());
        }
    }

    /// Looks for a tray on this desktop (opt in: it opens the appindicator library and asks
    /// the session bus). `AOIDE_TRAY_CHECK=1 cargo test -p aoide tray_check -- --nocapture`
    #[test]
    fn tray_check_on_this_desktop() {
        if std::env::var_os("AOIDE_TRAY_CHECK").is_none() {
            return;
        }
        println!("tray: {:?}", tray_check());
    }

    #[test]
    fn every_event_reaches_the_window_and_those_that_may_end_a_job_poke() {
        let seen = Arc::new(Seen::default());
        let (events, pokes) = watched(seen.clone());
        events.emit(CoreEvent::FULLSCREEN, serde_json::Value::Null);
        assert!(pokes.try_recv().is_err());
        events.emit(CoreEvent::GENERATION_STATE, serde_json::Value::Null);
        events.emit(CoreEvent::PLAYER_STATE, serde_json::Value::Null);
        assert_eq!(pokes.try_iter().count(), 2);
        assert_eq!(
            *crate::core::lock(&seen.0),
            [
                CoreEvent::FULLSCREEN,
                CoreEvent::GENERATION_STATE,
                CoreEvent::PLAYER_STATE
            ]
        );
    }
}
