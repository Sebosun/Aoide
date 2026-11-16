//! The sync job driven through its own interface, with a test host in place of the app: one
//! book in an in-memory library, a scripted transcriber, and a recording whose samples encode
//! their own time (see `core/recording_tests.rs`).

use std::sync::{Arc, Mutex};

use super::super::environment::Clock;
use super::super::notices::Happened;
use super::super::recording::{Narration, RecognitionEngine, RecognitionSettings, SyncAction};
use super::super::recording_tests::{read, scripted, write_recording};
use super::super::syncing::Stage;
use super::super::tests::epub;
use super::super::CoreEvent;
use super::{Recordings, SyncHost};

/// The sync job's host in a test: one book, and what the job tells it.
struct TestHost {
    book: Arc<crate::book::BookModel>,
    events: Mutex<Vec<&'static str>>,
    synced: Mutex<Vec<bool>>,
    removed: Mutex<Vec<String>>,
    happened: Mutex<Vec<Happened>>,
}

impl SyncHost for TestHost {
    fn emit(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event.name());
    }

    fn load_book(&self, _: &str) -> Result<Arc<crate::book::BookModel>, String> {
        Ok(Arc::clone(&self.book))
    }

    fn recognition(&self) -> RecognitionSettings {
        // A recognition server needs no model here.
        RecognitionSettings {
            engine: RecognitionEngine::Server,
            ..Default::default()
        }
    }

    fn narration(&self, _: &str) -> Option<Narration> {
        Some(Narration::Recording)
    }

    fn synced(&self, _: &str, ready: bool) {
        self.synced.lock().unwrap().push(ready);
    }

    fn checked(&self, _: &str, _: bool) {}

    fn removed(&self, id: &str) {
        self.removed.lock().unwrap().push(id.to_owned());
    }

    fn happened(&self, happened: Happened, _: std::time::Duration) {
        self.happened.lock().unwrap().push(happened);
    }
}

#[test]
fn the_sync_job_runs_on_its_own_and_tells_its_host_how_far_it_got() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = epub();
    let id = aoide_library::content_hash(&bytes).0;
    let library = aoide_library::Library::open_in_memory().unwrap();
    let book = aoide_library::NewBook {
        id: aoide_library::BookId(id.clone()),
        path: dir.path().join("quay-lights.epub"),
        title: Some("Quay Lights".into()),
        authors: vec![],
        language: Some("en".into()),
        cover_path: None,
        error: None,
        kind: aoide_library::ItemKind::Book,
    };
    library.upsert_book(&book, 0).unwrap();
    let host = TestHost {
        book: Arc::new(crate::book::BookModel::parse(&id, &bytes, &Default::default()).unwrap()),
        events: Mutex::default(),
        synced: Mutex::default(),
        removed: Mutex::default(),
        happened: Mutex::default(),
    };
    let texts: Vec<String> = host.book.chapters.iter().map(|c| c.text.clone()).collect();
    let (factory, script, _, _) = scripted();
    let words = read(&texts, 1_000);
    let total = words.last().unwrap().1 + 1_000;
    *script.lock().unwrap() = words;
    let audio = dir.path().join("quay-lights.wav");
    write_recording(&audio, total);

    let recordings = Recordings::new(
        Arc::new(Mutex::new(library)),
        factory,
        dir.path().join("models"),
        Clock::default(),
    );
    let view = recordings
        .attach_recording(&host, &id, vec![audio.to_string_lossy().into()])
        .unwrap();
    assert_eq!((view.stage, view.listenable), (Stage::Checking, false));
    while recordings.sync_step(&host) {}

    let view = recordings.view(&host, &id).unwrap();
    assert_eq!(view.stage, Stage::Ready, "{view:?}");
    assert!(view.listenable && view.narrates);
    assert_eq!(
        host.synced.lock().unwrap().last(),
        Some(&true),
        "told once synced"
    );
    assert!(host.events.lock().unwrap().contains(&CoreEvent::SYNC_STATE));
    assert_eq!(
        *host.happened.lock().unwrap(),
        [Happened::Synced(id.clone())],
        "told once, when the sync ended"
    );
    let (playable, synced) = recordings.playable(&host, &host.book).unwrap();
    assert_eq!(synced, host.book.total_entries(), "all of it is synced");
    assert!(playable.clips.iter().all(|c| !c.audio.is_empty()));
    let (again, _) = recordings.playable(&host, &host.book).unwrap();
    assert!(
        Arc::ptr_eq(&playable, &again),
        "not laid out again while nothing changed"
    );
    recordings
        .sync_action(&host, &id, SyncAction::Resync)
        .unwrap();
    while recordings.sync_step(&host) {}
    let (resynced, _) = recordings.playable(&host, &host.book).unwrap();
    assert!(
        !Arc::ptr_eq(&playable, &resynced),
        "laid out again after syncing again"
    );
}

