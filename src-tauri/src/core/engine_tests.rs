//! How Core talks to the speech engine, seen from Core's public seam with scripted engines: which
//! voice listings count, when a launch command runs, and when `engine://status` is sent.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_tts::{
    BoxFuture, EngineDescriptor, EngineError, EngineProfile, EngineStatus, SpeechEngine, Synthesis,
    SynthesisRequest, Voice,
};

use super::tests::{library_folder_with, paths, until, Recorder};
use super::{Core, CoreEvent, Environment, Events, PrepareStage};

/// An engine whose status and voices a test sets, which can hold its voice listing until let
/// go, and which counts what it was asked.
pub(super) struct Scripted {
    pub(super) descriptor: EngineDescriptor,
    status: Mutex<EngineStatus>,
    voices: Vec<String>,
    /// While held, a voice listing waits for a permit.
    gate: tokio::sync::Semaphore,
    held: bool,
    pub(super) status_calls: AtomicUsize,
    pub(super) voice_calls: AtomicUsize,
    pub(super) listings_answered: AtomicUsize,
    requests: Mutex<Vec<SynthesisRequest>>,
}

impl Scripted {
    pub(super) fn new(kind: &str, status: EngineStatus, voices: &[&str], held: bool) -> Arc<Self> {
        Arc::new(Self {
            descriptor: EngineDescriptor {
                kind: kind.into(),
                name: kind.into(),
                adapter_version: 1,
                languages: None,
                word_timings: false,
                max_chars: 400,
                params: vec![],
            },
            status: Mutex::new(status),
            voices: voices.iter().map(|v| v.to_string()).collect(),
            gate: tokio::sync::Semaphore::new(0),
            held,
            status_calls: AtomicUsize::new(0),
            voice_calls: AtomicUsize::new(0),
            listings_answered: AtomicUsize::new(0),
            requests: Mutex::new(vec![]),
        })
    }

    /// Lets every listing held now or later answer.
    pub(super) fn release(&self) {
        self.gate.add_permits(64);
    }

    pub(super) fn set_status(&self, status: EngineStatus) {
        *self.status.lock().unwrap() = status;
    }
}

impl SpeechEngine for Scripted {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }
    fn status(&self) -> BoxFuture<'_, EngineStatus> {
        self.status_calls.fetch_add(1, Ordering::SeqCst);
        let status = self.status.lock().unwrap().clone();
        Box::pin(async move { status })
    }
    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>> {
        self.voice_calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if self.held {
                let _ = self.gate.acquire().await;
            }
            self.listings_answered.fetch_add(1, Ordering::SeqCst);
            Ok(self
                .voices
                .iter()
                .map(|id| Voice {
                    id: id.clone(),
                    display_name: id.trim_end_matches(".wav").into(),
                })
                .collect())
        })
    }
    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest,
    ) -> BoxFuture<'a, Result<Synthesis, EngineError>> {
        self.requests.lock().unwrap().push(request.clone());
        Box::pin(async {
            Ok(Synthesis {
                sample_rate: 24_000,
                samples: vec![0.0; 2_400],
                word_timings: None,
            })
        })
    }
}

pub(super) fn profile(kind: &str) -> EngineProfile {
    EngineProfile {
        id: kind.into(),
        name: format!("Engine {kind}"),
        kind: kind.into(),
        ..super::default_profile()
    }
}

/// Core over the test book, using `engines`, with the first one's profile; `extra` is more
/// TOML for its `[engine]` section.
async fn start(
    root: &std::path::Path,
    engines: &[&Arc<Scripted>],
    extra: &str,
) -> (Arc<Core>, Arc<Recorder>, String) {
    let id = library_folder_with(root, "", "");
    let kind = &engines[0].descriptor.kind;
    std::fs::write(
        root.join("config/settings.toml"),
        format!(
            "[engine]\nid = \"{kind}\"\nname = \"Engine {kind}\"\nkind = \"{kind}\"\n\
             baseUrl = \"http://127.0.0.1:9\"\nnetwork = \"loopback\"\n{extra}\n"
        ),
    )
    .unwrap();
    let events = Arc::new(Recorder::default());
    let mut registry = aoide_tts::Registry::new();
    for engine in engines {
        let engine = Arc::clone(engine);
        registry.register(engine.descriptor.clone(), move |_| {
            Ok(Arc::clone(&engine) as Arc<dyn SpeechEngine>)
        });
    }
    let environment = Environment {
        events: Arc::clone(&events) as Arc<dyn Events>,
        engines: registry,
        audio: Environment::silent_audio(),
        background: false,
        show_in_folder: Arc::new(|_| Ok(())),
        transcribers: Arc::new(|_| Err("no speech recognition in this test".into())),
    };
    let core = Core::start(environment, paths(root)).unwrap();
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    (core, events, id)
}

