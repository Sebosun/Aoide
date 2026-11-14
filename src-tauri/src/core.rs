//! Application core: library, playback session, generation and engine glue.
//!
//! Locks are `std::sync::Mutex` and are never held across `.await`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aoide_audio::output::{OutputCommand, PipeOutput};
use aoide_audio::{codec, Entry, Pcm, PlayerCore, Status};
use aoide_generation::{
    buffered_ahead_ms, estimate_ms, plan, Job, Listening, PlanItem, Preparing, SegmentStore,
    WorkItem, Worker, WorkerEvent, WorkerOptions,
};
use aoide_library::{
    content_hash, scan_folder, BookId, BookRecord, BookStatus, CachedSegment, Library, NewBook,
    Progress,
};
use aoide_tts::{
    cache_key, encode_wav, resolve_params, supports_language, BoxFuture, EngineProfile,
    EngineStatus, LaunchCommand, NetworkScope, Params, Registry, SpeechEngine, Synthesis,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Spoken by voice previews: original text, long enough to hear the voice's pace and tone.
const PREVIEW_TEXT: &str =
    "The lantern swung gently as the little boat drifted home across the quiet harbour.";
mod align;
mod config;
pub use config::PreferencesView;
mod discovery;
mod storage;
pub use discovery::{Candidate, CandidateStatus, ImportChoice};
pub use storage::{CleanResult, StorageReport};
mod export;
mod extras;
mod feeder;
mod metadata;
mod preparation;
mod recording;
mod syncing;
pub use recording::{
    app_transcribers, InspectionView, Narration, RecognitionOptions, RecordingView, SyncAction,
    SyncReportView, TranscriberFactory,
};
mod session;
mod voicing;
use session::Session;
use voicing::{fallback_voice, Voicing};
#[cfg(test)]
mod contract;
#[cfg(test)]
mod recording_tests;
#[cfg(test)]
mod tests;
#[cfg(debug_assertions)]
pub use extras::AlignerSettings;
pub use extras::{AppSettings, ListeningStatsView};
use preparation::{Plan, Preparation};
pub use preparation::{PrepareView, Stage as PrepareStage};

use crate::book::{BookModel, ChapterSummary, ChapterView, ExcludedView, SearchHit};

const OUTPUT_RATE: u32 = 48_000;
const LOOKAHEAD_MS: u32 = 90_000;
const TICK: Duration = Duration::from_millis(33);
const IGNORED_KEY: &str = "library.ignored";

fn book_settings_key(id: &str) -> String {
    format!("book.{id}.settings")
}

/// All built-in engine adapters.
pub fn registry() -> Registry {
    let mut registry = Registry::new();
    aoide_chatterbox::engine::register(&mut registry);
    aoide_openai_speech::register(&mut registry);
    registry
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------- wire types

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "BookProgress"))]
pub struct BookProgressView {
    pub chapter: u32,
    pub segment_id: Option<String>,
    pub fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub status: BookStatus,
    pub cover_path: Option<String>,
    pub progress: Option<BookProgressView>,
    pub audio_bytes: u64,
    /// Whole-book preparation status, if queued, running or finished.
    pub prepare: Option<PrepareView>,
    pub added_at: i64,
    pub last_opened_at: Option<i64>,
    /// Ids of the collections the book is on.
    pub collections: Vec<i64>,
    /// The sync stage of the book's recording, if it has one.
    pub recording: Option<syncing::Stage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookView {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub chapters: Vec<ChapterSummary>,
    pub warnings: Vec<String>,
    pub excluded: Vec<ExcludedView>,
    pub language_warning: Option<String>,
    /// The book's voice is not offered by the active engine, so another one reads it.
    pub voice_warning: Option<String>,
    pub progress: Option<BookProgressView>,
    /// Words read aloud in the whole book, for time estimates.
    pub word_count: usize,
}

/// What the player is doing, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum PlaybackStatus {
    Stopped,
    Buffering,
    Playing,
    Paused,
}

/// The speech engine's state, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum EngineState {
    Offline,
    Loading,
    Ready,
    /// Making audio.
    Busy,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "PlayerState"))]
pub struct PlayerStateView {
    pub book_id: Option<String>,
    pub status: PlaybackStatus,
    pub chapter: usize,
    pub segment_id: Option<String>,
    pub word_index: Option<usize>,
    pub speed: f32,
    /// Output gain, 0.0..=1.0.
    pub volume: f32,
    pub chapter_fraction: f64,
    pub error: Option<String>,
    pub sleep: Option<SleepView>,
    /// What is being heard: a speech engine or the book's recording.
    pub source: Narration,
    /// The recording is playing narration that is not in the text (credits, a dedication…).
    pub off_text: bool,
    /// Listening time left in the chapter at this speed, when it is known exactly (a
    /// recording); otherwise the interface estimates it from the words left.
    pub chapter_left_ms: Option<u64>,
    /// The recording is being listened to while it syncs, and the sync has not reached the
    /// playhead yet: playback waits for it.
    pub waiting_for_sync: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EngineStatusView {
    pub profile_name: String,
    pub state: EngineState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "GenerationState"))]
pub struct GenerationStateView {
    pub book_id: Option<String>,
    pub generating_segment_id: Option<String>,
    pub buffered_ahead_ms: u32,
    pub pending: usize,
    pub error: Option<String>,
    /// Progress of the book currently being prepared, if any.
    pub preparing: Option<PreparingView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PreparingView {
    pub book_id: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ImportResult {
    pub path: String,
    pub book_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppInfo {
    pub library_folder: String,
}

// ---------------------------------------------------------------- core

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
}

/// How much of one chapter's audio is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterAudioView {
    pub done: usize,
    pub total: usize,
}

/// A book and its generation plan.
struct Target {
    book: Arc<BookModel>,
    plan: Plan,
    /// Where the audio comes from. A recording's plan has every item made.
    source: Source,
}

/// Where a target's audio comes from.
#[derive(Clone)]
enum Source {
    /// A speech engine, through the generation plan.
    Speech,
    /// A synced recording, decoded clip by clip.
    Recording(Arc<recording::RecordingPlan>),
}

/// Per-book overrides of the global engine profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookSettings {
    pub voice_id: Option<String>,
    pub params: aoide_tts::Params,
    /// BCP-47 language to send instead of the book's declared language.
    pub language: Option<String>,
    /// Speak footnotes after the paragraph that references them.
    pub read_notes: bool,
    /// Which narration plays; `None` until chosen (a speech engine).
    pub narration: Option<recording::Narration>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EngineOptions {
    pub descriptors: Vec<aoide_tts::EngineDescriptor>,
    pub profile: EngineProfile,
}

/// The engine in use and what Core knows about it.
struct ActiveEngine {
    profile: EngineProfile,
    engine: Arc<dyn SpeechEngine>,
    model: Option<String>,
    /// The voice for books that do not choose one.
    voice: Option<String>,
    voices: VoiceList,
    launched: Option<std::process::Child>,
}

/// What the active engine has said about its voices.
#[derive(Debug, Clone, PartialEq)]
enum VoiceList {
    /// Not asked yet, or it could not be reached when asked.
    Unchecked,
    Known(Vec<aoide_tts::Voice>),
    /// It was ready but listed none; saved voice names are used as they are.
    Unlisted,
}

impl VoiceList {
    fn offered(&self) -> Option<&[aoide_tts::Voice]> {
        match self {
            VoiceList::Known(voices) => Some(voices),
            VoiceList::Unchecked | VoiceList::Unlisted => None,
        }
    }
}

/// Everything Core tells the UI: one variant per event, with its payload. The UI's map from
/// event name to payload type is generated from this (core/contract.rs).
#[derive(Debug, Clone)]
pub enum CoreEvent {
    LibraryChanged,
    ConfigChanged(PreferencesView),
    PlayerState(PlayerStateView),
    EngineStatus(EngineStatusView),
    GenerationState(GenerationStateView),
    ScanProgress(discovery::ScanProgress),
    /// Online details arrived for this found book.
    DiscoveryDetails(String),
    DiscoveryImport(discovery::ImportProgress),
    ExportProgress(export::ExportProgress),
    /// A book's recording changed stage or made progress.
    SyncState(recording::RecordingView),
    ModelDownload(recording::ModelDownloadView),
}

impl CoreEvent {
    pub const LIBRARY_CHANGED: &'static str = "library://changed";
    pub const CONFIG_CHANGED: &'static str = "config://changed";
    pub const PLAYER_STATE: &'static str = "player://state";
    pub const ENGINE_STATUS: &'static str = "engine://status";
    pub const GENERATION_STATE: &'static str = "generation://state";
    pub const SCAN_PROGRESS: &'static str = "discovery://progress";
    pub const DISCOVERY_DETAILS: &'static str = "discovery://details";
    pub const DISCOVERY_IMPORT: &'static str = "discovery://import";
    pub const EXPORT_PROGRESS: &'static str = "export://progress";
    pub const SYNC_STATE: &'static str = "sync://state";
    pub const MODEL_DOWNLOAD: &'static str = "model://download";

