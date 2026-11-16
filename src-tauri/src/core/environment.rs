//! What the core is given by the app around it: folders, where events go, the speech
//! engines, how to play audio and show files, and speech recognition.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aoide_audio::output::OutputCommand;
use aoide_tts::{EngineProfile, NetworkScope, Registry};
use tauri::{AppHandle, Emitter};

use super::recording::{app_transcribers, TranscriberFactory};

pub(super) const OUTPUT_RATE: u32 = 48_000;

/// All built-in engine adapters.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    aoide_chatterbox::engine::register(&mut registry);
    aoide_openai_speech::register(&mut registry);
    registry
}

pub struct Paths {
    /// Where "Find books" looks by default.
    pub home_dir: PathBuf,
    pub library_dir: PathBuf,
    pub covers_dir: PathBuf,
    pub audio_dir: PathBuf,
    pub db_path: PathBuf,
    /// User settings and themes (`~/.config/aoide`).
    pub config_dir: PathBuf,
    /// Speech-recognition models.
    pub models_dir: PathBuf,
    /// Saved articles: a folder each, holding its snapshot and cover.
    pub articles_dir: PathBuf,
    /// The optional browser's profile and a downloaded browser.
    pub browser_dir: PathBuf,
}

/// Where Core's events go: Tauri's app handle in the app, a recorder in tests.
pub trait Events: Send + Sync {
    fn emit(&self, name: &str, payload: serde_json::Value);
}

impl Events for AppHandle {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        let _ = Emitter::emit(self, name, payload);
    }
}

/// What Core needs from the world outside the library folder.
/// Opens the file manager on a file's folder with the file selected.
pub type ShowInFolder = Arc<dyn Fn(&Path) -> Result<(), String> + Send + Sync>;
/// Opens a web address in the system's browser.
pub type OpenLink = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// What time it is, for how long jobs ran: the system's clock in the app, one tests move on.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> Instant + Send + Sync>);

impl Clock {
    pub fn now(&self) -> Instant {
        (self.0)()
    }

    /// A clock that moves on by `step` each time it is read.
    #[cfg(test)]
    pub fn stepping(step: std::time::Duration) -> Self {
        let start = Instant::now();
        let reads = std::sync::atomic::AtomicU32::new(0);
        Self(Arc::new(move || {
            start + step * reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        }))
    }

    /// A clock that stands still until the returned handle moves it on.
    #[cfg(test)]
    pub fn manual() -> (Self, Arc<std::sync::Mutex<Instant>>) {
        let now = Arc::new(std::sync::Mutex::new(Instant::now()));
        let read = Arc::clone(&now);
        (Self(Arc::new(move || *read.lock().unwrap())), now)
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self(Arc::new(Instant::now))
    }
}

pub struct Environment {
    pub events: Arc<dyn Events>,
    pub engines: Registry,
    /// Commands tried in order to play audio.
    pub audio: Vec<OutputCommand>,
    /// Media keys, file and config watchers, and the startup scan. Tests leave them off.
    pub background: bool,
    /// Opens the file manager on a file's folder with the file selected.
    pub show_in_folder: ShowInFolder,
    /// Opens an article's page in the system's browser.
    pub open_link: OpenLink,
    /// Speech recognition for syncing recordings.
    pub transcribers: TranscriberFactory,
    /// Gets articles' pages from the web.
    pub fetcher: Arc<dyn super::articles::Fetcher>,
    /// Drives the optional browser (started only when the user turned it on).
    pub browser: Arc<dyn super::browsing::BrowserDriver>,
    /// The browser "Download Chrome" gets; `None` where none is offered.
    pub browser_download: Option<super::browser_download::Source>,
    /// Shows desktop notifications, and knows whether the window is in front.
    pub notifier: Arc<dyn super::notices::Notifier>,
    /// How long jobs ran.
    pub clock: Clock,
}

impl Environment {
    /// The desktop app: Tauri events, the built-in engines, the system's audio player.
    pub fn desktop(app: AppHandle) -> Self {
        Self {
            events: Arc::new(app),
            engines: registry(),
            audio: OutputCommand::candidates(OUTPUT_RATE),
            background: true,
            show_in_folder: Arc::new(|path| {
                tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
            }),
            open_link: Arc::new(|url| {
                tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
            }),
            transcribers: app_transcribers(),
            fetcher: super::articles::web_fetcher(),
            browser: Arc::new(super::browsing::ChromeDriver::default()),
            browser_download: super::browser_download::Source::pinned(),
            notifier: Arc::new(super::notices::Unsent),
            clock: Clock::default(),
        }
    }

    /// A real-time-paced sink that plays nothing (debug smoke runs, tests).
    #[cfg(any(test, debug_assertions))]
    pub fn silent_audio() -> Vec<OutputCommand> {
        vec![OutputCommand {
            program: "sh".into(),
            args: vec!["-c".into(), "exec cat > /dev/null".into()],
            device_latency_ms: 0,
        }]
    }
}

pub(super) fn default_profile() -> EngineProfile {
    EngineProfile {
        id: "chatterbox-local".into(),
        name: "Chatterbox (local)".into(),
        kind: aoide_chatterbox::engine::KIND.into(),
        base_url: "http://127.0.0.1:8004".into(),
        model: None,
        network: NetworkScope::Loopback,
        voice_id: None,
        params: Default::default(),
        launch: None,
    }
}
