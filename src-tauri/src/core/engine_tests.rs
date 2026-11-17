//! How Core talks to the speech engine, seen from Core's public seam with scripted engines: which
//! voice listings count, when a launch command runs, and when `engine://status` is sent.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_tts::{
    BoxFuture, EngineDescriptor, EngineError, EngineProfile, EngineStatus, SpeechEngine, Synthesis,
    SynthesisRequest, Voice,
};

use super::tests::{paths, until, Recorder};
use super::{Core, CoreEvent, Environment, Events, PrepareStage};

/// An engine whose status and voices a test sets, which can hold its voice listing until let
/// go, and which counts what it was asked.
pub(super) struct Scripted {
    pub(super) descriptor: EngineDescriptor,
    status: Mutex<EngineStatus>,
    voices: Mutex<Vec<String>>,
    /// Listing voices fails while set.
    pub(super) voices_fail: std::sync::atomic::AtomicBool,
    /// While set, each synthesis waits for a permit ([`Scripted::allow`]).
    synthesis_held: std::sync::atomic::AtomicBool,
    synthesis_gate: tokio::sync::Semaphore,
    /// Synthesis fails while set.
    pub(super) synthesis_fails: std::sync::atomic::AtomicBool,
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
                cheap_voice_switch: true,
                params: vec![],
            },
            status: Mutex::new(status),
            voices: Mutex::new(voices.iter().map(|v| v.to_string()).collect()),
            voices_fail: Default::default(),
            synthesis_held: Default::default(),
            synthesis_gate: tokio::sync::Semaphore::new(0),
            synthesis_fails: Default::default(),
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

    /// Makes each synthesis wait until [`Scripted::allow`]ed.
    pub(super) fn hold_synthesis(&self) {
        self.synthesis_held.store(true, Ordering::SeqCst);
    }

    /// Lets `n` more held syntheses finish.
    pub(super) fn allow(&self, n: usize) {
        self.synthesis_gate.add_permits(n);
    }

    /// Lets every held synthesis finish, now and later.
    pub(super) fn let_go(&self) {
        self.synthesis_held.store(false, Ordering::SeqCst);
        self.synthesis_gate.add_permits(1_000);
    }

    /// What it was asked to say, in order.
    pub(super) fn texts(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.text.clone())
            .collect()
    }

    /// The voices it was asked to speak in, in order.
    pub(super) fn voices_used(&self) -> Vec<String> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.voice_id.clone())
            .collect()
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
            if self.voices_fail.load(Ordering::SeqCst) {
                return Err(EngineError::Unavailable("not listening".into()));
            }
            Ok(self
                .voices
                .lock()
                .unwrap()
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
        Box::pin(async move {
            if self.synthesis_held.load(Ordering::SeqCst) {
                if let Ok(permit) = self.synthesis_gate.acquire().await {
                    permit.forget();
                }
            }
            if self.synthesis_fails.load(Ordering::SeqCst) {
                return Err(EngineError::Unavailable("the engine fell over".into()));
            }
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
pub(super) async fn start(
    root: &std::path::Path,
    engines: &[&Arc<Scripted>],
    extra: &str,
) -> (Arc<Core>, Arc<Recorder>, String) {
    start_holding(root, engines, extra, &super::tests::epub()).await
}

/// [`start`], the library holding `bytes` as its one book.
pub(super) async fn start_holding(
    root: &std::path::Path,
    engines: &[&Arc<Scripted>],
    extra: &str,
    bytes: &[u8],
) -> (Arc<Core>, Arc<Recorder>, String) {
    let id = super::tests::library_folder_holding(root, "", "", bytes);
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
        open_link: Arc::new(|_| Ok(())),
        transcribers: Arc::new(|_| Err("no speech recognition in this test".into())),
        fetcher: super::articles::offline(),
        browser: Arc::new(super::browsing::testing::ScriptedBrowser::default()),
        browser_download: None,
        notifier: Arc::new(super::notices::testing::Recorded::default()),
        clock: super::environment::Clock::default(),
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
async fn an_offline_engine_with_a_launch_command_is_launched_once_speech_is_wanted() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("launched.txt");
    let engine = Scripted::new("offline", EngineStatus::Offline, &["A.wav"], false);
    let launch = format!(
        "[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"echo launched >> '{}'; exec sleep 300\"]",
        log.display()
    );
    let (core, _events, id) = start(dir.path(), &[&engine], &launch).await;
    let launched = || std::fs::read_to_string(&log).unwrap_or_default();

    until_rounds("a second look at the engine", || {
        engine.status_calls.load(Ordering::SeqCst) >= 2
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(launched(), "", "nothing wants speech yet");
    assert_eq!(
        core.engine_status().detail.as_deref(),
        Some("Starts when needed")
    );

    let _ = core.prepare_book(&id).await;
    until_rounds("the launch", || !launched().is_empty()).await;
    until_rounds("another look at the engine", || {
        engine.status_calls.load(Ordering::SeqCst) >= 4
    })
    .await;
    assert_eq!(launched().lines().count(), 1, "{:?}", launched());
    assert_eq!(
        core.engine_status().detail.as_deref(),
        Some("Starting the voice…")
    );
    core.shutdown();
}

#[tokio::test]
async fn a_launched_server_that_ends_at_once_shows_its_exit_code_not_offline() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new("offline", EngineStatus::Offline, &["A.wav"], false);
    let launch =
        "[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"echo 'no such script' >&2; exit 2\"]";
    let (core, events, id) = start(dir.path(), &[&engine], launch).await;

    let _ = core.prepare_book(&id).await;
    until_rounds("the failure", || {
        core.engine_status().state == super::EngineState::Error
    })
    .await;
    // A preparation stalled on a failed engine is not work: Quit when done does not wait on it.
    assert_eq!(core.working().preparing, None);

    let status = core.engine_status();
    assert!(status.wanted);
    let detail = status.detail.unwrap();
    assert!(detail.contains("exit code 2"), "{detail}");
    let sent = events.payloads(CoreEvent::ENGINE_STATUS);
    assert_eq!(sent.last().unwrap()["state"], "error", "{sent:?}");
    let log = dir.path().join("data/logs/speech-server.log");
    assert_eq!(std::fs::read_to_string(log).unwrap(), "no such script\n");
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

#[tokio::test]
async fn a_book_is_read_to_the_end_with_a_failing_engine_and_its_problem_waits_for_listening() {
    use super::ReadState;
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new(
        "broken",
        EngineStatus::Error("the model did not load".into()),
        &[],
        false,
    );
    let (core, events, id) = start(dir.path(), &[&engine], "").await;
    until_rounds("a look at the engine", || {
        engine.status_calls.load(Ordering::SeqCst) >= 1
    })
    .await;

    core.open_book(&id).unwrap();
    let mut last = None;
    for chapter in 0..core.book_overview(&id).unwrap().chapters.len() {
        let view = core.chapter(&id, chapter).unwrap();
        for segment in view.blocks.iter().flat_map(|b| &b.segments) {
            last = Some((chapter, segment.id.clone()));
            core.set_reading_place(&id, chapter, &segment.id).unwrap();
        }
    }
    let (chapter, segment) = last.unwrap();
    assert!(core.set_reading_place(&id, chapter, &segment).is_ok());
    assert_eq!(
        core.list_books().unwrap()[0].read_state,
        ReadState::Finished
    );
    assert_eq!(events.count(CoreEvent::PROBLEM), 0);
    let sent = events.payloads(CoreEvent::ENGINE_STATUS);
    assert!(!sent.is_empty());
    assert!(
        sent.iter().all(|s| s["wanted"] == false),
        "the engine's problem is not shown to someone only reading: {sent:?}"
    );
    assert!(!core.engine_status().wanted);

    // Asking to listen makes the problem the user's.
    let _ = core.play(&id, Some(0), None).await;
    let sent = events.payloads(CoreEvent::ENGINE_STATUS);
    assert_eq!(sent.last().unwrap()["wanted"], true, "{sent:?}");
    assert_eq!(sent.last().unwrap()["state"], "error");
    core.stop();
}

#[tokio::test]
async fn saving_a_new_voice_keeps_the_launched_server_and_a_new_command_replaces_it() {
    use super::engine_process::process_tests::alive_in;
    let dir = tempfile::tempdir().unwrap();
    let (pid, other) = (dir.path().join("pid"), dir.path().join("other"));
    let engine = Scripted::new("local", EngineStatus::Offline, &["A.wav", "B.wav"], false);
    let launch = format!(
        "voiceId = \"A.wav\"\n[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"echo $$ > '{}'; exec sleep 300\"]",
        pid.display()
    );
    let (core, _events, id) = start(dir.path(), &[&engine], &launch).await;
    core.prepare_book(&id).await.unwrap();
    until_rounds("the launch", || alive_in(&pid) == Some(true)).await;
    engine.set_status(EngineStatus::Ready(None));
    until_rounds("ready", || {
        core.engine_status().state != super::EngineState::Loading
    })
    .await;

    let mut profile = core.engine_options().profile;
    profile.voice_id = Some("B.wav".into());
    core.save_engine_profile(profile.clone()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(alive_in(&pid), Some(true), "only the voice changed");

    let launch = profile.launch.as_mut().unwrap();
    launch.args[1] = format!("echo $$ > '{}'; exec sleep 300", other.display());
    engine.set_status(EngineStatus::Offline);
    core.save_engine_profile(profile).await.unwrap();
    until_rounds("the old server to stop", || alive_in(&pid) == Some(false)).await;

    core.prepare_book(&id).await.unwrap();
    until_rounds("the new server", || alive_in(&other) == Some(true)).await;
    core.stop_speech_server();
    until_rounds("stop now", || alive_in(&other) == Some(false)).await;
    core.shutdown();
}

#[tokio::test]
async fn try_again_launches_a_failed_server_once_more() {
    let dir = tempfile::tempdir().unwrap();
    let marks = dir.path().join("launched.txt");
    let engine = Scripted::new("offline", EngineStatus::Offline, &["A.wav"], false);
    let launch = format!(
        "[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"echo x >> '{}'; exit 2\"]",
        marks.display()
    );
    let (core, _events, id) = start(dir.path(), &[&engine], &launch).await;
    let launched = || {
        std::fs::read_to_string(&marks)
            .unwrap_or_default()
            .lines()
            .count()
    };
    let _ = core.prepare_book(&id).await;
    until_rounds("the failure", || {
        core.engine_status().state == super::EngineState::Error
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(launched(), 1);

    core.retry_speech_server();
    until_rounds("the second launch", || launched() == 2).await;
}

#[tokio::test]
async fn a_stopped_server_is_not_woken_by_a_prepared_book_or_a_voice_menu() {
    use super::engine_process::process_tests::alive_in;
    let dir = tempfile::tempdir().unwrap();
    let pid = dir.path().join("pid");
    let engine = Scripted::new("local", EngineStatus::Offline, &["A.wav"], false);
    let launch = format!(
        "voiceId = \"A.wav\"\n[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"echo $$ > '{}'; exec sleep 300\"]",
        pid.display()
    );
    let (core, _events, id) = start(dir.path(), &[&engine], &launch).await;
    core.prepare_book(&id).await.unwrap();
    until_rounds("the launch", || alive_in(&pid) == Some(true)).await;
    engine.set_status(EngineStatus::Ready(None));
    until_rounds("the book to be prepared", || {
        core.prepare_view(&id).map(|v| v.state) == Some(PrepareStage::Done)
    })
    .await;

    core.stop_speech_server(); // As an idle stop does.
    engine.set_status(EngineStatus::Offline);
    engine.voices_fail.store(true, Ordering::SeqCst);
    until_rounds("the stop", || alive_in(&pid) == Some(false)).await;
    std::fs::remove_file(&pid).unwrap();

    let voices = core.engine_voices().await.unwrap();
    assert_eq!(voices.len(), 1, "the voices last seen");
    core.play(&id, Some(0), None).await.unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await; // At least one more look at the engine.

    assert!(
        !pid.exists(),
        "playing made audio and a voice menu start nothing"
    );
    assert_eq!(core.engine_status().state, super::EngineState::Offline);
    core.stop();
    core.shutdown();
}
