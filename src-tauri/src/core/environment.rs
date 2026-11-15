//! What the core is given by the app around it: folders, where events go, the speech
//! engines, how to play audio and show files, and speech recognition.

use std::path::{Path, PathBuf};
use std::sync::Arc;

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

pub struct Environment {
    pub events: Arc<dyn Events>,
    pub engines: Registry,
    /// Commands tried in order to play audio.
    pub audio: Vec<OutputCommand>,
    /// Media keys, file and config watchers, and the startup scan. Tests leave them off.
    pub background: bool,
    /// Opens the file manager on a file's folder with the file selected.
    pub show_in_folder: ShowInFolder,
    /// Speech recognition for syncing recordings.
    pub transcribers: TranscriberFactory,
    /// Gets articles' pages from the web.
    pub fetcher: Arc<dyn super::articles::Fetcher>,
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
            transcribers: app_transcribers(),
            fetcher: super::articles::web_fetcher(),
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
