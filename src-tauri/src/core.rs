//! Application core: library, playback session, generation and engine glue.
//!
//! Locks are `std::sync::Mutex` and are never held across `.await`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aoide_audio::output::{OutputCommand, PipeOutput};
use aoide_audio::{codec, PlayerCore};
use aoide_generation::{estimate_ms, Job, PlanItem, WorkerEvent};
use aoide_library::{
    content_hash, BookId, BookKey, BookStatus, CachedSegment, Library, NewBook, Progress,
};
use aoide_tts::{
    cache_key, EngineProfile, LaunchCommand, NetworkScope, Params, Registry, SpeechEngine,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Spoken by voice previews: original text, long enough to hear the voice's pace and tone.
mod align;
mod audiobooks;
mod config;
pub use config::PreferencesView;
mod discovery;
mod engine;
mod error;
mod generation;
mod shelf;
use generation::PlanEvent;
mod storage;
pub use audiobooks::{AudiobookChoice, AudiobookMatch, PairOutcome};
pub use discovery::{Candidate, CandidateStatus, ImportChoice};
pub use storage::{CleanResult, StorageReport};
mod bookmarks;
mod export;
mod feeder;
mod listening;
mod metadata;
mod narration;
mod playback;
mod preparation;
mod read_along;
mod recording;
mod recordings;
mod settings;
mod syncing;
pub use recording::{
    app_transcribers, InspectionView, Narration, RecognitionOptions, RecordingView, SyncAction,
    SyncReportView, TranscriberFactory,
};
pub use syncing::Stage as SyncStage;
mod session;
mod voicing;
mod wire;
pub use wire::*;

impl shelf::ShelfHost for Core {
    fn reads_notes(&self, id: &str) -> bool {
        let settings = self.book_settings(id);
        // A recording is laid onto the book's own text, without footnotes read in between.
        settings.read_notes && !self.plays_recording(id, &settings)
    }

    fn playing(&self) -> Option<String> {
        self.playback.book().map(|book| book.id.clone())
    }

    fn prepare_of(&self, id: &str) -> Option<PrepareView> {
        self.prepare_view(id)
    }

    fn recording_of(&self, id: &str) -> Option<SyncStage> {
        self.recording_stage(id)
    }
}

impl generation::GenerationHost for Core {
    fn with_listening(&self, f: &mut dyn FnMut(Option<generation::Listened<'_>>)) {
        let playhead = self.playback.position().index;
        let mut called = false;
        self.playback.with_narration(|book, narration| {
            called = true;
            f(Some(generation::Listened {
                book,
                narration,
                playhead,
            }))
        });
        if !called {
            f(None);
        }
    }

    fn book_plan(&self, id: &str, chapters: Option<&[usize]>) -> Option<Result<Plan, String>> {
        if self.engine.default_voice().is_none() && self.book_settings(id).voice_id.is_none() {
            return None;
        }
        Some(self.load_book(id).and_then(|book| {
            let mut plan = self.speech_plan(&book)?;
            if let Some(chapters) = chapters {
                plan.only(|index| {
                    book.locate(index)
                        .is_some_and(|(chapter, _)| chapters.contains(&chapter))
                });
            }
            Ok(plan)
        }))
    }
}

#[cfg(test)]
mod contract;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod generation_tests;
#[cfg(test)]
mod recording_tests;
#[cfg(test)]
mod tests;
#[cfg(debug_assertions)]
pub use error::CoreError;
pub use listening::ListeningStatsView;
use preparation::Plan;
pub use preparation::{PrepareView, Stage as PrepareStage};
pub use settings::{AlignerSettings, AppSettings, AppSettingsPatch};

use crate::book::{BookModel, ChapterSummary, ChapterView, ExcludedView, SearchHit};

const OUTPUT_RATE: u32 = 48_000;
const TICK: Duration = Duration::from_millis(33);

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

/// A book and how each of its entries is narrated.
struct Target {
    book: Arc<BookModel>,
    narration: narration::NarrationPlan,
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
    /// Tells the user about failures Aoide carried on from.
    reporter: error::Reporter,
    audio: Vec<OutputCommand>,
    show_in_folder: ShowInFolder,
    pub paths: Paths,
    library: Arc<Mutex<Library>>,
    shelf: shelf::Shelf,
    playback: playback::Playback,
    output: Mutex<Option<PipeOutput>>,
    /// The speech engine in use.
    engine: engine::Engine,
    /// What the speech engine is asked to make.
    generation: generation::Generation,
    last_player_state: Mutex<Option<PlayerStateView>>,
    sleep: Mutex<Option<SleepTimer>>,
    media: Mutex<Option<std::sync::mpsc::Sender<crate::media::MediaUpdate>>>,
    extras: Mutex<listening::ExtrasState>,
    /// Exact word timings being asked for. Never held with the playback's locks.
    aligning: Mutex<align::Aligning>,
    config: Mutex<config::Config>,
    /// Recordings and their sync.
    recordings: recordings::Recordings,
    /// What "Find audiobooks" found.
    audiobooks: Mutex<audiobooks::Matches>,
}

#[derive(Debug, Clone, Copy)]
enum SleepTimer {
    At(Instant),
    EndOfChapter(usize),
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

/// Made speech, read from the library's audio files.
struct LibraryAudio<'a>(&'a Core);

impl playback::SpeechAudio for LibraryAudio<'_> {
    fn read(&self, key: &str) -> Result<Option<playback::MadeSpeech>, String> {
        let segment = lock(&self.0.library)
            .segment(key)
            .map_err(|e| e.to_string())?;
        let Some(segment) = segment else {
            return Ok(None);
        };
        let bytes =
            std::fs::read(self.0.paths.audio_dir.join(&segment.file)).map_err(|e| e.to_string())?;
        Ok(Some(playback::MadeSpeech {
            pcm: codec::decode(&bytes).map_err(|e| e.to_string())?,
            timings: segment.word_timings,
        }))
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
            tracing::warn!(event = "config.problem", error = %problem, "Problem in the settings");
        }
        let profile = config
            .get::<EngineProfile>(config::ENGINE)
            .unwrap_or_else(default_profile);
        let settings = config.get::<config::PlayerSettings>(config::PLAYER);
        let engine = registry.create(&profile).map_err(|e| e.to_string())?;
        let library = Arc::new(Mutex::new(library));
        let reporter = error::Reporter::new(Arc::clone(&events));
        let (generation, worker_events) = generation::Generation::new(
            Arc::clone(&library),
            paths.audio_dir.clone(),
            Arc::clone(&engine),
            reporter.clone(),
            Arc::clone(&events),
        );
        let mut player = PlayerCore::new(OUTPUT_RATE);
        if let Some(settings) = settings {
            player.set_speed(settings.speed);
            player.set_volume(settings.volume);
        }
        let recordings = recordings::Recordings::new(
            Arc::clone(&library),
            transcribers,
            paths.models_dir.clone(),
        );
        let engine_component =
            engine::Engine::new(registry, profile, Arc::clone(&engine), Arc::clone(&events));
        let shelf = shelf::Shelf::new(
            Arc::clone(&library),
            paths.library_dir.clone(),
            paths.covers_dir.clone(),
            reporter.clone(),
            Arc::clone(&events),
        );
        let core = Arc::new(Self {
            events,
            reporter,
            audio,
            show_in_folder,
            paths,
            library,
            shelf,
            playback: playback::Playback::new(player),
            output: Mutex::new(None),
            engine: engine_component,
            generation,
            last_player_state: Mutex::new(None),
            sleep: Mutex::new(None),
            media: Mutex::new(None),
            extras: Mutex::new(Default::default()),
            aligning: Mutex::new(Default::default()),
            config: Mutex::new(config),
            recordings,
            audiobooks: Mutex::new(Default::default()),
        });
        core.recordings
            .load_recordings(core.app_settings().recognition.resume_on_start);
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
                    tracing::warn!(event = "discovery.scan", outcome = "failed", %error, "Startup scan for books failed");
                }
            });
        }
        tokio::spawn(Arc::clone(&core).worker_events(worker_events));
        tokio::spawn(Arc::clone(&core).tick_loop());
        tokio::spawn(Arc::clone(&core).engine_loop());
        tokio::spawn(Arc::clone(&core).aligner_loop());
        let scanner = Arc::clone(&core);
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            if let Err(error) = scanner.rescan() {
                tracing::error!(event = "library.scan", outcome = "failed", %error, "First library scan failed");
            }
            // Resume books left queued for preparation in an earlier run, unless turned off.
            if scanner.app_settings().resume_prepare {
                let resumer = Arc::clone(&scanner);
                runtime.spawn(async move {
                    // Voices first, so a voice chosen with another engine is not sent.
                    let _ = resumer.check_voices(false).await;
                    resumer.plan(PlanEvent::ListeningChanged { jump: false });
                });
            }
            if background {
                scanner.shelf.watch();
            }
        });
        if background {
            let watcher = Arc::clone(&core);
            let runtime = tokio::runtime::Handle::current();
            std::thread::spawn(move || watcher.watch_config(runtime));
        }
        Ok(core)
    }

    /// Logs a failure Aoide carried on from and tells the user; see [`error::Reporter`].
    fn report(&self, event: &'static str, doing: &str, error: impl Into<CoreError>) {
        self.reporter.report(event, doing, error);
    }

    fn emit(&self, event: CoreEvent) {
        let name = event.name();
        match event.payload() {
            Ok(value) => self.events.emit(name, value),
            Err(error) => {
                tracing::error!(event = "app.emit", name, %error, "Cannot send an event to the window")
            }
        }
    }

    pub fn app_info(&self) -> AppInfo {
        AppInfo {
            library_folder: self.paths.library_dir.to_string_lossy().into_owned(),
        }
    }

    // ------------------------------------------------------------ library

    /// Scans the library folder, importing new books and marking vanished ones missing.
    pub fn rescan(&self) -> Result<(), String> {
        self.shelf.rescan()
    }

    pub fn import_files(&self, paths: Vec<String>) -> Vec<ImportResult> {
        self.shelf.import_files(paths)
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
        if self.playback.is_book(id) {
            self.stop();
        }
        self.plan(PlanEvent::BookForgotten(id.into()));
        self.forget_recording(id);
        for file in self.shelf.remove(id)? {
            let _ = std::fs::remove_file(self.paths.audio_dir.join(file));
        }
        let _ = std::fs::remove_dir(self.paths.audio_dir.join(id));
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub fn list_books(&self) -> Result<Vec<BookSummary>, String> {
        self.shelf.list(self)
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
        self.shelf.load(id, self)
    }

    fn language_warning(&self, language: Option<&str>) -> Option<String> {
        self.engine.language_warning(language)
    }

    fn voice_warning(&self, book: &BookModel) -> Option<String> {
        self.engine.voice_warning(&self.book_settings(&book.id))
    }

    pub fn open_book(&self, id: &str) -> Result<BookView, String> {
        let view = self.book_overview(id)?;
        if let Err(error) = lock(&self.library).set_last_opened(&BookId(id.into()), now_ms()) {
            self.report(
                "library.save",
                "Could not save when the book was opened",
                error,
            );
        }
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
            progress: self.shelf.progress(&BookId(id.into())),
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

    async fn ensure_voice(&self) -> Result<String, String> {
        self.engine.ensure_voice().await
    }

    async fn check_voices(&self, ready: bool) -> Result<(), String> {
        self.engine.check_voices(ready).await
    }

    async fn ensure_model(&self) {
        self.engine.ensure_model().await
    }

    fn ensure_output(&self) -> Result<(), String> {
        let mut output = lock(&self.output);
        if output.as_ref().is_some_and(PipeOutput::is_running) {
            return Ok(());
        }
        let started = PipeOutput::start(Arc::clone(self.playback.player()), &self.audio)
            .map_err(|e| {
                tracing::error!(event = "output.start", outcome = "failed", error = %e, "Cannot start the audio output");
                e.to_string()
            })?;
        tracing::info!(
            event = "output.start",
            outcome = "ok",
            latency_ms = started.latency_ms(),
            "Started the audio output"
        );
        self.playback.set_latency(started.latency_ms());
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
        let same_book = self.playback.is_book(book_id);
        let start = segment_id
            .as_deref()
            .and_then(|id| book.index_of(id))
            .or_else(|| chapter.and_then(|c| book.chapter_start(c)));
        if same_book {
            if let Some(index) = start {
                self.seek(index);
            }
            self.playback.play();
            return self.ensure_output();
        }
        let settings = self.book_settings(book_id);
        let recording = self.plays_recording(book_id, &settings);
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
        if recording && settings.speak_missing {
            // The speech engine speaks what the recording lacks; without it, that is passed over.
            if let Err(error) = self.ensure_voice().await {
                tracing::warn!(event = "engine.voice", %error, "No voice for what the recording lacks");
            }
            self.ensure_model().await;
        }
        let recorded = if recording {
            self.build_recording_target(Arc::clone(&book), &settings)
                .map_err(|error| {
                    tracing::warn!(event = "narration.fallback", %error, "Reading with the speech engine instead of the recording")
                })
                .ok()
        } else {
            None
        };
        let target = match recorded {
            Some(target) => target,
            None => {
                self.ensure_voice().await?;
                self.ensure_model().await;
                let plan = self.speech_plan(&book)?;
                Target {
                    narration: narration::NarrationPlan::speech(&book, plan),
                    book: Arc::clone(&book),
                }
            }
        };
        self.playback.start(target, start);
        self.plan(PlanEvent::ListeningChanged { jump: true });
        self.ensure_output()
    }

    // ------------------------------------------------------------ settings

    pub fn book_settings(&self, id: &str) -> BookSettings {
        let stored = lock(&self.library).book_value(&BookId(id.into()), BookKey::Settings);
        stored
            .unwrap_or_else(|error| {
                self.report(
                    "book.read",
                    "Could not read the book's voice settings",
                    error,
                );
                None
            })
            .unwrap_or_default()
    }

    pub async fn save_book_settings(&self, id: &str, settings: BookSettings) -> Result<(), String> {
        self.engine.check_params(&settings.params)?;
        lock(&self.library)
            .set_book_value(&BookId(id.into()), BookKey::Settings, &settings)
            .map_err(|e| e.to_string())?;
        // Footnote narration changes segmentation: reparse.
        self.shelf.unload(id);
        // Whether its recording narrates it may have changed.
        self.emit_recording(id);
        if self.playback.is_book(id) {
            self.restart_session().await?;
        }
        self.plan(PlanEvent::BookSettingsChanged(id.into()));
        Ok(())
    }

    pub fn engine_options(&self) -> EngineOptions {
        self.engine.options()
    }

    pub async fn engine_voices(&self) -> Result<Vec<aoide_tts::Voice>, String> {
        self.engine.voices().await
    }

    /// Speaks a short sample sentence; see [`engine::Engine::preview`].
    pub async fn preview_voice(
        &self,
        profile: EngineProfile,
        voice_id: Option<String>,
        params: Params,
        language: Option<String>,
    ) -> Result<Vec<u8>, String> {
        self.engine
            .preview(profile, voice_id, params, language)
            .await
    }

    /// Validates, persists and activates a new engine profile. Must run inside the runtime.
    pub async fn save_engine_profile(&self, profile: EngineProfile) -> Result<(), String> {
        let engine = self.engine.create(&profile)?;
        lock(&self.config).update(|file| file.set_section(config::ENGINE, &profile))?;
        self.activate_profile(profile, engine).await
    }

    /// Switches generation to a validated profile.
    async fn activate_profile(
        &self,
        profile: EngineProfile,
        engine: Arc<dyn SpeechEngine>,
    ) -> Result<(), String> {
        self.engine.switch(profile, Arc::clone(&engine));
        self.plan(PlanEvent::EngineSwitched(engine));
        self.restart_session().await
    }

    /// Rebuilds the listening session's requests and cache state (voice/engine/settings
    /// changed), keeping the playhead.
    async fn restart_session(&self) -> Result<(), String> {
        let Some(book) = self.playback.book() else {
            return Ok(());
        };
        let playing = self.playback.is_playing();
        let segment = book
            .segment(self.playback.position().index)
            .map(|seg| seg.id.clone());
        // The playhead stays where it is, so there is no new place to save.
        let _ = self.playback.stop();
        self.play(&book.id, None, segment).await?;
        if !playing {
            let _ = self.playback.pause();
        }
        Ok(())
    }

    /// Deletes a book's generated audio; it will be regenerated on demand.
    pub async fn regenerate_book(&self, id: &str) -> Result<(), String> {
        let keys = self
            .load_book(id)
            .and_then(|book| self.speech_plan(&book))
            .map(|plan| plan.items.into_iter().map(|item| item.key).collect())
            .unwrap_or_default();
        let files = lock(&self.library)
            .delete_book_segments(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        for file in files {
            let _ = std::fs::remove_file(self.paths.audio_dir.join(file));
        }
        self.plan(PlanEvent::BookAudioDeleted {
            book: id.into(),
            keys,
        });
        if self.playback.is_book(id) {
            self.restart_session().await?;
        }
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    // ------------------------------------------------------------ preparation

    pub async fn prepare_book(&self, id: &str) -> Result<(), String> {
        self.load_book(id)?;
        self.ensure_voice().await?;
        self.ensure_model().await;
        // Plans it now: a book whose audio is all made already finishes here.
        self.plan(PlanEvent::BookQueued(id.into()));
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub fn cancel_prepare(&self, id: &str) {
        self.plan(PlanEvent::PrepareCancelled(id.into()));
        self.emit(CoreEvent::LibraryChanged);
    }

    /// Pauses or resumes a queued book's preparation; the choice survives restarts.
    pub fn set_prepare_paused(&self, id: &str, paused: bool) {
        self.plan(PlanEvent::PreparePaused {
            book: id.into(),
            paused,
        });
        self.emit(CoreEvent::LibraryChanged);
    }

    /// Queues one chapter of a book for preparation (joining the book's entry if it has one).
    pub async fn prepare_chapter(&self, id: &str, chapter: usize) -> Result<(), String> {
        let book = self.load_book(id)?;
        if chapter >= book.chapters.len() {
            return Err("no such chapter".into());
        }
        self.ensure_voice().await?;
        self.ensure_model().await;
        self.plan(PlanEvent::ChapterQueued {
            book: id.into(),
            chapter,
        });
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// How much of each chapter's audio is made, in the voice and settings it would be read in.
    pub async fn chapter_audio(&self, id: &str) -> Result<Vec<ChapterAudioView>, String> {
        let book = self.load_book(id)?;
        let _ = self.ensure_voice().await; // Without a voice nothing can be cached yet.
        let plan = self.speech_plan(&book)?;
        let mut chapters = vec![ChapterAudioView { done: 0, total: 0 }; book.chapters.len()];
        for (index, item) in plan.items.iter().enumerate() {
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
        self.generation.prepare_view(id)
    }

    /// The speech engine's plan for reading all of `book` in its voice and settings.
    fn speech_plan(&self, book: &BookModel) -> Result<Plan, String> {
        let settings = self.book_settings(&book.id);
        let engine::Speech {
            voicing,
            descriptor,
            model,
        } = self.engine.speech(&settings, book.language.as_deref())?;
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
        Ok(Plan { items, requests })
    }

    fn seek(&self, index: usize) {
        self.seek_at(index, None);
    }

    /// Moves the playhead `offset_ms` into entry `index`; `None` starts where the entry's text
    /// does (for a recording, just before its first word).
    fn seek_at(&self, index: usize, offset_ms: Option<u32>) {
        self.save_place(self.playback.seek(index, offset_ms));
        self.plan(PlanEvent::ListeningChanged { jump: true });
    }

    /// Tells generation what changed; see [`generation::Generation::apply`].
    fn plan(&self, event: PlanEvent) {
        self.generation.apply(event, self);
    }

    pub fn pause(&self) {
        self.save_place(self.playback.pause());
    }

    pub fn resume(&self) -> Result<(), String> {
        self.playback.play();
        self.ensure_output()
    }

    pub fn stop(&self) {
        self.save_place(self.playback.stop());
        *lock(&self.output) = None;
        self.plan(PlanEvent::ListeningStopped);
    }

    pub fn toggle(&self) {
        if self.playback.is_playing() {
            self.pause();
        } else {
            let _ = self.resume();
        }
    }

    /// Plays from the start of the chapter `delta` chapters away from the current one.
    pub async fn skip_chapter(&self, delta: i64) -> Result<(), String> {
        let book = self.playback.book().ok_or("nothing is playing")?;
        let (chapter, _) = book
            .locate(self.playback.position().index)
            .unwrap_or((0, 0));
        let last = book.chapters.len().saturating_sub(1) as i64;
        let target = (chapter as i64 + delta).clamp(0, last) as usize;
        self.play(&book.id, Some(target), None).await
    }

    /// `minutes`: pause after that long; `end_of_chapter`: pause when the chapter ends.
    /// Neither cancels the timer.
    pub fn set_sleep_timer(&self, minutes: Option<u32>, end_of_chapter: bool) {
        let timer = if end_of_chapter {
            self.playback.book().and_then(|book| {
                book.locate(self.playback.position().index)
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
        let Some((id, title, authors, chapter_title)) = self.playback.book().map(|book| {
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
        self.playback.set_speed(speed);
        self.save_player_settings();
    }

    pub fn set_volume(&self, volume: f32) {
        self.playback.set_volume(volume);
        self.save_player_settings();
    }

    fn save_player_settings(&self) {
        let settings = config::PlayerSettings {
            speed: self.playback.speed(),
            volume: self.playback.volume(),
        };
        let saved = lock(&self.config).update(|file| file.set_section(config::PLAYER, &settings));
        if let Err(error) = saved {
            self.report("config.save", "Could not save the player settings", error);
        }
    }

    /// Saves the listener's place; the one way it is saved.
    fn save_place(&self, place: Option<playback::Place>) {
        let Some(playback::Place { book, index }) = place else {
            return;
        };
        let Some((chapter, local)) = book.locate(index) else {
            return;
        };
        let segment = &book.chapters[chapter].segments[local];
        let id = BookId(book.id.clone());
        let saved = {
            let library = lock(&self.library);
            library
                .save_progress(
                    &id,
                    &Progress {
                        chapter: chapter as u32,
                        char_offset: segment.range.start as u32,
                        segment_id: Some(segment.id.clone()),
                        updated_at: now_ms(),
                    },
                )
                .and_then(|()| {
                    library.set_book_value(&id, BookKey::ProgressFraction, &book.fraction(index))
                })
        };
        if let Err(error) = saved {
            self.report("progress.save", "Could not save your place", error);
        }
    }

    // ------------------------------------------------------------ background loops

    async fn worker_events(
        self: Arc<Self>,
        mut events: tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
    ) {
        while let Some(event) = events.recv().await {
            match event {
                WorkerEvent::Started(job) => {
                    self.engine.mark_busy();
                    let segment_id = self.segment_id_for(&job);
                    self.plan(PlanEvent::WorkStarted { segment_id });
                }
                WorkerEvent::Finished { job, duration_ms } => {
                    self.playback.segment_made(&job.key, duration_ms);
                    self.plan(PlanEvent::SegmentMade {
                        key: job.key,
                        duration_ms,
                    });
                    self.enforce_cache_limit();
                }
                WorkerEvent::Failed { job, error } => {
                    tracing::warn!(event = "gen.failed", key = %job.key, %error, "Making audio failed");
                    self.plan(PlanEvent::WorkFailed { error });
                }
            }
        }
    }

    fn segment_id_for(&self, job: &Job) -> Option<String> {
        if let Some(book) = self.playback.book().filter(|b| b.id == job.book) {
            return book.segment(job.index).map(|s| s.id.clone());
        }
        self.load_book(&job.book)
            .ok()?
            .segment(job.index)
            .map(|s| s.id.clone())
    }

    async fn tick_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            if self.provide_audio() {
                self.plan(PlanEvent::ListeningChanged { jump: false });
                self.save_place(self.playback.place_to_save(false));
            }
            self.keep_output_alive();
            let state = self.player_state();
            self.check_sleep(state.chapter);
            self.track_listening(&state);
            let mut last = lock(&self.last_player_state);
            let previous = last.as_ref();
            if previous.map(|p| p.status) != Some(state.status) {
                tracing::info!(
                    event = "player.status",
                    book = state.book_id.as_deref().map(crate::logging::book),
                    from = crate::logging::name(previous.map(|p| p.status)),
                    to = crate::logging::name(state.status),
                    chapter = state.chapter,
                    source = crate::logging::name(state.source),
                    error = state.error.as_deref(),
                    "Player status changed"
                );
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
        let playing = self.playback.is_playing();
        let dead = lock(&self.output)
            .as_ref()
            .is_some_and(|output| !output.is_running());
        if playing && dead {
            tracing::warn!(
                event = "output.restart",
                "The audio output stopped during playback; restarting it"
            );
            if let Err(error) = self.ensure_output() {
                self.playback.fail(format!("Audio output failed: {error}"));
            }
        }
    }

    /// Gives the player the audio near the playhead it does not have yet, and asks for exact
    /// word times for it. Returns whether the playhead moved to another entry.
    fn provide_audio(&self) -> bool {
        let Some(tick) = self.playback.tick(&LibraryAudio(self)) else {
            return false;
        };
        self.align(tick.to_align);
        tick.moved
    }

    pub(crate) fn player_state(&self) -> PlayerStateView {
        PlayerStateView {
            sleep: self.sleep_view(),
            ..self.playback.state()
        }
    }

    async fn engine_loop(self: Arc<Self>) {
        loop {
            let busy = self.generation.busy();
            if self.engine.poll(busy).await == engine::Polled::VoicesKnown {
                // Plans made before the engine listed its voices (it was down) may name a
                // voice it does not offer: make them again now that the list is known.
                if let Err(error) = self.restart_session().await {
                    tracing::warn!(event = "engine.session", %error, "Could not restart playback with the engine's voices");
                }
                self.plan(PlanEvent::VoicesKnown);
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }

    pub fn engine_status(&self) -> EngineStatusView {
        self.engine.status()
    }

    #[cfg(debug_assertions)]
    pub(crate) fn generation_state(&self) -> GenerationStateView {
        self.generation.state()
    }

    /// Stops processes Aoide started and saves progress.
    pub fn shutdown(&self) {
        self.save_place(self.playback.place_to_save(true));
        *lock(&self.output) = None;
        self.engine.shutdown();
        self.stop_aligner();
    }
}
