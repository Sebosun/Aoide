//! Application core: builds the components (each its own module under `core/`, owning its
//! state and locks), runs the background loops, implements the components' host traits and
//! forwards commands. See `docs/core-refactor.md`, section Component.
//!
//! Locks are `std::sync::Mutex`, taken through [`lock`], and never held across `.await`.
//! `library` is the leaf lock: taken last and released before any call out of the component
//! holding it. A component's own locks are released before it calls its host.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aoide_audio::{codec, PlayerCore};
use aoide_generation::WorkerEvent;
use aoide_library::{content_hash, BookId, BookStatus, CachedSegment, Library, NewBook};
use aoide_tts::{EngineProfile, LaunchCommand, NetworkScope};
use serde::Serialize;

/// Spoken by voice previews: original text, long enough to hear the voice's pace and tone.
mod align;
mod articles;
mod audiobook;
mod audiobooks;
mod browser_download;
mod browsing;
mod config;
mod controls;
pub use config::PreferencesView;
mod discovery;
mod engine;
mod environment;
mod output;
use environment::{default_profile, OUTPUT_RATE};
pub use environment::{Environment, Events, OpenLink, Paths, ShowInFolder};
mod error;
mod facets;
mod feeds;
mod fetching;
pub use articles::{ArticleSource, ArticleSummary, ArticleView};
pub use facets::HomeView;
pub use feeds::{
    AddedLink, EntryFilter, EntryQuery, EntryView, FeedView, OpmlImport, SubscribeResult,
};
mod generation;
mod shelf;
use generation::PlanEvent;
mod storage;
pub use audiobooks::{AudiobookChoice, AudiobookMatch, AudiobookSuggestion, PairOutcome};
#[cfg(any(test, debug_assertions))]
pub use discovery::CandidateStatus;
pub use discovery::{Candidate, ImportChoice};
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
mod reading;
pub use reading::{ReadState, ReadingMode, Shape};
mod recording;
mod recordings;
mod settings;
mod syncing;
pub use recording::{
    InspectionView, Narration, RecognitionOptions, RecordingView, SyncAction, SyncReportView,
};
#[cfg(any(test, debug_assertions))]
pub use syncing::Stage as SyncStage;
mod session;
mod timeline;
mod voicing;
mod wire;
pub use wire::*;

#[cfg(test)]
mod article_tests;
#[cfg(test)]
mod browser_tests;
#[cfg(test)]
mod contract;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod feeds_tests;
#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod generation_tests;
#[cfg(test)]
mod recording_tests;
#[cfg(test)]
mod tests;
pub use error::CoreError;
use preparation::Plan;
pub use preparation::{PrepareView, Stage as PrepareStage};
pub use settings::{AlignerSettings, AppSettings, AppSettingsPatch};

use crate::book::{BookModel, ChapterSummary, ExcludedView};

const TICK: Duration = Duration::from_millis(33);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Tells the user of a poisoned lock; set once, by the first core started.
static POISON_REPORTER: std::sync::OnceLock<error::Reporter> = std::sync::OnceLock::new();
static POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Locks `mutex`, carrying on with its data when a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| recover(poisoned, &POISONED, POISON_REPORTER.get()))
}

/// Carries on with a poisoned lock's data; the first time (`told` unset), says so.
fn recover<G>(
    poisoned: std::sync::PoisonError<G>,
    told: &std::sync::atomic::AtomicBool,
    reporter: Option<&error::Reporter>,
) -> G {
    if !told.swap(true, std::sync::atomic::Ordering::Relaxed) {
        let error = format!(
            "a lock on {} was poisoned by a panic",
            std::any::type_name::<G>()
        );
        match reporter {
            Some(reporter) => reporter.report(
                "app.poisoned",
                "Something failed and Aoide carried on",
                error,
            ),
            None => {
                tracing::error!(event = "app.poisoned", %error, "Carrying on past a poisoned lock")
            }
        }
    }
    poisoned.into_inner()
}