/// A synced recording of the test book, with the host that saw it synced.
fn synced() -> (tempfile::TempDir, String, TestHost, Recordings) {
    let dir = tempfile::tempdir().unwrap();
    let bytes = epub();
    let id = aoide_library::content_hash(&bytes).0;
    let library = aoide_library::Library::open_in_memory().unwrap();
    library
        .upsert_book(
            &aoide_library::NewBook {
                id: aoide_library::BookId(id.clone()),
                path: dir.path().join("quay-lights.epub"),
                title: Some("Quay Lights".into()),
                authors: vec![],
                language: Some("en".into()),
                cover_path: None,
                error: None,
                kind: aoide_library::ItemKind::Book,
            },
            0,
        )
        .unwrap();
    let host = TestHost {
        book: Arc::new(crate::book::BookModel::parse(&id, &bytes, &Default::default()).unwrap()),
        events: Mutex::default(),
        synced: Mutex::default(),
        removed: Mutex::default(),
        happened: Mutex::default(),
    };
    let texts: Vec<String> = host.book.chapters.iter().map(|c| c.text.clone()).collect();
    let (factory, script, _, _) = scripted();
    let words = read(&texts, 1_000);
    let total = words.last().unwrap().1 + 1_000;
    *script.lock().unwrap() = words;
    let audio = dir.path().join("quay-lights.wav");
    write_recording(&audio, total);
    let recordings = Recordings::new(
        Arc::new(Mutex::new(library)),
        factory,
        dir.path().join("models"),
        Clock::default(),
    );
    recordings
        .attach_recording(&host, &id, vec![audio.to_string_lossy().into()])
        .unwrap();
    while recordings.sync_step(&host) {}
    (dir, id, host, recordings)
}

#[test]
fn the_check_every_start_makes_tells_of_nothing() {
    let (dir, id, host, recordings) = synced();
    host.happened.lock().unwrap().clear();
    let (factory, _, _, _) = scripted();
    let restarted = Recordings::new(
        Arc::clone(&recordings.library),
        factory,
        dir.path().join("models"),
        Clock::default(),
    );
    restarted.load_recordings(true);
    while restarted.sync_step(&host) {}
    assert_eq!(restarted.view(&host, &id).unwrap().stage, Stage::Ready);
    assert_eq!(*host.happened.lock().unwrap(), []);
}

#[test]
fn removing_a_recording_forgets_it_keeps_its_files_and_tells_the_host_once() {
    let (dir, id, host, recordings) = synced();
    assert_eq!(recordings.view(&host, &id).unwrap().stage, Stage::Ready);

    recordings.remove(&host, &id).unwrap();

    assert!(recordings.view(&host, &id).is_none(), "its row is gone");
    assert!(!recordings.listenable(&id));
    assert!(recordings.paired_files(None).is_empty());
    assert_eq!(*host.removed.lock().unwrap(), vec![id.clone()]);
    assert!(host
        .events
        .lock()
        .unwrap()
        .contains(&CoreEvent::LIBRARY_CHANGED));
    assert!(
        dir.path().join("quay-lights.wav").exists(),
        "the audio files stay"
    );
    assert!(!recordings.sync_step(&host), "no work is left for it");
}