/// Waits up to ten seconds (a few of the engine loop's three-second rounds) for `check`.
async fn until_rounds(what: &str, check: impl Fn() -> bool) {
    for _ in 0..400 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn switching_engines_while_voices_are_being_listed_ignores_the_old_listing() {
    let dir = tempfile::tempdir().unwrap();
    let first = Scripted::new("first", EngineStatus::Ready(None), &["First.wav"], true);
    let second = Scripted::new("second", EngineStatus::Offline, &["Second.wav"], false);
    let (core, _events, id) = start(dir.path(), &[&first, &second], "").await;
    let book = super::BookSettings {
        voice_id: Some("Nowhere.wav".into()),
        ..Default::default()
    };
    core.save_book_settings(&id, book).await.unwrap();
    until("the first engine to be asked for its voices", || {
        first.voice_calls.load(Ordering::SeqCst) >= 1
    })
    .await;

    core.save_engine_profile(profile("second")).await.unwrap();
    first.release();
    until("the old listings to come back", || {
        first.listings_answered.load(Ordering::SeqCst) == first.voice_calls.load(Ordering::SeqCst)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(
        second.voice_calls.load(Ordering::SeqCst),
        0,
        "the new engine is not asked"
    );
    assert_eq!(
        core.book_overview(&id).unwrap().voice_warning,
        None,
        "the old engine's voices are not taken for the new one's"
    );
}

#[tokio::test]
async fn a_ready_engine_that_lists_no_voices_is_sent_the_saved_voice_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new("plain", EngineStatus::Ready(None), &[], false);
    let (core, _events, id) = start(dir.path(), &[&engine], "voiceId = \"Saved.wav\"").await;

    core.prepare_book(&id).await.unwrap();
    until("the book to be prepared", || {
        core.prepare_view(&id).map(|v| v.state) == Some(PrepareStage::Done)
    })
    .await;

    let requests = engine.requests.lock().unwrap().clone();
    assert!(!requests.is_empty());
    assert!(
        requests.iter().all(|r| r.voice_id == "Saved.wav"),
        "{requests:?}"
    );
}

#[tokio::test]
async fn an_offline_engine_with_a_launch_command_is_launched_once() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("launched.txt");
    let engine = Scripted::new("offline", EngineStatus::Offline, &[], false);
    let launch = format!(
        "[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"echo launched >> '{}'\"]",
        log.display()
    );
    let (_core, _events, _id) = start(dir.path(), &[&engine], &launch).await;

    until_rounds("a second look at the engine", || {
        engine.status_calls.load(Ordering::SeqCst) >= 2
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let launched = std::fs::read_to_string(&log).unwrap_or_default();
    assert_eq!(launched.lines().count(), 1, "{launched:?}");
}

#[tokio::test]
async fn the_engine_status_is_sent_only_when_it_changes() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new("steady", EngineStatus::Ready(None), &["A.wav"], false);
    let (_core, events, _id) = start(dir.path(), &[&engine], "").await;
    until_rounds("a second look at the engine", || {
        engine.status_calls.load(Ordering::SeqCst) >= 2
    })
    .await;
    assert_eq!(events.count(CoreEvent::ENGINE_STATUS), 1);

    engine.set_status(EngineStatus::Loading);
    until_rounds("a third look at the engine", || {
        engine.status_calls.load(Ordering::SeqCst) >= 3
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let sent = events.payloads(CoreEvent::ENGINE_STATUS);
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert_eq!(sent[1]["state"], "loading");
}
