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
mod backup;
mod browser_download;
mod browsing;
mod config;
mod controls;
pub use config::PreferencesView;
mod details;
mod discovery;
mod engine;
mod engine_process;
mod environment;
mod output;
use environment::{default_profile, OUTPUT_RATE};
pub use environment::{Environment, Events, OpenLink, Paths, ShowInFolder};
mod error;
mod facets;
mod feeds;
mod fetching;
mod fullscreen;
pub use articles::{ArticleSource, ArticleSummary, ArticleView};
pub use browsing::BrowserStatusView;
#[cfg(any(test, debug_assertions))]
pub use browsing::BrowserUse;
pub use details::DetailsPatch;
pub use facets::{HomeView, SeriesShelfView};
pub use feeds::{
    AddedLink, EntryFilter, EntryQuery, EntryView, FeedView, OpmlImport, SubscribeResult,
};
#[cfg(any(test, debug_assertions))]
pub use fetching::Stage as FetchStage;
pub use signin::SignInView;
pub use sites::SiteView;
mod generation;
mod inbox;
pub use inbox::InboxView;
mod item;
mod launching;
mod link_import;
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
mod highlights;
pub use highlights::{HighlightAdded, HighlightView};
mod leaving_out;
mod listening;
mod metadata;
mod narration;
mod notes;
mod notices;
pub use notices::{Notice, Notifier};
mod playback;
mod preparation;
mod presence;
mod read_along;
mod reading;
pub use reading::{ReadState, ReadingMode, Shape};
mod recording;
mod recordings;
mod resuming;
mod settings;
mod signin;
mod sites;
mod syncing;
pub use recording::{
    InspectionView, Narration, RecognitionOptions, RecordingView, SyncAction, SyncReportView,
};
#[cfg(any(test, debug_assertions))]
pub use syncing::Stage as SyncStage;
mod session;
mod speech_problems;
pub use speech_problems::SpeechProblem;
mod tagging;
pub use tagging::{TaggingRun, TaggingSettings};
mod pronunciation;
mod queue;
pub use pronunciation::AffectedView;
pub use queue::{QueuePlace, QueueView};
mod takes;
mod timeline;
mod voicing;
mod watching;
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
mod queue_tests;
#[cfg(test)]
mod recording_tests;
#[cfg(test)]
mod tests;
pub use backup::{
    apply_staged as apply_staged_restore, BackupInfoView, BackupOptions, BackupWritten,
    EXTENSION as BACKUP_EXTENSION,
};
pub use error::CoreError;
pub use link_import::{LinkChoice, LinkImportView};
use preparation::Plan;
pub use preparation::{PrepareView, Stage as PrepareStage};
pub use presence::{Action as WindowAction, CloseBehaviour};
pub use settings::{AlignerSettings, AppSettings, AppSettingsPatch, PauseSettings};

use crate::book::{BookModel, ChapterSummary, ExcludedView};

/// How often the player is looked at while it plays (or just changed).
const TICK: Duration = Duration::from_millis(33);
/// How long after a change the player is still looked at every tick.
const LINGER: Duration = Duration::from_secs(2);
/// How often an idle player is looked at, when nothing tells of a change.
const IDLE_TICK: Duration = Duration::from_secs(1);

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Tells the user of a poisoned lock; set once, by the first core started.
static POISON_REPORTER: std::sync::OnceLock<error::Reporter> = std::sync::OnceLock::new();
static POISONED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Locks `mutex`, carrying on with its data when a thread panicked while holding it.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
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
    highlights: highlights::Highlights,
    pronunciations: pronunciation::Pronunciations,
    listening: listening::Listening,
    reading: reading::Reading,
    metadata: metadata::Metadata,
    storage: storage::Storage,
    exports: export::Exports,
    playback: playback::Playback,
    output: output::Output,
    /// The speech engine in use.
    engine: engine::Engine,
    /// The server Aoide starts for the engine's launch command.
    speech_server: engine_process::ServerProcess,
    /// What the speech engine is asked to make.
    generation: generation::Generation,
    /// What went wrong with the speech engine lately.
    speech_problems: speech_problems::SpeechProblems,
    tagging: tagging::Tagging,
    /// What plays next.
    queue: queue::Queue,
    /// Whether the window fills the screen.
    fullscreen: fullscreen::Fullscreen,
    /// In the window or in the background, and what closing the window does.
    presence: presence::Presences,
    /// Writing backups, and the automatic ones.
    backups: backup::Backups,
    /// Importing links from another app.
    link_imports: link_import::LinkImports,
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
    /// A sign-in in a visible browser window.
    sign_in: signin::SignIn,
    /// Desktop notifications when long jobs end.
    notices: notices::Notices,
    /// What starts of Aoide ask it to open.
    launching: launching::Launching,
    inbox: inbox::Inbox,
}