    pub fn name(&self) -> &'static str {
        match self {
            Self::LibraryChanged => Self::LIBRARY_CHANGED,
            Self::ConfigChanged(_) => Self::CONFIG_CHANGED,
            Self::PlayerState(_) => Self::PLAYER_STATE,
            Self::EngineStatus(_) => Self::ENGINE_STATUS,
            Self::GenerationState(_) => Self::GENERATION_STATE,
            Self::ScanProgress(_) => Self::SCAN_PROGRESS,
            Self::DiscoveryDetails(_) => Self::DISCOVERY_DETAILS,
            Self::DiscoveryImport(_) => Self::DISCOVERY_IMPORT,
            Self::ExportProgress(_) => Self::EXPORT_PROGRESS,
            Self::SyncState(_) => Self::SYNC_STATE,
            Self::ModelDownload(_) => Self::MODEL_DOWNLOAD,
        }
    }

    fn payload(&self) -> serde_json::Result<serde_json::Value> {
        match self {
            Self::LibraryChanged => Ok(serde_json::Value::Null),
            Self::ConfigChanged(p) => serde_json::to_value(p),
            Self::PlayerState(p) => serde_json::to_value(p),
            Self::EngineStatus(p) => serde_json::to_value(p),
            Self::GenerationState(p) => serde_json::to_value(p),
            Self::ScanProgress(p) => serde_json::to_value(p),
            Self::DiscoveryDetails(p) => serde_json::to_value(p),
            Self::DiscoveryImport(p) => serde_json::to_value(p),
            Self::ExportProgress(p) => serde_json::to_value(p),
            Self::SyncState(p) => serde_json::to_value(p),
            Self::ModelDownload(p) => serde_json::to_value(p),
        }
    }
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
        }
    }

    /// A real-time-paced sink that plays nothing (debug smoke runs, tests).
    pub fn silent_audio() -> Vec<OutputCommand> {
        vec![OutputCommand {
            program: "sh".into(),
            args: vec!["-c".into(), "exec cat > /dev/null".into()],
            device_latency_ms: 0,
        }]
    }
}

pub struct Core {
    events: Arc<dyn Events>,
    audio: Vec<OutputCommand>,
    show_in_folder: ShowInFolder,
    pub paths: Paths,
    library: Arc<Mutex<Library>>,
    books: Mutex<HashMap<String, Arc<BookModel>>>,
    player: Arc<Mutex<PlayerCore>>,
    output: Mutex<Option<PipeOutput>>,
    output_latency_ms: Mutex<u32>,
    session: Mutex<Option<Session>>,
    engine: Mutex<ActiveEngine>,
    worker: Mutex<Worker>,
    worker_events: tokio::sync::mpsc::UnboundedSender<WorkerEvent>,
    store: Arc<Store>,
    registry: Registry,
    /// Whole-book preparation. Lock before `library` when both are needed.
    preparation: Mutex<Preparation>,
    generation: Mutex<GenerationStateView>,
    last_player_state: Mutex<Option<PlayerStateView>>,
    last_engine_status: Mutex<Option<EngineStatusView>>,
    player_error: Mutex<Option<String>>,
    sleep: Mutex<Option<SleepTimer>>,
    media: Mutex<Option<std::sync::mpsc::Sender<crate::media::MediaUpdate>>>,
    extras: Mutex<extras::ExtrasState>,
    config: Mutex<config::Config>,
    transcribers: TranscriberFactory,
    /// Recordings being synced. Lock after `library` when both are needed.
    syncing: Mutex<recording::SyncState>,
}