// ---------------------------------------------------------------- core

/// A book and how each of its entries is narrated.
struct Target {
    book: Arc<BookModel>,
    narration: narration::NarrationPlan,
}

pub struct Core {
    events: Arc<dyn Events>,
    /// Tells the user about failures Aoide carried on from.
    reporter: error::Reporter,
    show_in_folder: ShowInFolder,
    open_link: OpenLink,
    pub paths: Paths,
    library: Arc<Mutex<Library>>,
    shelf: Arc<shelf::Shelf>,
    discovery: Arc<discovery::Discovery>,
    bookmarks: bookmarks::Bookmarks,
    listening: listening::Listening,
    reading: reading::Reading,
    metadata: metadata::Metadata,
    storage: storage::Storage,
    exports: export::Exports,
    playback: playback::Playback,
    output: output::Output,
    /// The speech engine in use.
    engine: engine::Engine,
    /// What the speech engine is asked to make.
    generation: generation::Generation,
    /// The sleep timer, the media keys and the player state last sent.
    controls: controls::Controls,
    /// Exact word timings being asked for.
    aligner: align::Aligner,
    settings: config::Settings,
    /// Recordings and their sync.
    recordings: recordings::Recordings,
    /// What "Find audiobooks" found.
    audiobooks: Arc<audiobooks::Finder>,
    /// Articles saved from the web and their fetches.
    articles: Arc<articles::Articles>,
    /// Feeds subscribed to and their checks.
    feeds: Arc<feeds::Feeds>,
    /// The optional browser and the site rules.
    browsing: Arc<browsing::Browsing>,
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
            open_link,
            transcribers,
            fetcher,
            browser,
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
        let _ = POISON_REPORTER.set(reporter.clone());
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
        let bookmarks = bookmarks::Bookmarks::new(Arc::clone(&library));
        let listening = listening::Listening::new(Arc::clone(&library), reporter.clone());
        let reading = reading::Reading::new(Arc::clone(&library), reporter.clone());
        let storage = storage::Storage::new(Arc::clone(&library), &paths);
        let audiobooks = Arc::new(audiobooks::Finder::new(
            Arc::clone(&library),
            &paths,
            Arc::clone(&events),
        ));
        let discovery = Arc::new(discovery::Discovery::new(
            Arc::clone(&library),
            &paths,
            Arc::clone(&events),
            reporter.clone(),
        ));
        let aligner = align::Aligner::new(Arc::clone(&library), paths.audio_dir.clone());
        let exports = export::Exports::new(Arc::clone(&library), &paths, Arc::clone(&events));
        let metadata = metadata::Metadata::new(
            Arc::clone(&library),
            paths.covers_dir.clone(),
            reporter.clone(),
        );
        let articles = Arc::new(articles::Articles::new(
            Arc::clone(&library),
            paths.articles_dir.clone(),
            paths.covers_dir.clone(),
            Arc::clone(&fetcher),
        ));
        let feeds = Arc::new(feeds::Feeds::new(Arc::clone(&library), fetcher));
        let browsing = Arc::new(browsing::Browsing::new(
            browser,
            paths.browser_dir.clone(),
            &paths.config_dir,
        ));
        let core = Arc::new(Self {
            articles,
            feeds,
            browsing,
            events,
            reporter,
            show_in_folder,
            open_link,
            paths,
            library,
            shelf: Arc::new(shelf),
            discovery,
            bookmarks,
            listening,
            reading,
            metadata,
            storage,
            exports,
            playback: playback::Playback::new(player),
            output: output::Output::new(audio),
            engine: engine_component,
            generation,
            controls: Default::default(),
            aligner,
            settings: config::Settings::new(config),
            recordings,
            audiobooks,
        });
        core.recordings
            .load_recordings(core.app_settings().recognition.resume_on_start);
        core.articles.load();
        let fetching = Arc::clone(&core);
        tokio::spawn(async move { fetching.articles.run(&*fetching).await });
        if background {
            let checking = Arc::clone(&core);
            tokio::spawn(async move { checking.feeds.run(&*checking).await });
        }
        let idling = Arc::clone(&core);
        tokio::spawn(async move { idling.browsing.run(&*idling).await });
        if background {
            core.spawn_sync_runner();
        }
        if background {
            core.controls.attach_media(crate::media::spawn(&core));
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
        let aligning = Arc::clone(&core);
        tokio::spawn(async move { aligning.aligner.run(&*aligning).await });
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
            std::thread::spawn(move || watcher.settings.watch(&*watcher, &runtime));
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

    // ------------------------------------------------------------ books

    // ------------------------------------------------------------ playback

    pub async fn play(
        &self,
        book_id: &str,
        chapter: Option<usize>,
        segment_id: Option<String>,
    ) -> Result<(), String> {
        if self.kind_of(book_id) == Some(aoide_library::ItemKind::Audiobook) {
            return self.play_timeline(book_id, chapter);
        }
        let book = self.load_book(book_id)?;
        // Playing an item by any route means listening to it.
        self.choose_mode(book_id, reading::ModeEvent::ListenChosen)?;
        self.engine.want();
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

    // ------------------------------------------------------------ preparation

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
                WorkerEvent::Discarded(_) => self.plan(PlanEvent::WorkDiscarded),
            }
        }
    }

    async fn tick_loop(self: Arc<Self>) {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            if self.provide_audio() {
                self.plan(PlanEvent::ListeningChanged { jump: false });
                self.save_place(self.playback.place_to_save(false));
            }
            self.output.keep_alive(&self.playback);
            let state = self.player_state();
            if self.controls.sleep_due(state.chapter) {
                self.pause();
            }
            self.listening.track(&state);
            let now_playing = |chapter| match self.playback.book() {
                Some(book) => Some(controls::NowPlaying::of(
                    &book,
                    chapter,
                    self.shelf.cover(&book.id),
                )),
                None => {
                    let timeline = self.playback.timeline()?;
                    Some(controls::NowPlaying::of_timeline(
                        &timeline,
                        chapter,
                        self.shelf.cover(&timeline.id),
                    ))
                }
            };
            if let Some(state) = self.controls.observe(state, &now_playing) {
                self.emit(CoreEvent::PlayerState(state));
            }
        }
    }

    /// Gives the player the audio near the playhead it does not have yet, and asks for exact
    /// word times for it. Returns whether the playhead moved to another entry.
    fn provide_audio(&self) -> bool {
        let Some(tick) = self.playback.tick(&playback::LibraryAudio {
            library: &self.library,
            audio_dir: &self.paths.audio_dir,
        }) else {
            return false;
        };
        self.align(tick.to_align);
        tick.moved
    }

    /// Runs the sync job on a thread of its own until Core is dropped.
    fn spawn_sync_runner(self: &Arc<Self>) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.recordings.set_wake(tx);
        let core = Arc::downgrade(self);
        let spawned = std::thread::Builder::new()
            .name("aoide-sync".into())
            .spawn(move || loop {
                let Some(strong) = core.upgrade() else { return };
                let worked = strong.sync_step();
                drop(strong);
                if worked {
                    continue;
                }
                match rx.recv_timeout(Duration::from_secs(60)) {
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(strong) = core.upgrade() {
                            strong.look_for_missing_files();
                        }
                    }
                    Ok(()) => {}
                }
            });
        if let Err(error) = spawned {
            self.report(
                "sync.runner",
                "Could not start syncing recordings",
                CoreError::internal(error),
            );
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

    #[cfg(debug_assertions)]
    pub(crate) fn generation_state(&self) -> GenerationStateView {
        self.generation.state()
    }

    /// Stops processes Aoide started and saves progress.
    pub fn shutdown(&self) {
        self.save_place(self.playback.place_to_save(true));
        self.output.close();
        self.engine.shutdown();
        self.stop_aligner();
        self.browsing.stop();
    }
}
