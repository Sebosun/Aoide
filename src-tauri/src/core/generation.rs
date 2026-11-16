//! Making speech: what the engine's worker is asked for, from what is being listened to and
//! the books queued for preparing, and the generation view the window shows.
//!
//! A component with one way in: [`Generation::apply`] takes a [`PlanEvent`] (something that
//! changes what should be made), makes the change, plans the work again and sends
//! `generation://state` when the view changed. What it needs from the rest of the app (what is
//! being listened to, a book's speech plan) comes through [`GenerationHost`].

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use aoide_audio::{codec, Pcm};
use aoide_generation::{
    buffered_ahead_ms, estimate_ms, plan, Job, Listening, PlanItem, Preparing, Reason,
    SegmentStore, WorkItem, Worker, WorkerEvent, WorkerOptions,
};
use aoide_library::{BookId, CachedSegment, Library};
use aoide_tts::{cache_key, BoxFuture, SpeechEngine, Synthesis};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::book::BookModel;

use super::engine::Speech;
use super::error::Reporter;
use super::narration::NarrationPlan;
use super::preparation::{Plan, Preparation, PrepareView};
use super::{lock, now_ms, CoreEvent, Events, GenerationStateView, PreparingView};

/// How far ahead of the playhead speech is made.
const LOOKAHEAD_MS: u32 = 90_000;

/// Something that changes what should be made.
pub(super) enum PlanEvent {
    /// What is listened to, or where, changed; `jump` starts from the playhead at once.
    ListeningChanged {
        jump: bool,
    },
    /// Playback stopped: nothing is listened to.
    ListeningStopped,
    BookQueued(String),
    ChapterQueued {
        book: String,
        chapter: usize,
    },
    PrepareCancelled(String),
    PreparePaused {
        book: String,
        paused: bool,
    },
    /// The book left the library.
    BookForgotten(String),
    /// A book's voice settings changed: its plan is made again.
    BookSettingsChanged(String),
    /// Another engine makes speech from now on.
    EngineSwitched(Arc<dyn SpeechEngine>),
    /// The engine's voices became known: plans made without them are made again.
    VoicesKnown,
    /// Audio was deleted to keep the cache small.
    AudioLost {
        books: HashSet<String>,
        keys: HashSet<String>,
    },
    /// A book's audio was deleted to be made again (its keys forgotten first, see
    /// [`Generation::forget`]).
    BookAudioDeleted(String),
    /// The worker started on a segment (of the book being listened to, `segment_id`).
    WorkStarted {
        segment_id: Option<String>,
    },
    SegmentMade {
        key: String,
        duration_ms: u32,
    },
    WorkFailed {
        error: String,
    },
    /// The worker dropped what it made: its audio was deleted meanwhile.
    WorkDiscarded,
}

/// What is being listened to.
pub(super) struct Listened<'a> {
    pub book: &'a str,
    pub narration: &'a NarrationPlan,
    pub playhead: usize,
}