#[derive(Debug, Clone, Copy)]
enum SleepTimer {
    At(Instant),
    EndOfChapter(usize),
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SleepView {
    /// Milliseconds until playback pauses, when timed.
    pub remaining_ms: Option<u64>,
    pub end_of_chapter: bool,
}

fn default_profile() -> EngineProfile {
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

struct Store {
    library: Arc<Mutex<Library>>,
    audio_dir: PathBuf,
}

impl SegmentStore for Store {
    fn store<'a>(
        &'a self,
        job: &'a Job,
        synthesis: Synthesis,
    ) -> BoxFuture<'a, Result<u32, String>> {
        Box::pin(async move {
            let duration_ms = synthesis.duration_ms() as u32;
            let pcm = Pcm {
                sample_rate: synthesis.sample_rate,
                samples: synthesis.samples,
            };
            let bytes = tokio::task::spawn_blocking(move || codec::encode(&pcm))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let relative = PathBuf::from(&job.book).join(format!("{}.opus", job.key));
            let path = self.audio_dir.join(&relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
            lock(&self.library)
                .record_segment(&CachedSegment {
                    key: job.key.clone(),
                    book_id: BookId(job.book.clone()),
                    file: relative,
                    duration_ms: u64::from(duration_ms),
                    bytes: bytes.len() as u64,
                    word_timings: synthesis.word_timings.map(|t| {
                        t.into_iter()
                            .map(|(start_ms, end_ms)| aoide_library::WordTiming {
                                start_ms,
                                end_ms,
                            })
                            .collect()
                    }),
                    created_at: now_ms(),
                })
                .map_err(|e| e.to_string())?;
            Ok(duration_ms)
        })
    }
}

impl Core {
    /// Must be called inside the async runtime (spawns background tasks).
    pub fn start(environment: Environment, paths: Paths) -> Result<Arc<Self>, String> {
        let Environment {
            events,
            engines: registry,
            audio,
            background,
            show_in_folder,
            transcribers,
        } = environment;
        for dir in [&paths.library_dir, &paths.covers_dir, &paths.audio_dir] {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        if let Some(parent) = paths.db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let library = Library::open(&paths.db_path).map_err(|e| e.to_string())?;
        let config = config::Config::open(&paths.config_dir, &library);
        for problem in config.problems() {
            eprintln!("aoide: {problem}");
        }
        let profile = config
            .get::<EngineProfile>(config::ENGINE)
            .unwrap_or_else(default_profile);
        let settings = config.get::<config::PlayerSettings>(config::PLAYER);
        let engine = registry.create(&profile).map_err(|e| e.to_string())?;
        let library = Arc::new(Mutex::new(library));
        let (events_tx, events_rx) = tokio::sync::mpsc::unbounded_channel();
        let store = Arc::new(Store {
            library: Arc::clone(&library),
            audio_dir: paths.audio_dir.clone(),
        });
        let worker = Worker::spawn(
            Arc::clone(&engine),
            Arc::clone(&store) as Arc<dyn SegmentStore>,
            WorkerOptions::default(),
            events_tx.clone(),
        );
        let preparation = Preparation::load(&lock(&library));
        let mut player = PlayerCore::new(OUTPUT_RATE);
        if let Some(settings) = settings {
            player.set_speed(settings.speed);
            player.set_volume(settings.volume);
        }
        let core = Arc::new(Self {
            events,
            audio,
            show_in_folder,
            paths,
            library,
            books: Mutex::new(HashMap::new()),
            player: Arc::new(Mutex::new(player)),
            output: Mutex::new(None),
            output_latency_ms: Mutex::new(0),
            session: Mutex::new(None),
            engine: Mutex::new(ActiveEngine {
                voice: profile.voice_id.clone(),
                voices: VoiceList::Unchecked,
                profile,
                engine,
                model: None,
                launched: None,
            }),
            worker: Mutex::new(worker),
            worker_events: events_tx,
            store,
            registry,
            preparation: Mutex::new(preparation),
            generation: Mutex::new(GenerationStateView::default()),
            last_player_state: Mutex::new(None),
            last_engine_status: Mutex::new(None),
            player_error: Mutex::new(None),
            sleep: Mutex::new(None),
            media: Mutex::new(None),
            extras: Mutex::new(Default::default()),
            config: Mutex::new(config),
            transcribers,
            syncing: Mutex::new(Default::default()),
        });
        core.load_recordings(core.app_settings().recognition.resume_on_start);
        if background {
            core.spawn_sync_runner();
        }
        if background {
            *lock(&core.media) = crate::media::spawn(&core);
        }
        if background && core.app_settings().scan.on_startup {
            let scanner = Arc::clone(&core);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(8)).await;
                if let Err(error) = scanner.scan_for_books().await {
                    eprintln!("aoide: startup scan failed: {error}");
                }
            });
        }
        tokio::spawn(Arc::clone(&core).worker_events(events_rx));
        tokio::spawn(Arc::clone(&core).tick_loop());
        tokio::spawn(Arc::clone(&core).engine_loop());
        tokio::spawn(Arc::clone(&core).aligner_loop());
        let scanner = Arc::clone(&core);
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = scanner.rescan() {
                eprintln!("aoide: initial scan failed: {error}");
            }
            // Resume books left queued for preparation in an earlier run, unless turned off.
            if scanner.app_settings().resume_prepare {
                let resumer = Arc::clone(&scanner);
                runtime.spawn(async move {
                    // Voices first, so a voice chosen with another engine is not sent.
                    let _ = resumer.check_voices(false).await;
                    resumer.replan(false);
                    resumer.emit_generation();
                });
            }
            if background {
                scanner.watch_library();
            }
        });
        if background {
            let watcher = Arc::clone(&core);
            let runtime = tokio::runtime::Handle::current();
            std::thread::spawn(move || watcher.watch_config(runtime));
        }
        Ok(core)
    }

    fn emit(&self, event: CoreEvent) {
        let name = event.name();
        match event.payload() {
            Ok(value) => self.events.emit(name, value),
            Err(error) => eprintln!("aoide: cannot send {name}: {error}"),
        }
    }

    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            library_folder: self.paths.library_dir.to_string_lossy().into_owned(),
        }
    }

    // ------------------------------------------------------------ library

    fn ignored(&self) -> HashSet<String> {
        lock(&self.library)
            .setting(IGNORED_KEY)
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    fn set_ignored(&self, ignored: &HashSet<String>) {
        if let Ok(json) = serde_json::to_string(ignored) {
            let _ = lock(&self.library).set_setting(IGNORED_KEY, &json);
        }
    }

    /// Scans the library folder, importing new books and marking vanished ones missing.
    pub fn rescan(&self) -> Result<(), String> {
        let files = scan_folder(&self.paths.library_dir).map_err(|e| e.to_string())?;
        let ignored = self.ignored();
        let mut present = Vec::new();
        for file in files {
            if ignored.contains(&file.id.0) {
                continue;
            }
            present.push(file.id.clone());
            let existing = lock(&self.library)
                .book(&file.id)
                .map_err(|e| e.to_string())?;
            let book = match existing {
                Some(record) if !matches!(record.status, BookStatus::Error(_)) => NewBook {
                    id: record.id,
                    path: file.path.clone(),
                    title: record.title,
                    authors: record.authors,
                    language: record.language,
                    cover_path: record.cover_path,
                    error: None,
                },
                _ => self.describe(&file.path, &file.id),
            };
            lock(&self.library)
                .upsert_book(&book, now_ms())
                .map_err(|e| e.to_string())?;
        }
        lock(&self.library)
            .mark_missing_except(&present)
            .map_err(|e| e.to_string())?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// Parses a new book's metadata and extracts its cover.
    fn describe(&self, path: &Path, id: &BookId) -> NewBook {
        let fallback_title = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let parsed = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                aoide_epub::parse_epub(&bytes, &aoide_epub::ParseOptions::default())
                    .map_err(|e| e.to_string())
            })
            .and_then(|book: aoide_epub::ParsedBook| {
                // Parsing can succeed with nothing to read (every section empty or left out).
                if book.chapters.iter().any(|c| !c.text.trim().is_empty()) {
                    Ok(book)
                } else {
                    Err("no readable text was found in this EPUB".into())
                }
            });
        match parsed {
            Ok(book) => {
                let cover_path = book.cover.as_ref().and_then(|cover| {
                    let ext = match cover.media_type.as_str() {
                        "image/png" => "png",
                        "image/gif" => "gif",
                        "image/webp" => "webp",
                        "image/svg+xml" => "svg",
                        _ => "jpg",
                    };
                    let target = self.paths.covers_dir.join(format!("{}.{ext}", id.0));
                    std::fs::write(&target, &cover.bytes).ok().map(|_| target)
                });
                NewBook {
                    id: id.clone(),
                    path: path.to_path_buf(),
                    title: book.title.or(fallback_title),
                    authors: book.authors,
                    language: book.language,
                    cover_path,
                    error: None,
                }
            }
            Err(error) => NewBook {
                id: id.clone(),
                path: path.to_path_buf(),
                title: fallback_title,
                authors: vec![],
                language: None,
                cover_path: None,
                error: Some(error),
            },
        }
    }

    fn watch_library(self: Arc<Self>) {
        use notify::{RecursiveMode, Watcher};
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(error) => {
                eprintln!("aoide: cannot watch library folder: {error}");
                return;
            }
        };
        if let Err(error) = watcher.watch(&self.paths.library_dir, RecursiveMode::Recursive) {
            eprintln!("aoide: cannot watch library folder: {error}");
            return;
        }
        while rx.recv().is_ok() {
            // Debounce bursts (copies emit many events).
            while rx.recv_timeout(Duration::from_millis(1200)).is_ok() {}
            if let Err(error) = self.rescan() {
                eprintln!("aoide: rescan failed: {error}");
            }
        }
        drop(watcher);
    }

    pub fn import_files(&self, paths: Vec<String>) -> Vec<ImportResult> {
        let mut ignored = self.ignored();
        let results: Vec<ImportResult> = paths
            .into_iter()
            .map(|path| {
                let source = PathBuf::from(&path);
                let outcome = (|| -> Result<String, String> {
                    if !source
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
                    {
                        return Err("not an .epub file".into());
                    }
                    let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
                    let id = content_hash(&bytes);
                    ignored.remove(&id.0);
                    if !source.starts_with(&self.paths.library_dir) {
                        let name = source.file_name().ok_or("file has no name")?;
                        let mut target = self.paths.library_dir.join(name);
                        let mut n = 1;
                        while target.exists() {
                            if std::fs::read(&target).is_ok_and(|b| content_hash(&b) == id) {
                                break;
                            }
                            let stem = source.file_stem().unwrap_or_default().to_string_lossy();
                            target = self.paths.library_dir.join(format!("{stem} ({n}).epub"));
                            n += 1;
                        }
                        if !target.exists() {
                            std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(id.0)
                })();
                match outcome {
                    Ok(id) => ImportResult {
                        path,
                        book_id: Some(id),
                        error: None,
                    },
                    Err(error) => ImportResult {
                        path,
                        book_id: None,
                        error: Some(error),
                    },
                }
            })
            .collect();
        self.set_ignored(&ignored);
        if let Err(error) = self.rescan() {
            eprintln!("aoide: rescan after import failed: {error}");
        }
        // A copied file the parser could not read is a failed import, not a book.
        let library = lock(&self.library);
        results
            .into_iter()
            .map(|result| {
                let failed = result.book_id.as_ref().and_then(|id| {
                    match library.book(&BookId(id.clone())).ok().flatten()?.status {
                        BookStatus::Error(message) => Some(message),
                        _ => None,
                    }
                });
                match failed {
                    Some(message) => ImportResult {
                        path: result.path,
                        book_id: None,
                        error: Some(format!("could not import: {message}")),
                    },
                    None => result,
                }
            })
            .collect()
    }

    /// Opens the file manager on the book's folder with its EPUB selected.
    pub fn show_in_folder(&self, id: &str) -> Result<(), String> {
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("no such book")?;
        if !record.path.is_file() {
            return Err(format!(
                "the book's file is missing: {}",
                record.path.display()
            ));
        }
        (self.show_in_folder)(&record.path)
    }

    pub fn remove_book(&self, id: &str) -> Result<(), String> {
        if lock(&self.session)
            .as_ref()
            .is_some_and(|s| s.target.book.id == id)
        {
            self.stop();
        }
        {
            let mut preparation = lock(&self.preparation);
            preparation.forget(&lock(&self.library), id);
        }
        self.forget_recording(id);
        self.replan(false);
        let files = lock(&self.library)
            .remove_book(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        for file in files {
            let _ = std::fs::remove_file(self.paths.audio_dir.join(file));
        }
        let _ = std::fs::remove_dir(self.paths.audio_dir.join(id));
        let mut ignored = self.ignored();
        ignored.insert(id.into());
        self.set_ignored(&ignored);
        lock(&self.books).remove(id);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    fn progress_view(&self, id: &BookId) -> Option<BookProgressView> {
        let library = lock(&self.library);
        let progress = library.progress(id).ok().flatten()?;
        let fraction = library
            .setting(&format!("progress.fraction.{}", id.0))
            .ok()
            .flatten()
            .and_then(|f| f.parse().ok())
            .unwrap_or(0.0);
        Some(BookProgressView {
            chapter: progress.chapter,
            segment_id: progress.segment_id,
            fraction,
        })
    }

    pub fn list_books(&self) -> Result<Vec<BookSummary>, String> {
        let (records, mut shelves) = {
            let library = lock(&self.library);
            (
                library.books().map_err(|e| e.to_string())?,
                library.book_collections().map_err(|e| e.to_string())?,
            )
        };
        let ignored = self.ignored();
        Ok(records
            .into_iter()
            .filter(|r| !ignored.contains(&r.id.0))
            .map(|record| {
                let collections = shelves.remove(&record.id).unwrap_or_default();
                self.summary(record, collections)
            })
            .collect())
    }

    fn summary(&self, record: BookRecord, collections: Vec<i64>) -> BookSummary {
        let audio_bytes = lock(&self.library)
            .book_audio_bytes(&record.id)
            .unwrap_or(0);
        BookSummary {
            prepare: self.prepare_view(&record.id.0),
            recording: self.recording_stage(&record.id.0),
            progress: self.progress_view(&record.id),
            id: record.id.0,
            title: record.title.unwrap_or_else(|| "Untitled".into()),
            authors: record.authors,
            language: record.language,
            status: record.status,
            cover_path: record.cover_path.map(|p| p.to_string_lossy().into_owned()),
            audio_bytes,
            added_at: record.added_at,
            last_opened_at: record.last_opened_at,
            collections,
        }
    }

    // ------------------------------------------------------------ collections

    pub fn collections(&self) -> Result<Vec<aoide_library::Collection>, String> {
        lock(&self.library).collections().map_err(|e| e.to_string())
    }

    /// Runs a collection change and tells the UI the library changed.
    fn shelve<T>(
        &self,
        change: impl FnOnce(&Library) -> aoide_library::Result<T>,
    ) -> Result<T, String> {
        let result = change(&lock(&self.library)).map_err(|e| e.to_string())?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(result)
    }

    pub fn create_collection(&self, name: &str) -> Result<i64, String> {
        self.shelve(|library| library.create_collection(name, now_ms()))
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> Result<(), String> {
        self.shelve(|library| library.rename_collection(id, name))
    }

    pub fn delete_collection(&self, id: i64) -> Result<(), String> {
        self.shelve(|library| library.delete_collection(id))
    }

    pub fn set_book_collections(&self, book_id: &str, ids: &[i64]) -> Result<(), String> {
        self.shelve(|library| library.set_book_collections(&BookId(book_id.into()), ids))
    }

    // ------------------------------------------------------------ books

    pub fn load_book(&self, id: &str) -> Result<Arc<BookModel>, String> {
        if let Some(book) = lock(&self.books).get(id) {
            return Ok(Arc::clone(book));
        }
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("book not found")?;
        match record.status {
            BookStatus::Present => {}
            BookStatus::Missing => return Err("the book file is missing".into()),
            BookStatus::Error(message) => return Err(message),
        }
        let bytes = std::fs::read(&record.path).map_err(|e| e.to_string())?;
        let settings = self.book_settings(id);
        // A recording is laid onto the book's own text, without footnotes read in between.
        let read_notes = settings.read_notes && !self.plays_recording(id, &settings);
        let book = Arc::new(BookModel::parse(id, &bytes, read_notes)?);
        let mut books = lock(&self.books);
        if books.len() >= 3 {
            let session_book = lock(&self.session)
                .as_ref()
                .map(|s| s.target.book.id.clone());
            books.retain(|k, _| Some(k) == session_book.as_ref());
        }
        books.insert(id.into(), Arc::clone(&book));
        Ok(book)
    }

    fn language_warning(&self, language: Option<&str>) -> Option<String> {
        let language = language?;
        let engine = lock(&self.engine);
        let descriptor = engine.engine.descriptor();
        (!supports_language(descriptor, language)).then(|| {
            format!(
                "{} does not list the book's language ({language}); narration may sound wrong.",
                engine.profile.name
            )
        })
    }

    fn voice_warning(&self, book: &BookModel) -> Option<String> {
        let settings = self.book_settings(&book.id);
        let engine = lock(&self.engine);
        let voices = engine.voices.offered()?;
        let voicing = Voicing::resolve(
            engine.engine.descriptor(),
            &engine.profile.params,
            engine.voice.as_deref(),
            &settings,
            None,
            fallback_voice(voices).as_deref(),
            Some(voices),
        )
        .ok()?;
        let missing = voicing.missing_voice?;
        let name = |id: &str| {
            voices.iter().find(|v| v.id == id).map_or_else(
                || id.trim_end_matches(".wav").to_owned(),
                |v| v.display_name.clone(),
            )
        };
        Some(format!(
            "{} is not one of {}'s voices, so {} reads this book. Choose a voice in the book's settings.",
            name(&missing),
            engine.profile.name,
            name(&voicing.voice_id)
        ))
    }

    pub fn open_book(&self, id: &str) -> Result<BookView, String> {
        let view = self.book_overview(id)?;
        let _ = lock(&self.library).set_last_opened(&BookId(id.into()), now_ms());
        Ok(view)
    }

    /// The same view as [`Core::open_book`], without marking the book as opened.
    pub fn book_overview(&self, id: &str) -> Result<BookView, String> {
        let book = self.load_book(id)?;
        Ok(BookView {
            id: book.id.clone(),
            title: book.title.clone(),
            authors: book.authors.clone(),
            language: book.language.clone(),
            chapters: book.chapter_summaries(),
            warnings: book.warnings.clone(),
            excluded: book.excluded.clone(),
            language_warning: self.language_warning(book.language.as_deref()),
            voice_warning: self.voice_warning(&book),
            progress: self.progress_view(&BookId(id.into())),
            word_count: book.word_count(),
        })
    }

    pub fn chapter(&self, id: &str, chapter: usize) -> Result<ChapterView, String> {
        let book = self.load_book(id)?;
        let mut view = book
            .chapter_view(chapter)
            .ok_or_else(|| "no such chapter".to_owned())?;
        view.not_narrated = self.not_narrated(book, chapter);
        Ok(view)
    }

    // ------------------------------------------------------------ playback

    /// Learns the engine's voices and settles the default voice before anything is planned.
    async fn ensure_voice(&self) -> Result<String, String> {
        let checked = self.check_voices(false).await;
        if let Some(voice) = lock(&self.engine).voice.clone() {
            return Ok(voice);
        }
        checked?;
        Err("the speech engine has no voices".into())
    }

    /// Asks the engine for its voices once. The default voice becomes the profile's if the
    /// engine offers it, otherwise the engine's own. `ready`: the engine just reported ready,
    /// so a failed listing means it cannot list voices rather than that it is down.
    async fn check_voices(&self, ready: bool) -> Result<(), String> {
        let engine = {
            let state = lock(&self.engine);
            if state.voices != VoiceList::Unchecked {
                return Ok(());
            }
            Arc::clone(&state.engine)
        };
        let listed = engine.voices().await;
        let mut state = lock(&self.engine);
        if !Arc::ptr_eq(&state.engine, &engine) {
            return Ok(()); // Switched meanwhile; the new engine is asked on its own.
        }
        match listed {
            Ok(voices) if voices.is_empty() => state.voices = VoiceList::Unlisted,
            Ok(voices) => {
                let named = state
                    .profile
                    .voice_id
                    .clone()
                    .filter(|v| !v.trim().is_empty());
                let offered = named.clone().filter(|v| voices.iter().any(|o| &o.id == v));
                if let (Some(voice), None) = (&named, &offered) {
                    eprintln!(
                        "aoide: {} has no voice {voice}; using its default",
                        state.profile.name
                    );
                }
                state.voice = offered.or_else(|| fallback_voice(&voices));
                state.voices = VoiceList::Known(voices);
            }
            Err(error) => {
                if ready {
                    state.voices = VoiceList::Unlisted;
                }
                return Err(error.to_string());
            }
        }
        Ok(())
    }

    async fn ensure_model(&self) {
        if lock(&self.engine).model.is_some() {
            return;
        }
        let engine = Arc::clone(&lock(&self.engine).engine);
        if let Ok(EngineStatus::Ready(model)) =
            tokio::time::timeout(Duration::from_secs(3), engine.status()).await
        {
            lock(&self.engine).model = model;
        }
    }

    fn ensure_output(&self) -> Result<(), String> {
        let mut output = lock(&self.output);
        if output.as_ref().is_some_and(PipeOutput::is_running) {
            return Ok(());
        }
        let started =
            PipeOutput::start(Arc::clone(&self.player), &self.audio).map_err(|e| e.to_string())?;
        *lock(&self.output_latency_ms) = started.latency_ms();
        *output = Some(started);
        Ok(())
    }

    pub async fn play(
        &self,
        book_id: &str,
        chapter: Option<usize>,
        segment_id: Option<String>,
    ) -> Result<(), String> {
        let book = self.load_book(book_id)?;
        let same_book = lock(&self.session)
            .as_ref()
            .is_some_and(|s| s.target.book.id == book_id);
        let start = segment_id
            .as_deref()
            .and_then(|id| book.index_of(id))
            .or_else(|| chapter.and_then(|c| book.chapter_start(c)));
        if same_book {
            if let Some(index) = start {
                self.seek(index);
            }
            lock(&self.player).play();
            return self.ensure_output();
        }
        let recording = self.plays_recording(book_id, &self.book_settings(book_id));
        let start = start
            .or_else(|| {
                let progress = lock(&self.library)
                    .progress(&BookId(book_id.into()))
                    .ok()
                    .flatten()?;
                book.index_at(progress.chapter as usize, progress.char_offset as usize)
            })
            .or_else(|| book.chapter_start(0))
            .ok_or("the book has no readable text")?;
        let recorded = if recording {
            self.build_recording_target(Arc::clone(&book))
                .map_err(|error| eprintln!("aoide: {error}; reading with the speech engine"))
                .ok()
        } else {
            None
        };
        let target = match recorded {
            Some(target) => target,
            None => {
                self.ensure_voice().await?;
                self.ensure_model().await;
                self.build_target(Arc::clone(&book))?
            }
        };
        let entries = (0..book.total_entries())
            .map(|i| {
                let segment = book.segment(i).expect("index in range");
                match &target.source {
                    // The clips run on into each other: their pauses are in the recording.
                    Source::Recording(_) => Entry {
                        id: segment.id.clone(),
                        speakable: target.plan.items[i].speakable,
                        pause_after_ms: 0,
                    },
                    Source::Speech => Entry {
                        id: segment.id.clone(),
                        speakable: !segment.speech.is_empty(),
                        pause_after_ms: segment.pause_after_ms,
                    },
                }
            })
            .collect();
        let offset = match &target.source {
            Source::Recording(plan) => plan.start_offset(start),
            Source::Speech => 0,
        };
        {
            let mut player = lock(&self.player);
            player.load(entries, start);
            player.seek_to(start, offset);
            player.play();
        }
        *lock(&self.session) = Some(Session::new(target, start, OUTPUT_RATE));
        *lock(&self.player_error) = None;
        self.replan(true);
        self.ensure_output()
    }

    // ------------------------------------------------------------ settings

    pub fn book_settings(&self, id: &str) -> BookSettings {
        lock(&self.library)
            .setting(&book_settings_key(id))
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    pub async fn save_book_settings(&self, id: &str, settings: BookSettings) -> Result<(), String> {
        {
            let engine = lock(&self.engine);
            resolve_params(engine.engine.descriptor(), &settings.params)
                .map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string(&settings).map_err(|e| e.to_string())?;
        lock(&self.library)
            .set_setting(&book_settings_key(id), &json)
            .map_err(|e| e.to_string())?;
        // Footnote narration changes segmentation: reparse.
        lock(&self.books).remove(id);
        let playing = lock(&self.session)
            .as_ref()
            .is_some_and(|s| s.target.book.id == id);
        if playing {
            self.restart_session().await?;
        }
        lock(&self.preparation).drop_plans(Some(id));
        self.replan(true);
        Ok(())
    }

    pub fn engine_options(&self) -> EngineOptions {
        EngineOptions {
            descriptors: self.registry.descriptors(),
            profile: lock(&self.engine).profile.clone(),
        }
    }

    pub async fn engine_voices(&self) -> Result<Vec<aoide_tts::Voice>, String> {
        let engine = Arc::clone(&lock(&self.engine).engine);
        engine.voices().await.map_err(|e| e.to_string())
    }

    /// Speaks a short sample sentence with a profile that may not be saved yet (the settings
    /// form) or with a book's voice overrides, and returns it as a WAV file. The engine is built
    /// through the registry, so the profile's network rules apply as for generation.
    pub async fn preview_voice(
        &self,
        profile: EngineProfile,
        voice_id: Option<String>,
        params: Params,
        language: Option<String>,
    ) -> Result<Vec<u8>, String> {
        let engine = self.registry.create(&profile).map_err(|e| e.to_string())?;
        let descriptor = engine.descriptor().clone();
        let book = BookSettings {
            voice_id: voice_id.filter(|v| !v.trim().is_empty()),
            params,
            ..BookSettings::default()
        };
        let named = profile
            .voice_id
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty());
        let fallback = if book.voice_id.is_none() && !named {
            fallback_voice(&engine.voices().await.map_err(|e| e.to_string())?)
        } else {
            None
        };
        // The preview is told the book's language (it may not be the open book).
        let request = Voicing::resolve(
            &descriptor,
            &profile.params,
            profile.voice_id.as_deref(),
            &book,
            language.as_deref(),
            fallback.as_deref(),
            None, // The settings form previews exactly the voice it names.
        )?
        .request(PREVIEW_TEXT.into());
        let audio = engine
            .synthesize(&request)
            .await
            .map_err(|e| e.to_string())?;
        encode_wav(&audio).map_err(|e| e.to_string())
    }

    /// Validates, persists and activates a new engine profile. Must run inside the runtime.
    pub async fn save_engine_profile(&self, profile: EngineProfile) -> Result<(), String> {
        let engine = self.registry.create(&profile).map_err(|e| e.to_string())?;
        resolve_params(engine.descriptor(), &profile.params).map_err(|e| e.to_string())?;
        lock(&self.config).update(|file| file.set_section(config::ENGINE, &profile))?;
        self.activate_profile(profile, engine).await
    }

    /// Switches generation to a validated profile.
    async fn activate_profile(
        &self,
        profile: EngineProfile,
        engine: Arc<dyn SpeechEngine>,
    ) -> Result<(), String> {
        let worker = Worker::spawn(
            Arc::clone(&engine),
            Arc::clone(&self.store) as Arc<dyn SegmentStore>,
            WorkerOptions::default(),
            self.worker_events.clone(),
        );
        // The old worker finishes any in-flight request and then stops.
        *lock(&self.worker) = worker;
        {
            let mut state = lock(&self.engine);
            state.voice = profile.voice_id.clone();
            state.voices = VoiceList::Unchecked;
            state.profile = profile;
            state.engine = engine;
            state.model = None;
        }
        *lock(&self.last_engine_status) = None;
        lock(&self.generation).generating_segment_id = None;
        if lock(&self.session).is_some() {
            self.restart_session().await?;
        }
        lock(&self.preparation).drop_plans(None);
        self.replan(true);
        Ok(())
    }

    /// Rebuilds the listening session's requests and cache state (voice/engine/settings
    /// changed), keeping the playhead.
    async fn restart_session(&self) -> Result<(), String> {
        let (id, index, playing) = {
            let session = lock(&self.session);
            let Some(session) = session.as_ref() else {
                return Ok(());
            };
            let position = lock(&self.player).position();
            (
                session.target.book.id.clone(),
                position.index,
                matches!(position.status, Status::Playing | Status::Buffering),
            )
        };
        let segment = lock(&self.session)
            .as_ref()
            .and_then(|s| s.target.book.segment(index).map(|seg| seg.id.clone()));
        *lock(&self.session) = None;
        self.play(&id, None, segment).await?;
        if !playing {
            lock(&self.player).pause();
        }
        Ok(())
    }

    /// Deletes a book's generated audio; it will be regenerated on demand.
    pub async fn regenerate_book(&self, id: &str) -> Result<(), String> {
        // The worker skips keys it made in this run unless told they are gone.
        if let Ok(target) = self.load_book(id).and_then(|book| self.build_target(book)) {
            lock(&self.worker).forget(target.plan.items.into_iter().map(|item| item.key));
        }
        let files = lock(&self.library)
            .delete_book_segments(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        for file in files {
            let _ = std::fs::remove_file(self.paths.audio_dir.join(file));
        }
        {
            let mut preparation = lock(&self.preparation);
            preparation.audio_lost(
                &lock(&self.library),
                &HashSet::from([id.to_owned()]),
                &HashSet::new(),
            );
            preparation.drop_plans(Some(id));
        }
        let playing = lock(&self.session)
            .as_ref()
            .is_some_and(|s| s.target.book.id == id);
        if playing {
            self.restart_session().await?;
        }
        self.replan(true);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    // ------------------------------------------------------------ preparation

    /// Plans made before the engine listed its voices (it was down) may name a voice it does
    /// not offer: make them again now that the list is known.
    async fn replan_with_voices(&self) {
        if let Err(error) = self.restart_session().await {
            eprintln!("aoide: {error}");
        }
        let planned = !lock(&self.preparation).planned_books().is_empty();
        if planned {
            lock(&self.preparation).drop_plans(None);
            self.replan(false);
        }
    }

    pub async fn prepare_book(&self, id: &str) -> Result<(), String> {
        self.load_book(id)?;
        self.ensure_voice().await?;
        self.ensure_model().await;
        {
            let mut preparation = lock(&self.preparation);
            preparation.enqueue(&lock(&self.library), id);
        }
        // Plans it now: a book whose audio is all made already finishes here.
        self.replan(false);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub fn cancel_prepare(&self, id: &str) {
        {
            let mut preparation = lock(&self.preparation);
            preparation.cancel(&lock(&self.library), id);
        }
        self.replan(false);
        self.emit(CoreEvent::LibraryChanged);
    }

    /// Pauses or resumes a queued book's preparation; the choice survives restarts.
    pub fn set_prepare_paused(&self, id: &str, paused: bool) {
        {
            let mut preparation = lock(&self.preparation);
            preparation.set_paused(&lock(&self.library), id, paused);
        }
        self.replan(false);
        self.emit_generation();
        self.emit(CoreEvent::LibraryChanged);
    }

    /// Builds plans for queued books waiting for one. A book that cannot be loaded leaves the queue.
    fn plan_waiting_books(&self) {
        let waiting = lock(&self.preparation).unplanned();
        let mut changed = false;
        for id in waiting {
            if lock(&self.engine).voice.is_none() && self.book_settings(&id).voice_id.is_none() {
                continue; // Voice not known yet; planned once playback or preparation resolves it.
            }
            let built = self.load_book(&id).and_then(|book| self.build_target(book));
            let mut preparation = lock(&self.preparation);
            let library = lock(&self.library);
            match built {
                Ok(mut target) => {
                    if let Some(chapters) = preparation.chapters(&id) {
                        let book = &target.book;
                        target.plan.only(|index| {
                            book.locate(index)
                                .is_some_and(|(c, _)| chapters.contains(&c))
                        });
                    }
                    preparation.planned(&library, &id, target.plan)
                }
                Err(error) => {
                    eprintln!("aoide: cannot prepare {id}: {error}");
                    preparation.cancel(&library, &id);
                }
            }
            changed = true;
        }
        if changed {
            self.emit(CoreEvent::LibraryChanged);
        }
    }

    /// Queues one chapter of a book for preparation (joining the book's entry if it has one).
    pub async fn prepare_chapter(&self, id: &str, chapter: usize) -> Result<(), String> {
        let book = self.load_book(id)?;
        if chapter >= book.chapters.len() {
            return Err("no such chapter".into());
        }
        self.ensure_voice().await?;
        self.ensure_model().await;
        {
            let mut preparation = lock(&self.preparation);
            preparation.enqueue_chapter(&lock(&self.library), id, chapter);
        }
        self.replan(false);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// How much of each chapter's audio is made, in the voice and settings it would be read in.
    pub async fn chapter_audio(&self, id: &str) -> Result<Vec<ChapterAudioView>, String> {
        let book = self.load_book(id)?;
        let _ = self.ensure_voice().await; // Without a voice nothing can be cached yet.
        let target = self.build_target(Arc::clone(&book))?;
        let mut chapters = vec![ChapterAudioView { done: 0, total: 0 }; book.chapters.len()];
        for (index, item) in target.plan.items.iter().enumerate() {
            let Some((chapter, _)) = book.locate(index) else {
                continue;
            };
            if item.speakable {
                chapters[chapter].total += 1;
                chapters[chapter].done += usize::from(item.cached);
            }
        }
        Ok(chapters)
    }

    pub fn prepare_view(&self, id: &str) -> Option<PrepareView> {
        let preparation = lock(&self.preparation);
        preparation.view(&lock(&self.library), id)
    }

    fn build_target(&self, book: Arc<BookModel>) -> Result<Target, String> {
        let settings = self.book_settings(&book.id);
        let engine = lock(&self.engine);
        let descriptor = engine.engine.descriptor().clone();
        let offered = engine.voices.offered();
        let voicing = Voicing::resolve(
            &descriptor,
            &engine.profile.params,
            engine.voice.as_deref(),
            &settings,
            book.language.as_deref(),
            offered.and_then(fallback_voice).as_deref(),
            offered,
        )?;
        let model = engine.model.clone();
        drop(engine);
        let mut requests = Vec::with_capacity(book.total_entries());
        let mut keys = Vec::new();
        for index in 0..book.total_entries() {
            let segment = book.segment(index).expect("index in range");
            if segment.speech.is_empty() {
                requests.push(None);
                continue;
            }
            let request = voicing.request(segment.speech.clone());
            keys.push(cache_key(&descriptor, model.as_deref(), &request));
            requests.push(Some(request));
        }
        let library = lock(&self.library);
        let mut keys = keys.into_iter();
        let items = requests
            .iter()
            .enumerate()
            .map(|(index, request)| match request {
                None => PlanItem {
                    key: format!("silent:{index}"),
                    speakable: false,
                    cached: false,
                    duration_ms: book.segment(index).map_or(0, |s| s.pause_after_ms),
                },
                Some(request) => {
                    let key = keys.next().expect("one key per request");
                    let cached = library.segment(&key).ok().flatten();
                    PlanItem {
                        duration_ms: cached.as_ref().map_or_else(
                            || estimate_ms(request.text.chars().count()),
                            |c| c.duration_ms as u32,
                        ),
                        cached: cached.is_some(),
                        speakable: true,
                        key,
                    }
                }
            })
            .collect();
        Ok(Target {
            book,
            plan: Plan { items, requests },
            source: Source::Speech,
        })
    }

    fn seek(&self, index: usize) {
        self.seek_at(index, None);
    }

    /// Moves the playhead `offset_ms` into entry `index`; `None` starts where the entry's text
    /// does (for a recording, just before its first word).
    fn seek_at(&self, index: usize, offset_ms: Option<u32>) {
        let (offset, released) = match lock(&self.session).as_mut() {
            Some(session) => {
                session.last_index = index;
                let offset = offset_ms.unwrap_or_else(|| {
                    session
                        .recording()
                        .map_or(0, |plan| plan.start_offset(index))
                });
                (offset, session.restart_feed())
            }
            None => (0, Vec::new()),
        };
        {
            let mut player = lock(&self.player);
            for entry in released {
                player.release(entry);
            }
            player.seek_to(index, offset);
        }
        self.save_progress(true);
        self.replan(true);
    }

    /// Recomputes the generation plan from the playhead and the prepare queue. `jump` debounces.
    fn replan(&self, jump: bool) {
        self.plan_waiting_books();
        let session = lock(&self.session);
        let preparation = lock(&self.preparation);
        let planned = preparation.planned_books();
        let playhead = lock(&self.player).position().index;
        // A recording is not made by the speech engine (what it lacks waits for its sync).
        let listening = session
            .as_ref()
            .filter(|s| s.recording().is_none())
            .map(|s| Listening {
                book: &s.target.book.id,
                items: &s.target.plan.items,
                playhead,
                lookahead_ms: LOOKAHEAD_MS,
            });
        let preparing: Vec<Preparing<'_>> = planned
            .iter()
            .map(|(book, plan)| Preparing {
                book,
                items: &plan.items,
            })
            .collect();
        let jobs = plan(listening.as_ref(), &preparing);
        let pending = jobs.len();
        let work = jobs
            .into_iter()
            .filter_map(|job| {
                let requests = match session.as_ref() {
                    Some(s) if s.target.book.id == job.book => &s.target.plan.requests,
                    _ => {
                        &planned
                            .iter()
                            .find(|(book, _)| *book == job.book)?
                            .1
                            .requests
                    }
                };
                let request = requests.get(job.index)?.clone()?;
                Some(WorkItem { job, request })
            })
            .collect();
        {
            let mut generation = lock(&self.generation);
            // Only speech is made; a recording has nothing to report here.
            let speech = session.as_ref().filter(|s| s.recording().is_none());
            generation.book_id = speech.map(|s| s.target.book.id.clone());
            generation.pending = pending;
            generation.buffered_ahead_ms =
                speech.map_or(0, |s| buffered_ahead_ms(&s.target.plan.items, playhead));
        }
        let worker = lock(&self.worker);
        if jump {
            worker.set_plan(work);
        } else {
            worker.update_plan(work);
        }
    }

    pub fn pause(&self) {
        lock(&self.player).pause();
        self.save_progress(true);
    }

    pub fn resume(&self) -> Result<(), String> {
        lock(&self.player).play();
        self.ensure_output()
    }

    pub fn stop(&self) {
        self.save_progress(true);
        lock(&self.player).load(Vec::new(), 0);
        *lock(&self.session) = None;
        *lock(&self.output) = None;
        *lock(&self.generation) = GenerationStateView::default();
        self.replan(true);
    }

    pub fn toggle(&self) {
        let status = lock(&self.player).position().status;
        if matches!(status, Status::Playing | Status::Buffering) {
            self.pause();
        } else {
            let _ = self.resume();
        }
    }

    /// Plays from the start of the chapter `delta` chapters away from the current one.
    pub async fn skip_chapter(&self, delta: i64) -> Result<(), String> {
        let (id, target) = {
            let session = lock(&self.session);
            let session = session.as_ref().ok_or("nothing is playing")?;
            let index = lock(&self.player).position().index;
            let (chapter, _) = session.target.book.locate(index).unwrap_or((0, 0));
            let last = session.target.book.chapters.len().saturating_sub(1) as i64;
            (
                session.target.book.id.clone(),
                (chapter as i64 + delta).clamp(0, last) as usize,
            )
        };
        self.play(&id, Some(target), None).await
    }

    /// `minutes`: pause after that long; `end_of_chapter`: pause when the chapter ends.
    /// Neither cancels the timer.
    pub fn set_sleep_timer(&self, minutes: Option<u32>, end_of_chapter: bool) {
        let timer = if end_of_chapter {
            lock(&self.session).as_ref().and_then(|s| {
                let index = lock(&self.player).position().index;
                s.target
                    .book
                    .locate(index)
                    .map(|(c, _)| SleepTimer::EndOfChapter(c))
            })
        } else {
            minutes.map(|m| SleepTimer::At(Instant::now() + Duration::from_secs(u64::from(m) * 60)))
        };
        *lock(&self.sleep) = timer;
    }

    fn check_sleep(&self, chapter: usize) {
        let mut sleep = lock(&self.sleep);
        let due = match *sleep {
            Some(SleepTimer::At(at)) => Instant::now() >= at,
            Some(SleepTimer::EndOfChapter(c)) => chapter != c,
            None => false,
        };
        if due {
            *sleep = None;
            drop(sleep);
            self.pause();
        }
    }

    fn sleep_view(&self) -> Option<SleepView> {
        match *lock(&self.sleep) {
            Some(SleepTimer::At(at)) => Some(SleepView {
                remaining_ms: Some(at.saturating_duration_since(Instant::now()).as_millis() as u64),
                end_of_chapter: false,
            }),
            Some(SleepTimer::EndOfChapter(_)) => Some(SleepView {
                remaining_ms: None,
                end_of_chapter: true,
            }),
            None => None,
        }
    }

    /// Case-insensitive search of the book's text; at most 200 hits in reading order.
    pub fn search_book(&self, id: &str, query: &str) -> Result<Vec<SearchHit>, String> {
        Ok(self.load_book(id)?.search(query, 200))
    }

    fn send_now_playing(&self, chapter: usize) {
        let Some((id, title, authors, chapter_title)) = lock(&self.session).as_ref().map(|s| {
            let book = &s.target.book;
            (
                book.id.clone(),
                book.title.clone(),
                book.authors.join(", "),
                book.chapters
                    .get(chapter)
                    .map(|c| c.title.clone())
                    .unwrap_or_default(),
            )
        }) else {
            return;
        };
        let cover = lock(&self.library)
            .book(&BookId(id))
            .ok()
            .flatten()
            .and_then(|r| r.cover_path)
            .map(|p| p.to_string_lossy().into_owned());
        self.send_media(crate::media::MediaUpdate::Metadata {
            title: chapter_title,
            album: title,
            artist: authors,
            cover,
        });
    }

    fn send_media(&self, update: crate::media::MediaUpdate) {
        if let Some(tx) = lock(&self.media).as_ref() {
            let _ = tx.send(update);
        }
    }

    pub fn set_speed(&self, speed: f32) {
        lock(&self.player).set_speed(speed);
        self.save_player_settings();
    }

    pub fn set_volume(&self, volume: f32) {
        lock(&self.player).set_volume(volume);
        self.save_player_settings();
    }

    fn save_player_settings(&self) {
        let settings = {
            let player = lock(&self.player);
            config::PlayerSettings {
                speed: player.speed(),
                volume: player.volume(),
            }
        };
        let saved = lock(&self.config).update(|file| file.set_section(config::PLAYER, &settings));
        if let Err(error) = saved {
            eprintln!("aoide: {error}");
        }
    }

    fn save_progress(&self, force: bool) {
        let mut session = lock(&self.session);
        let Some(session) = session.as_mut() else {
            return;
        };
        if !force && session.last_saved.elapsed() < Duration::from_secs(3) {
            return;
        }
        let index = lock(&self.player)
            .position()
            .index
            .min(session.target.book.total_entries().saturating_sub(1));
        let Some((chapter, local)) = session.target.book.locate(index) else {
            return;
        };
        let segment = &session.target.book.chapters[chapter].segments[local];
        let id = BookId(session.target.book.id.clone());
        let library = lock(&self.library);
        let _ = library.save_progress(
            &id,
            &Progress {
                chapter: chapter as u32,
                char_offset: segment.range.start as u32,
                segment_id: Some(segment.id.clone()),
                updated_at: now_ms(),
            },
        );
        let _ = library.set_setting(
            &format!("progress.fraction.{}", id.0),
            &session.target.book.fraction(index).to_string(),
        );
        session.last_saved = Instant::now();
    }

    // ------------------------------------------------------------ background loops

    async fn worker_events(
        self: Arc<Self>,
        mut events: tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
    ) {
        while let Some(event) = events.recv().await {
            match event {
                WorkerEvent::Started(job) => {
                    let id = self.segment_id_for(&job);
                    lock(&self.generation).generating_segment_id = id;
                    self.emit_engine_status(EngineStatusView {
                        profile_name: lock(&self.engine).profile.name.clone(),
                        state: EngineState::Busy,
                        detail: None,
                    });
                }
                WorkerEvent::Finished { job, duration_ms } => {
                    // Keys are content hashes, so mark every plan that shares them.
                    if let Some(session) = lock(&self.session).as_mut() {
                        session.segment_made(&job.key, duration_ms);
                    }
                    let finished = {
                        let mut preparation = lock(&self.preparation);
                        preparation.segment_made(&lock(&self.library), &job.key, duration_ms)
                    };
                    if finished {
                        self.emit(CoreEvent::LibraryChanged);
                    }
                    self.enforce_cache_limit();
                    {
                        let mut generation = lock(&self.generation);
                        generation.generating_segment_id = None;
                        generation.error = None;
                    }
                    self.replan(false);
                }
                WorkerEvent::Failed { job, error } => {
                    eprintln!("aoide: generation failed for {}: {error}", job.key);
                    let mut generation = lock(&self.generation);
                    generation.generating_segment_id = None;
                    generation.error = Some(error);
                }
            }
            self.emit_generation();
        }
    }

    fn segment_id_for(&self, job: &Job) -> Option<String> {
        if let Some(session) = lock(&self.session)
            .as_ref()
            .filter(|s| s.target.book.id == job.book)
        {
            return session.target.book.segment(job.index).map(|s| s.id.clone());
        }
        self.load_book(&job.book)
            .ok()?
            .segment(job.index)
            .map(|s| s.id.clone())
    }

    fn emit_generation(&self) {
        let preparing = lock(&self.preparation)
            .current()
            .map(|(id, done, total)| PreparingView {
                book_id: id.into(),
                done,
                total,
            });
        lock(&self.generation).preparing = preparing;
        let state = lock(&self.generation).clone();
        self.emit(CoreEvent::GenerationState(state));
    }

    async fn tick_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(TICK);
        let mut last_index = usize::MAX;
        loop {
            interval.tick().await;
            let index = self.provide_audio();
            if let Some(index) = index {
                if index != last_index {
                    if let Some(session) = lock(&self.session).as_mut() {
                        session.last_index = index;
                    }
                    self.replan(false);
                    self.save_progress(false);
                    self.emit_generation();
                    last_index = index;
                }
            }
            self.keep_output_alive();
            let state = self.player_state();
            self.check_sleep(state.chapter);
            self.track_listening(&state);
            let mut last = lock(&self.last_player_state);
            let previous = last.as_ref();
            if previous.map(|p| p.status) != Some(state.status) {
                self.send_media(crate::media::MediaUpdate::Playback(state.status));
            }
            if state.book_id.is_some()
                && previous.map(|p| (&p.book_id, p.chapter))
                    != Some((&state.book_id, state.chapter))
            {
                self.send_now_playing(state.chapter);
            }
            if last.as_ref() != Some(&state) {
                self.emit(CoreEvent::PlayerState(state.clone()));
                *last = Some(state);
            }
        }
    }

    /// Restarts the audio output if its player process died while playing.
    fn keep_output_alive(&self) {
        let playing = matches!(
            lock(&self.player).position().status,
            Status::Playing | Status::Buffering
        );
        let dead = lock(&self.output)
            .as_ref()
            .is_some_and(|output| !output.is_running());
        if playing && dead {
            if let Err(error) = self.ensure_output() {
                lock(&self.player).pause();
                *lock(&self.player_error) = Some(format!("Audio output failed: {error}"));
            }
        }
    }

    /// Decodes cached audio near the playhead into the player. Returns the playhead index.
    /// Gives the player decoded audio for made entries near the playhead, and lets it release
    /// audio far behind. Returns the playhead.
    fn provide_audio(&self) -> Option<usize> {
        let index = lock(&self.player).position().index;
        let recording = lock(&self.session).as_ref()?.recording().is_some();
        if recording {
            return self.provide_recording(index);
        }
        let wanted = lock(&self.session).as_ref()?.wanted(index);
        let mut align = Vec::new();
        for (i, key) in wanted {
            let segment = lock(&self.library).segment(&key).ok().flatten();
            let Some(segment) = segment else { continue };
            let pcm = std::fs::read(self.paths.audio_dir.join(&segment.file))
                .map_err(|e| e.to_string())
                .and_then(|bytes| codec::decode(&bytes).map_err(|e| e.to_string()));
            match pcm {
                Ok(pcm) => {
                    lock(&self.player).provide(i, &pcm);
                    if let Some(session) = lock(&self.session).as_mut() {
                        if session.provided(i, segment.word_timings.as_deref(), &pcm) {
                            align.push((session.target.book.id.clone(), i, key.clone()));
                        }
                    }
                }
                Err(error) => eprintln!("aoide: cannot read cached audio {key}: {error}"),
            }
        }
        if !align.is_empty() && self.app_settings().aligner.is_some() {
            for (book, i, key) in align {
                self.queue_alignment(&book, i, &key);
            }
        }
        let release = lock(&self.session).as_mut()?.release_behind(index);
        if let Some(before) = release {
            lock(&self.player).release_before(before);
        }
        Some(index)
    }

    /// Gives the player the recording's clips near the playhead as the feeder decodes them, and
    /// asks the aligner (when there is one) for their exact word times.
    fn provide_recording(&self, index: usize) -> Option<usize> {
        let pieces = lock(&self.session).as_mut()?.feed(index);
        self.align_recording_clips();
        if !pieces.is_empty() {
            let mut player = lock(&self.player);
            for piece in pieces {
                player.append(
                    piece.index,
                    &Pcm {
                        sample_rate: OUTPUT_RATE,
                        samples: piece.samples,
                    },
                );
                if piece.last {
                    player.finish(piece.index);
                }
            }
        }
        let release = lock(&self.session).as_mut()?.release_behind(index);
        if let Some(before) = release {
            lock(&self.player).release_before(before);
        }
        Some(index)
    }

    pub(crate) fn player_state(&self) -> PlayerStateView {
        let (position, speed, volume) = {
            let player = lock(&self.player);
            (player.position(), player.speed(), player.volume())
        };
        let session = lock(&self.session);
        let status = match position.status {
            Status::Stopped | Status::Finished => PlaybackStatus::Stopped,
            Status::Paused => PlaybackStatus::Paused,
            Status::Playing => PlaybackStatus::Playing,
            Status::Buffering => PlaybackStatus::Buffering,
        };
        let Some(session) = session.as_ref() else {
            return PlayerStateView {
                book_id: None,
                status: PlaybackStatus::Stopped,
                chapter: 0,
                segment_id: None,
                word_index: None,
                speed,
                volume,
                chapter_fraction: 0.0,
                error: lock(&self.player_error).clone(),
                sleep: None,
                source: Narration::Speech,
                off_text: false,
                chapter_left_ms: None,
                waiting_for_sync: false,
            };
        };
        let book = &session.target.book;
        let index = position.index.min(book.total_entries().saturating_sub(1));
        let (chapter, _) = book.locate(index).unwrap_or((0, 0));
        let segment = book.segment(index);
        let latency = (*lock(&self.output_latency_ms) as f32 * speed) as u32;
        let heard_at = position.offset_ms.saturating_sub(latency);
        let off_text = session
            .recording()
            .is_some_and(|plan| plan.off_text(index, heard_at));
        let word = (position.status == Status::Playing && !off_text)
            .then(|| session.word_at(index, heard_at))
            .flatten();
        PlayerStateView {
            book_id: Some(book.id.clone()),
            status,
            chapter,
            segment_id: segment.map(|s| s.id.clone()),
            word_index: word,
            speed,
            volume,
            chapter_fraction: book.chapter_fraction(index),
            error: lock(&self.player_error).clone(),
            sleep: self.sleep_view(),
            source: if session.recording().is_some() {
                Narration::Recording
            } else {
                Narration::Speech
            },
            off_text,
            chapter_left_ms: session.recording().and_then(|_| {
                let entries = book
                    .chapters
                    .get(chapter)
                    .map_or(index..index, |c| index..c.first_entry + c.segments.len());
                let items = session.target.plan.items.get(entries).unwrap_or_default();
                // Not known while part of the chapter waits for the sync.
                if items.iter().any(|item| item.speakable && !item.cached) {
                    return None;
                }
                let audio: u64 = items.iter().map(|item| u64::from(item.duration_ms)).sum();
                let left = audio.saturating_sub(u64::from(position.offset_ms));
                Some((left as f64 / f64::from(speed.max(0.1))) as u64)
            }),
            waiting_for_sync: session.recording().is_some()
                && session
                    .target
                    .plan
                    .items
                    .get(index)
                    .is_some_and(|item| item.speakable && !item.cached),
        }
    }

    async fn engine_loop(self: Arc<Self>) {
        let mut launched_once = false;
        loop {
            let busy = lock(&self.generation).generating_segment_id.is_some();
            if !busy {
                let (engine, name, launch) = {
                    let state = lock(&self.engine);
                    (
                        Arc::clone(&state.engine),
                        state.profile.name.clone(),
                        state.profile.launch.clone(),
                    )
                };
                let status = engine.status().await;
                if status == EngineStatus::Offline && !launched_once {
                    if let Some(launch) = launch {
                        launched_once = true;
                        self.launch_engine(&launch);
                    }
                }
                if let EngineStatus::Ready(model) = &status {
                    {
                        let mut state = lock(&self.engine);
                        if state.model.is_none() {
                            state.model = model.clone();
                        }
                    }
                    if lock(&self.engine).voices == VoiceList::Unchecked {
                        let _ = self.check_voices(true).await;
                        if lock(&self.engine).voices.offered().is_some() {
                            self.replan_with_voices().await;
                        }
                    }
                }
                let (state, detail) = match status {
                    EngineStatus::Offline => (EngineState::Offline, None),
                    EngineStatus::Loading => (EngineState::Loading, None),
                    EngineStatus::Ready(model) => (EngineState::Ready, model),
                    EngineStatus::Error(message) => (EngineState::Error, Some(message)),
                };
                self.emit_engine_status(EngineStatusView {
                    profile_name: name,
                    state,
                    detail,
                });
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    fn launch_engine(&self, launch: &LaunchCommand) {
        let mut command = std::process::Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = &launch.working_dir {
            command.current_dir(dir);
        }
        match command.spawn() {
            Ok(child) => lock(&self.engine).launched = Some(child),
            Err(error) => eprintln!("aoide: could not launch engine: {error}"),
        }
    }

    fn emit_engine_status(&self, status: EngineStatusView) {
        let mut last = lock(&self.last_engine_status);
        if last.as_ref() != Some(&status) {
            self.emit(CoreEvent::EngineStatus(status.clone()));
            *last = Some(status);
        }
    }

    pub fn engine_status(&self) -> EngineStatusView {
        lock(&self.last_engine_status)
            .clone()
            .unwrap_or_else(|| EngineStatusView {
                profile_name: lock(&self.engine).profile.name.clone(),
                state: EngineState::Offline,
                detail: Some("checking…".into()),
            })
    }

    #[cfg(debug_assertions)]
    pub(crate) fn generation_state(&self) -> GenerationStateView {
        lock(&self.generation).clone()
    }

    /// Stops processes Aoide started and saves progress.
    pub fn shutdown(&self) {
        self.save_progress(true);
        *lock(&self.output) = None;
        if let Some(mut child) = lock(&self.engine).launched.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stop_aligner();
    }
}