impl Core {
    /// Must be called inside the async runtime (spawns background tasks).
    pub fn start(environment: Environment, paths: Paths) -> Result<Arc<Self>, String> {
        let background = environment.background;
        let (core, worker_events) = Self::build(environment, paths)?;
        core.recordings
            .load_recordings(core.app_settings().recognition.resume_on_start);
        core.articles.load();
        core.load_link_import();
        core.spawn_tasks(worker_events, background);
        Ok(core)
    }

    /// Opens the library and settings and builds every component, wired to one another.
    fn build(
        environment: Environment,
        paths: Paths,
    ) -> Result<(Arc<Self>, tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>), String> {
        let Environment {
            events,
            engines: registry,
            audio,
            background: _,
            show_in_folder,
            open_link,
            transcribers,
            fetcher,
            browser,
            browser_download,
            notifier,
            clock,
        } = environment;
        for dir in [&paths.library_dir, &paths.covers_dir, &paths.audio_dir] {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        if let Some(parent) = paths.db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let library = Library::open(&paths.db_path).map_err(|e| e.to_string())?;
        let config = config::Config::open(&paths.config_dir, &paths.library_dir, &library);
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
            clock.clone(),
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
            clock.clone(),
        );
        let data_dir = paths.db_path.parent().unwrap_or(Path::new(".")).to_owned();
        let speech_server = engine_process::ServerProcess::new(
            engine_process::Names::SPEECH,
            &data_dir,
            &data_dir.join("logs"),
            engine_process::Spec::of(&profile.launch, &profile.base_url),
            engine_process::Timing::default(),
        );
        let engine_component =
            engine::Engine::new(registry, profile, Arc::clone(&engine), Arc::clone(&events));
        engine_component.set_server(speech_server.view());
        let shelf = shelf::Shelf::new(
            Arc::clone(&library),
            paths.library_dir.clone(),
            paths.covers_dir.clone(),
            reporter.clone(),
            Arc::clone(&events),
        );
        let bookmarks = bookmarks::Bookmarks::new(Arc::clone(&library));
        let highlights = highlights::Highlights::new(Arc::clone(&library));
        let pronunciations = pronunciation::Pronunciations::new(Arc::clone(&library));
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
            clock.clone(),
        ));
        let aligner = align::Aligner::new(
            Arc::clone(&library),
            paths.audio_dir.clone(),
            &data_dir,
            &data_dir.join("logs"),
        );
        let exports = export::Exports::new(
            Arc::clone(&library),
            &paths,
            Arc::clone(&events),
            clock.clone(),
        );
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
            browser_download,
        ));
        let launching =
            launching::Launching::new(Arc::clone(&library), Arc::clone(&events), reporter.clone());
        let inbox = inbox::Inbox::new(Arc::clone(&events));
        let queue = queue::Queue::new(Arc::clone(&library), Arc::clone(&events));
        let core = Arc::new(Self {
            articles,
            feeds,
            browsing,
            sign_in: signin::SignIn::default(),
            events,
            reporter,
            show_in_folder,
            open_link,
            paths,
            library,
            shelf: Arc::new(shelf),
            discovery,
            bookmarks,
            highlights,
            pronunciations,
            listening,
            reading,
            metadata,
            storage,
            exports,
            playback: playback::Playback::new(player),
            output: output::Output::new(audio),
            engine: engine_component,
            speech_server,
            generation,
            speech_problems: Default::default(),
            tagging: Default::default(),
            queue,
            fullscreen: Default::default(),
            presence: Default::default(),
            backups: Default::default(),
            link_imports: Default::default(),
            controls: Default::default(),
            aligner,
            settings: config::Settings::new(config),
            recordings,
            audiobooks,
            notices: notices::Notices::new(notifier),
            launching,
            inbox,
        });
        core.inbox.set_folder(core.app_settings().inbox);
        Ok((core, worker_events))
    }

    /// Starts the work that runs for the life of the app. `background` is off in tests: no
    /// feed checks, sync runner, media keys, startup scan, file watching or settings watching.
    fn spawn_tasks(
        self: &Arc<Self>,
        worker_events: tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
        background: bool,
    ) {
        let core = self;
        let fetching = Arc::clone(core);
        tokio::spawn(async move { fetching.articles.run(&*fetching).await });
        let idling = Arc::clone(core);
        tokio::spawn(async move { idling.browsing.run(&*idling).await });
        tokio::spawn(Arc::clone(core).worker_events(worker_events));
        tokio::spawn(Arc::clone(core).tick_loop());
        tokio::spawn(Arc::clone(core).engine_loop());
        let serving = Arc::clone(core);
        tokio::spawn(async move {
            serving
                .speech_server
                .run(&engine::SpeechServer(&serving))
                .await
        });
        let aligning = Arc::clone(core);
        tokio::spawn(async move { aligning.aligner.run(&*aligning).await });
        let serving = Arc::clone(core);
        tokio::spawn(async move { serving.aligner.serve().await });
        let scanner = Arc::clone(core);
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
        if !background {
            return;
        }
        let checking = Arc::clone(core);
        tokio::spawn(async move { checking.feeds.run(&*checking).await });
        core.spawn_sync_runner();
        core.controls.attach_media(crate::media::spawn(core));
        if core.app_settings().scan.on_startup {
            let scanner = Arc::clone(core);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(8)).await;
                if let Err(error) = scanner.scan_for_books().await {
                    tracing::warn!(event = "discovery.scan", outcome = "failed", %error, "Startup scan for books failed");
                }
            });
        }
        let watcher = Arc::clone(core);
        let runtime = tokio::runtime::Handle::current();
        std::thread::spawn(move || watcher.settings.watch(&*watcher, &runtime));
        let inbox = Arc::clone(core);
        std::thread::spawn(move || inbox.inbox.run(&*inbox));
        // Automatic backups: a minute after start, then each hour (each writes only when due).
        let backing_up = Arc::clone(core);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(60)).await;
            loop {
                let core = Arc::clone(&backing_up);
                let _ = tokio::task::spawn_blocking(move || core.run_automatic_backup()).await;
                tokio::time::sleep(Duration::from_secs(3600)).await;
            }
        });
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

    // ------------------------------------------------------------ playback

    pub async fn play(
        &self,
        book_id: &str,
        chapter: Option<usize>,
        segment_id: Option<String>,
    ) -> Result<(), String> {
        self.start_playing(book_id, chapter, segment_id).await?;
        self.queue.started(book_id);
        self.played();
        Ok(())
    }

    /// [`Core::play`], without telling the sleep timer.
    async fn start_playing(
        &self,
        book_id: &str,
        chapter: Option<usize>,
        segment_id: Option<String>,
    ) -> Result<(), String> {
        if self.item_kind(book_id) == Some(aoide_library::ItemKind::Audiobook) {
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
            let Some(index) = start else {
                return self.resume_playing();
            };
            self.follow_through(self.playback.seek(index, None))?;
            return self.follow_through(self.playback.play());
        }
        // A place asked for or saved is played even in a chapter left out; with none, the book
        // starts at its first chapter narrated.
        let mut explicit = start.is_some();
        // From the saved place, a little earlier the longer ago it was saved.
        let mut away = None;
        let start = start
            .or_else(|| {
                let progress = lock(&self.library)
                    .progress(&BookId(book_id.into()))
                    .ok()
                    .flatten()?;
                away = self
                    .steps_back()
                    .then(|| resuming::Away::since(progress.updated_at, now_ms()));
                let saved = book.index_at(progress.chapter as usize, progress.char_offset as usize);
                explicit = saved.is_some();
                saved
            })
            .or_else(|| book.chapter_start(0))
            .ok_or("the book has no readable text")?;
        let mut target = self.narration_target(book).await?;
        if explicit {
            leaving_out::admit(&mut target, start);
        }
        let start = away
            .and_then(|away| playback::step_back_in(&target, start, away, "saved"))
            .unwrap_or(start);
        self.playback
            .use_own_speed(self.book_settings(book_id).playback_speed);
        self.follow_through(self.playback.start(target, start))
    }

    /// What narrates `book`: its synced recording when it has one and it is chosen (with the
    /// speech engine for what the recording lacks, if asked), else the speech engine.
    async fn narration_target(&self, book: Arc<BookModel>) -> Result<Target, String> {
        let settings = self.book_settings(&book.id);
        let recording = self.plays_recording(&book.id, &settings);
        if recording && settings.speak_missing {
            // The speech engine speaks what the recording lacks; without it, that is passed over.
            if let Err(error) = self.ensure_voice().await {
                tracing::warn!(event = "engine.voice", %error, "No voice for what the recording lacks");
            }
            self.ensure_model().await;
        }
        if let Some(target) = self.recorded_target(&book, &settings, recording) {
            return Ok(target);
        }
        self.ensure_voice().await?;
        self.ensure_model().await;
        self.speech_target(book)
    }

    /// `book` narrated by its synced recording, when `recording` says it is and it can be.
    fn recorded_target(
        &self,
        book: &Arc<BookModel>,
        settings: &BookSettings,
        recording: bool,
    ) -> Option<Target> {
        if !recording {
            return None;
        }
        self.build_recording_target(Arc::clone(book), settings)
            .map_err(|error| {
                tracing::warn!(event = "narration.fallback", %error, "Reading with the speech engine instead of the recording")
            })
            .ok()
    }

    /// `book` read by the speech engine, passing over the chapters it leaves out.
    fn speech_target(&self, book: Arc<BookModel>) -> Result<Target, String> {
        let plan = self.speech_plan(&book)?;
        let left_out = self.book_settings(&book.id).left_out;
        let mut target = Target {
            narration: narration::NarrationPlan::speech(&book, plan),
            book,
        };
        leaving_out::leave_out(&mut target, &left_out);
        Ok(target)
    }

    /// How every book's text is laid out changed (its pauses): each is read again when next
    /// loaded, and the book listened to at once, playing on from the same sentence (paused if
    /// it was) and planned again. Nothing it says changes, so no speech is asked for again.
    fn text_laid_out_again(&self) {
        self.shelf.unload_all();
        let Some(old) = self.playback.book() else {
            return;
        };
        let restarted = (|| {
            let book = self.load_book(&old.id)?;
            let settings = self.book_settings(&book.id);
            let recording = self.plays_recording(&book.id, &settings);
            let index = old
                .segment(self.playback.position().index)
                .and_then(|segment| book.index_of(&segment.id))
                .or_else(|| book.chapter_start(0))
                .ok_or("the book has no readable text")?;
            let mut target = match self.recorded_target(&book, &settings, recording) {
                Some(target) => target,
                None => self.speech_target(book)?,
            };
            // Playing on where it was, even in a chapter left out.
            leaving_out::admit(&mut target, index);
            let playing = self.playback.is_playing();
            let paused_since = self.playback.paused_since();
            drop(self.playback.stop());
            self.playback.use_own_speed(settings.playback_speed);
            self.follow_through(self.playback.start(target, index))?;
            if !playing {
                drop(self.playback.pause_since(paused_since));
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = restarted {
            self.report(
                "playback.restart",
                "Could not play on with the new pauses",
                error,
            );
        }
    }

    // ------------------------------------------------------------ settings

    /// Rebuilds the listening session's requests and cache state (voice/engine/settings
    /// changed), keeping the playhead.
    async fn restart_session(&self) -> Result<(), String> {
        let Some(book) = self.playback.book() else {
            return Ok(());
        };
        let playing = self.playback.is_playing();
        let paused_since = self.playback.paused_since();
        let segment = book
            .segment(self.playback.position().index)
            .map(|seg| seg.id.clone());
        // The playhead stays where it is, so there is no new place to save, and playing again
        // at once follows the stop through.
        drop(self.playback.stop());
        self.play(&book.id, None, segment).await?;
        if !playing {
            drop(self.playback.pause_since(paused_since));
        }
        Ok(())
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
                    self.speech_server.used();
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
                WorkerEvent::Failed { job, error, detail } => {
                    tracing::warn!(event = "gen.failed", key = %job.key, error = %detail, "Making audio failed");
                    self.speech_problems.record(&error, &detail, now_ms());
                    self.plan(PlanEvent::WorkFailed { error });
                }
                WorkerEvent::Discarded(_) => self.plan(PlanEvent::WorkDiscarded),
            }
        }
    }

    /// Looks at the player every tick while it plays, a sleep timer runs, or for a moment after
    /// a change; otherwise it waits for the next change (or a second), so an idle Aoide does
    /// not wake thirty times a second.
    async fn tick_loop(self: Arc<Self>) {
        let mut lively_until = Instant::now() + LINGER;
        loop {
            let wait = if Instant::now() < lively_until {
                TICK
            } else {
                IDLE_TICK
            };
            tokio::select! {
                () = self.playback.changed() => lively_until = Instant::now() + LINGER,
                () = tokio::time::sleep(wait) => {}
            }
            let audio = playback::LibraryAudio {
                library: &self.library,
                audio_dir: &self.paths.audio_dir,
            };
            let (moved, ended) = self
                .playback
                .advance(&audio, |to_align| self.align(to_align));
            // Advancing never opens or closes the output.
            let _ = self.follow_through(moved);
            self.output.keep_alive(&self.playback);
            let state = self.player_state();
            self.queue_tick(ended, &state);
            self.sleep(controls::SleepEvent::Tick {
                now: Instant::now(),
                chapter: state.chapter,
                playing: matches!(
                    state.status,
                    PlaybackStatus::Playing | PlaybackStatus::Buffering
                ),
            });
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
            let lively = matches!(
                state.status,
                PlaybackStatus::Playing | PlaybackStatus::Buffering
            ) || state.sleep.is_some();
            if let Some(state) = self.controls.observe(state, &now_playing) {
                self.emit(CoreEvent::PlayerState(state));
            }
            if lively {
                lively_until = Instant::now() + LINGER;
            }
        }
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
            let (polled, answer) = self.engine.poll(busy).await;
            if let Some(answer) = answer {
                self.speech_server.answered(answer);
            }
            if polled == engine::Polled::VoicesKnown {
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
        self.speech_server.stop(&engine::SpeechServer(self));
        self.stop_aligner();
        self.browsing.stop();
        self.inbox.stop();
    }
}