/// What generation needs from the rest of the app.
pub(super) trait GenerationHost {
    /// Calls `f` with what is being listened to, if anything.
    fn with_listening(&self, f: &mut dyn FnMut(Option<Listened<'_>>));
    /// The speech plan for book `id` (only `chapters` of it, when given), or `None` while there
    /// is no voice to plan it with.
    fn book_plan(&self, id: &str, chapters: Option<&[usize]>) -> Option<Result<Plan, String>>;
}

pub(super) struct Generation {
    worker: Mutex<Worker>,
    worker_events: UnboundedSender<WorkerEvent>,
    store: Arc<Store>,
    /// Whole-book preparation.
    preparation: Mutex<Preparation>,
    state: Mutex<GenerationStateView>,
    /// The view last sent to the window.
    sent: Mutex<Option<GenerationStateView>>,
    library: Arc<Mutex<Library>>,
    events: Arc<dyn Events>,
}

impl Generation {
    /// Generation with `engine`, keeping audio in `audio_dir`. The worker's events come back on
    /// the receiver, for the caller to turn into [`PlanEvent`]s.
    pub fn new(
        library: Arc<Mutex<Library>>,
        audio_dir: PathBuf,
        engine: Arc<dyn SpeechEngine>,
        reporter: Reporter,
        events: Arc<dyn Events>,
    ) -> (Self, UnboundedReceiver<WorkerEvent>) {
        let (worker_events, receiver) = tokio::sync::mpsc::unbounded_channel();
        let store = Arc::new(Store {
            library: Arc::clone(&library),
            audio_dir,
        });
        let worker = Worker::spawn(
            engine,
            Arc::clone(&store) as Arc<dyn SegmentStore>,
            WorkerOptions::default(),
            worker_events.clone(),
        );
        let preparation = Preparation::load(&lock(&library), reporter);
        let generation = Self {
            worker: Mutex::new(worker),
            worker_events,
            store,
            preparation: Mutex::new(preparation),
            state: Mutex::new(GenerationStateView::default()),
            sent: Mutex::new(None),
            library,
            events,
        };
        (generation, receiver)
    }

    /// The speech engine's plan for reading all of `book` in its voice and settings.
    /// The speech that reads `book` with `speech`, and which of it is made already.
    pub fn speech_plan(&self, book: &BookModel, speech: Speech) -> Plan {
        let Speech {
            voicing,
            descriptor,
            model,
        } = speech;
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
        Plan { items, requests }
    }

    /// The audio under `keys` is about to be deleted: the worker makes them again when asked
    /// (it skips keys it made in this run otherwise). Await the result before deleting, so a
    /// segment being stored now is not written after the deletion.
    pub fn forget(&self, keys: Vec<String>) -> aoide_generation::Forgotten {
        lock(&self.worker).forget(keys)
    }

    /// Makes the change `event` stands for, plans the work again and sends the view if it
    /// changed. The one way generation changes.
    pub fn apply(&self, event: PlanEvent, host: &dyn GenerationHost) {
        let replan = self.change(event);
        if let Some(jump) = replan {
            self.replan(jump, host);
        }
        self.send_view();
    }

    /// Book `id`'s preparation as the window shows it, if it is queued or prepared.
    pub fn prepare_view(&self, id: &str) -> Option<PrepareView> {
        let preparation = lock(&self.preparation);
        preparation.view(&lock(&self.library), id)
    }

    /// The books queued for preparing.
    pub fn queued(&self) -> Vec<String> {
        lock(&self.preparation).queued()
    }

    /// Whether there is speech to make: for what is listened to, or for a queued book.
    pub fn wants_speech(&self) -> bool {
        lock(&self.state).pending > 0
    }

    /// Whether the worker is making a segment now.
    pub fn busy(&self) -> bool {
        lock(&self.state).generating_segment_id.is_some()
    }

    #[cfg(any(test, debug_assertions))]
    pub fn state(&self) -> GenerationStateView {
        lock(&self.state).clone()
    }

    /// Makes the change; returns whether to plan again, and if so whether from the playhead
    /// at once (`Some(true)`).
    fn change(&self, event: PlanEvent) -> Option<bool> {
        match event {
            PlanEvent::ListeningChanged { jump } => Some(jump),
            PlanEvent::ListeningStopped => {
                *lock(&self.state) = GenerationStateView::default();
                Some(true)
            }
            PlanEvent::BookQueued(book) => {
                let mut preparation = lock(&self.preparation);
                preparation.enqueue(&lock(&self.library), &book);
                Some(false)
            }
            PlanEvent::ChapterQueued { book, chapter } => {
                let mut preparation = lock(&self.preparation);
                preparation.enqueue_chapter(&lock(&self.library), &book, chapter);
                Some(false)
            }
            PlanEvent::PrepareCancelled(book) => {
                let mut preparation = lock(&self.preparation);
                preparation.cancel(&lock(&self.library), &book);
                Some(false)
            }
            PlanEvent::PreparePaused { book, paused } => {
                let mut preparation = lock(&self.preparation);
                preparation.set_paused(&lock(&self.library), &book, paused);
                Some(false)
            }
            PlanEvent::BookForgotten(book) => {
                let mut preparation = lock(&self.preparation);
                preparation.forget(&lock(&self.library), &book);
                Some(false)
            }
            PlanEvent::BookSettingsChanged(book) => {
                lock(&self.preparation).drop_plans(Some(&book));
                Some(true)
            }
            PlanEvent::EngineSwitched(engine) => {
                let worker = Worker::spawn(
                    engine,
                    Arc::clone(&self.store) as Arc<dyn SegmentStore>,
                    WorkerOptions::default(),
                    self.worker_events.clone(),
                );
                // The old worker finishes any in-flight request and then stops.
                *lock(&self.worker) = worker;
                lock(&self.state).generating_segment_id = None;
                lock(&self.preparation).drop_plans(None);
                Some(true)
            }
            PlanEvent::VoicesKnown => {
                let mut preparation = lock(&self.preparation);
                if preparation.planned_books().is_empty() {
                    return None;
                }
                preparation.drop_plans(None);
                Some(false)
            }
            PlanEvent::AudioLost { books, keys } => {
                lock(&self.worker).forget(keys.iter().cloned());
                let mut preparation = lock(&self.preparation);
                preparation.audio_lost(&lock(&self.library), &books, &keys);
                Some(false)
            }
            PlanEvent::BookAudioDeleted(book) => {
                let mut preparation = lock(&self.preparation);
                preparation.audio_lost(
                    &lock(&self.library),
                    &HashSet::from([book.clone()]),
                    &HashSet::new(),
                );
                preparation.drop_plans(Some(&book));
                Some(true)
            }
            PlanEvent::WorkStarted { segment_id } => {
                lock(&self.state).generating_segment_id = segment_id;
                None
            }
            PlanEvent::SegmentMade { key, duration_ms } => {
                // Keys are content hashes, so every plan that shares them is marked.
                let finished = {
                    let mut preparation = lock(&self.preparation);
                    preparation.segment_made(&lock(&self.library), &key, duration_ms)
                };
                if finished {
                    self.emit(CoreEvent::LibraryChanged);
                }
                let mut state = lock(&self.state);
                state.generating_segment_id = None;
                state.error = None;
                Some(false)
            }
            PlanEvent::WorkDiscarded => {
                lock(&self.state).generating_segment_id = None;
                None
            }
            PlanEvent::WorkFailed { error } => {
                let mut state = lock(&self.state);
                state.generating_segment_id = None;
                state.error = Some(error);
                None
            }
        }
    }

    /// Recomputes the worker's plan from what is listened to and the prepare queue. `jump`
    /// replaces the plan at once; otherwise the worker finishes what it is on first.
    fn replan(&self, jump: bool, host: &dyn GenerationHost) {
        self.plan_waiting_books(host);
        let mut work = Vec::new();
        host.with_listening(&mut |listening| work = self.work(listening));
        let worker = lock(&self.worker);
        if jump {
            worker.set_plan(work);
        } else {
            worker.update_plan(work);
        }
    }

    /// The work for the speech heard now (if the narration speaks) and the planned books.
    fn work(&self, listening: Option<Listened<'_>>) -> Vec<WorkItem> {
        let preparation = lock(&self.preparation);
        let planned = preparation.planned_books();
        // A recording is not made by the speech engine, only the text it lacks when asked to
        // say that.
        let speech = listening.filter(|l| l.narration.speaks());
        let speech_items = speech.as_ref().map(|l| l.narration.speech_items());
        let playhead = speech.as_ref().map_or(0, |l| l.playhead);
        let heard = speech
            .as_ref()
            .zip(speech_items.as_ref())
            .map(|(l, items)| Listening {
                book: l.book,
                items,
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
        let jobs = plan(heard.as_ref(), &preparing);
        let pending = jobs.len();
        let work = jobs
            .into_iter()
            .filter_map(|job| {
                let request = match (job.reason, speech.as_ref()) {
                    (Reason::Playhead, Some(l)) => l.narration.request(job.index),
                    _ => planned
                        .iter()
                        .find(|(book, _)| *book == job.book)?
                        .1
                        .requests
                        .get(job.index)?
                        .as_ref(),
                }?
                .clone();
                Some(WorkItem { job, request })
            })
            .collect();
        let mut state = lock(&self.state);
        // Only speech is made; a recording that says nothing has nothing to report here.
        state.book_id = speech.as_ref().map(|l| l.book.to_owned());
        state.pending = pending;
        state.buffered_ahead_ms = speech_items
            .as_ref()
            .map_or(0, |items| buffered_ahead_ms(items, playhead));
        work
    }

    /// Builds plans for queued books waiting for one. A book that cannot be planned leaves the
    /// queue.
    fn plan_waiting_books(&self, host: &dyn GenerationHost) {
        let waiting = lock(&self.preparation).unplanned();
        let mut changed = false;
        for id in waiting {
            let chapters = lock(&self.preparation).chapters(&id);
            let Some(built) = host.book_plan(&id, chapters.as_deref()) else {
                continue; // Voice not known yet; planned once playback or preparation resolves it.
            };
            let mut preparation = lock(&self.preparation);
            let library = lock(&self.library);
            match built {
                Ok(plan) => preparation.planned(&library, &id, plan),
                Err(error) => {
                    tracing::warn!(event = "prep.plan", book = crate::logging::book(&id), %error, "Cannot plan the book's preparation");
                    preparation.cancel(&library, &id);
                }
            }
            changed = true;
        }
        if changed {
            self.emit(CoreEvent::LibraryChanged);
        }
    }

    /// Sends the generation view if it differs from the one last sent.
    fn send_view(&self) {
        let preparing = lock(&self.preparation)
            .current()
            .map(|(id, done, total)| PreparingView {
                book_id: id.into(),
                done,
                total,
            });
        let view = {
            let mut state = lock(&self.state);
            state.preparing = preparing;
            state.clone()
        };
        let mut sent = lock(&self.sent);
        if sent.as_ref() != Some(&view) {
            *sent = Some(view.clone());
            drop(sent);
            self.emit(CoreEvent::GenerationState(view));
        }
    }

    fn emit(&self, event: CoreEvent) {
        if let Ok(payload) = event.payload() {
            self.events.emit(event.name(), payload);
        }
    }
}

/// Where the worker puts what it makes: an Opus file under the audio folder, indexed in the
/// library.
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

impl GenerationHost for super::Core {
    fn with_listening(&self, f: &mut dyn FnMut(Option<Listened<'_>>)) {
        let playhead = self.playback.position().index;
        let mut called = false;
        self.playback.with_narration(|book, narration| {
            called = true;
            f(Some(Listened {
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

impl super::Core {
    pub async fn prepare_book(&self, id: &str) -> Result<(), String> {
        self.load_book(id)?;
        self.engine.want();
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

    pub fn prepare_view(&self, id: &str) -> Option<PrepareView> {
        self.generation.prepare_view(id)
    }

    /// Tells generation what changed; see [`generation::Generation::apply`].
    pub(super) fn plan(&self, event: PlanEvent) {
        self.generation.apply(event, self);
        if self.generation.wants_speech() {
            self.speech_server.want();
        }
    }

    pub(super) fn segment_id_for(&self, job: &Job) -> Option<String> {
        if let Some(book) = self.playback.book().filter(|b| b.id == job.book) {
            return book.segment(job.index).map(|s| s.id.clone());
        }
        self.load_book(&job.book)
            .ok()?
            .segment(job.index)
            .map(|s| s.id.clone())
    }
}

impl super::Core {
    /// The speech that reads `book`, in its voice settings.
    pub(super) fn speech_plan(&self, book: &BookModel) -> Result<Plan, String> {
        let settings = self.book_settings(&book.id);
        let speech = self.engine.speech(&settings, book.language.as_deref())?;
        Ok(self.generation.speech_plan(book, speech))
    }

    /// How much of each chapter's speech is made.
    pub async fn chapter_audio(&self, id: &str) -> Result<Vec<super::ChapterAudioView>, String> {
        // An audiobook has no text, so no speech is made for it.
        if self.item_kind(id) == Some(aoide_library::ItemKind::Audiobook) {
            return Ok(Vec::new());
        }
        let book = self.load_book(id)?;
        let _ = self.ensure_voice().await; // Without a voice nothing can be cached yet.
        Ok(self.speech_plan(&book)?.chapter_audio(&book))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use aoide_generation::{estimate_ms, PlanItem};
    use aoide_library::NewBook;
    use aoide_tts::{EngineStatus, SynthesisRequest};

    use super::super::engine_tests::Scripted;
    use super::super::tests::{until, Recorder};
    use super::*;

    /// A host listening to nothing, whose one book is three segments in `voice`; it counts
    /// how often it was asked for a plan.
    struct Host {
        voice: Mutex<String>,
        asked: Mutex<usize>,
    }

    impl GenerationHost for Host {
        fn with_listening(&self, f: &mut dyn FnMut(Option<Listened<'_>>)) {
            f(None)
        }

        fn book_plan(&self, _: &str, _: Option<&[usize]>) -> Option<Result<Plan, String>> {
            *lock(&self.asked) += 1;
            let voice = lock(&self.voice).clone();
            let texts = ["One.", "Two.", "Three."];
            Some(Ok(Plan {
                items: texts
                    .iter()
                    .map(|text| PlanItem {
                        key: format!("{voice}-{text}"),
                        speakable: true,
                        cached: false,
                        duration_ms: estimate_ms(text.chars().count()),
                    })
                    .collect(),
                requests: texts
                    .iter()
                    .map(|text| {
                        Some(SynthesisRequest {
                            text: (*text).into(),
                            voice_id: voice.clone(),
                            language: None,
                            params: Default::default(),
                        })
                    })
                    .collect(),
            }))
        }
    }

    type Made = UnboundedReceiver<WorkerEvent>;

    fn generation(dir: &std::path::Path, engine: &Arc<Scripted>) -> (Generation, Made, Host) {
        let library = Library::open_in_memory().unwrap();
        library
            .upsert_book(
                &NewBook {
                    id: BookId("book".into()),
                    path: dir.join("book.epub"),
                    title: None,
                    authors: vec![],
                    language: None,
                    cover_path: None,
                    error: None,
                    kind: aoide_library::ItemKind::Book,
                },
                0,
            )
            .unwrap();
        let (generation, made) = Generation::new(
            Arc::new(Mutex::new(library)),
            dir.join("audio"),
            Arc::clone(engine) as Arc<dyn SpeechEngine>,
            Reporter::default(),
            Arc::new(Recorder::default()),
        );
        let host = Host {
            voice: Mutex::new("Guess.wav".into()),
            asked: Mutex::new(0),
        };
        (generation, made, host)
    }

    #[tokio::test]
    async fn once_voices_are_known_a_planned_book_is_planned_again() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Scripted::new("plain", EngineStatus::Ready(None), &[], false);
        engine.hold_synthesis();
        let (generation, _made, host) = generation(dir.path(), &engine);
        generation.apply(PlanEvent::BookQueued("book".into()), &host);
        until("the first request", || engine.texts().len() == 1).await;

        *lock(&host.voice) = "Known.wav".into();
        generation.apply(PlanEvent::VoicesKnown, &host);
        engine.let_go();
        until("every segment", || engine.texts().len() >= 4).await;
        tokio::time::sleep(Duration::from_millis(100)).await;

        // The first was being made already; the plan made again names the voice now known.
        assert_eq!(
            engine.voices_used(),
            ["Guess.wav", "Known.wav", "Known.wav", "Known.wav"]
        );
    }

    #[tokio::test]
    async fn voices_becoming_known_with_nothing_planned_plans_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Scripted::new("plain", EngineStatus::Ready(None), &[], false);
        let (generation, _made, host) = generation(dir.path(), &engine);

        generation.apply(PlanEvent::VoicesKnown, &host);
        tokio::time::sleep(Duration::from_millis(100)).await;

        assert_eq!(*lock(&host.asked), 0);
        assert!(engine.texts().is_empty());
    }
}
