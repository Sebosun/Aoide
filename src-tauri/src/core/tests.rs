//! Core through its public interface: a temporary library folder, a fake engine, a silent
//! audio sink and a recorder for the events the UI would get.

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_library::Library;
use aoide_tts::testing::FakeEngine;

use super::preparation::Preparation;
use super::{BookSettings, Core, CoreEvent, Environment, Events, Paths, PrepareStage};

#[derive(Default)]
struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

impl Events for Recorder {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((name.into(), payload));
    }
}

impl Recorder {
    fn count(&self, name: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == name)
            .count()
    }
}

fn paths(root: &Path) -> Paths {
    Paths {
        home_dir: root.join("Home"),
        library_dir: root.join("Books"),
        covers_dir: root.join("data/covers"),
        audio_dir: root.join("cache/audio"),
        db_path: root.join("data/library.sqlite3"),
        config_dir: root.join("config"),
    }
}

/// A two-chapter book with original text: four segments to speak (two headings, two paragraphs).
pub(super) fn epub() -> Vec<u8> {
    let chapter = |title: &str, body: &str| {
        format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>{title}</h1><p>{body}</p></body></html>"#
        )
    };
    epub_with(
        chapter(
            "The Quay",
            "Gulls argued over the nets. The tide came in slowly.",
        ),
        chapter(
            "The Lamp",
            "Someone lit the lamp at dusk. It burned until morning.",
        ),
    )
}

/// The test book's package with these two chapter documents.
fn epub_with(one: String, two: String) -> Vec<u8> {
    let files = [
        ("mimetype", "application/epub+zip".to_string()),
        ("META-INF/container.xml", r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into()),
        ("OPS/book.opf", r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Quay Lights</dc:title><dc:creator>Test Author</dc:creator><dc:language>en</dc:language></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/><itemref idref="two"/></spine></package>"#.into()),
        ("OPS/one.xhtml", one),
        ("OPS/two.xhtml", two),
    ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A library folder holding the test book, with the fake engine selected. Returns the book id.
fn library_folder(root: &Path, settings: &str) -> String {
    library_folder_with(root, settings, "voiceId = \"Test.wav\"")
}

fn library_folder_with(root: &Path, settings: &str, voice: &str) -> String {
    let bytes = epub();
    std::fs::create_dir_all(root.join("Books")).unwrap();
    std::fs::write(root.join("Books/quay-lights.epub"), &bytes).unwrap();
    std::fs::create_dir_all(root.join("config")).unwrap();
    let engine = r#"
[engine]
id = "test"
name = "Test engine"
kind = "fake"
baseUrl = "http://127.0.0.1:9"
network = "loopback"
"#;
    std::fs::write(
        root.join("config/settings.toml"),
        format!("{settings}\n{engine}{voice}\n"),
    )
    .unwrap();
    aoide_library::content_hash(&bytes).0
}

fn start(root: &Path, engine: &Arc<FakeEngine>) -> (Arc<Core>, Arc<Recorder>) {
    let events = Arc::new(Recorder::default());
    let mut engines = aoide_tts::Registry::new();
    engine.register(&mut engines);
    let shown = Arc::clone(&events);
    let environment = Environment {
        events: Arc::clone(&events) as Arc<dyn Events>,
        engines,
        audio: Environment::silent_audio(),
        background: false,
        // Recorded as an event instead of opening a real file manager.
        show_in_folder: Arc::new(move |path| {
            shown.emit("test://shown-in-folder", serde_json::json!(path));
            Ok(())
        }),
    };
    (Core::start(environment, paths(root)).unwrap(), events)
}

/// Waits (up to a few seconds) for `check` to hold, letting Core's tasks run.
async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..200 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out waiting for {what}");
}

fn stage(core: &Core, id: &str) -> Option<PrepareStage> {
    core.prepare_view(id).map(|view| view.state)
}

/// What an earlier run leaves behind when it quits with the book still queued.
fn queued_by_an_earlier_run(root: &Path, id: &str) {
    std::fs::create_dir_all(root.join("data")).unwrap();
    let library = Library::open(&root.join("data/library.sqlite3")).unwrap();
    Preparation::default().enqueue(&library, id);
}

#[tokio::test]
async fn prepares_a_queued_book_with_the_engine_and_reports_it_done() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    let view = core.prepare_view(&id).unwrap();
    assert_eq!((view.done, view.total), (4, 4));
    assert_eq!(engine.calls().len(), 4, "{:?}", engine.calls());
    assert!(
        engine.calls().iter().any(|t| t.starts_with("Gulls argued")),
        "{:?}",
        engine.calls()
    );
    assert!(events.count(CoreEvent::LIBRARY_CHANGED) >= 2);
}

#[tokio::test]
async fn a_queue_left_by_an_earlier_run_carries_on_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    queued_by_an_earlier_run(dir.path(), &id);
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    assert_eq!(engine.calls().len(), 4, "{:?}", engine.calls());
}

#[tokio::test]
async fn with_resuming_turned_off_the_queue_waits_until_started() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "[app]\nresumePrepare = false\n");
    queued_by_an_earlier_run(dir.path(), &id);
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(stage(&core, &id), Some(PrepareStage::Waiting));
    assert!(engine.calls().is_empty());

    core.prepare_book(&id).await.unwrap(); // "Start" in the queue panel.
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
}

#[tokio::test]
async fn regenerating_the_book_being_played_makes_its_audio_again() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.play(&id, Some(0), None).await.unwrap();
    until("the whole short book to be made", || {
        engine.calls().len() == 4
    })
    .await;
    core.regenerate_book(&id).await.unwrap();
    // Before the fix the worker skipped keys it had made earlier in the run, and playback
    // waited for audio that never came.
    until("the audio to be made again", || engine.calls().len() == 8).await;
    core.stop();
}

#[tokio::test]
async fn the_voice_preview_uses_the_voice_the_book_will_be_read_in() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder_with(dir.path(), "", ""); // No voice chosen anywhere.
                                                      // The engine's first voice is not the one the app prefers.
    let engine = FakeEngine::with_voices(&["Alto.wav", "Emily.wav"]);
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    let profile = core.engine_options().profile;
    core.preview_voice(profile, None, Default::default(), Some("en".into()))
        .await
        .unwrap();
    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    let requests = engine.requests();
    let preview = &requests[0];
    assert!(
        requests[1..].iter().all(|r| r.voice_id == preview.voice_id
            && r.language == preview.language
            && r.params == preview.params),
        "{requests:?}"
    );
    assert_eq!(preview.voice_id, "Emily.wav");
}

#[tokio::test]
async fn a_book_voice_the_engine_lacks_is_read_with_its_default_and_flagged() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder_with(dir.path(), "", ""); // No voice chosen in the profile.
    let engine = FakeEngine::with_voices(&["Hazel.wav", "Arthur.wav"]);
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    // Chosen while another engine was active.
    let chatterbox_voice = BookSettings {
        voice_id: Some("Abigail.wav".into()),
        ..BookSettings::default()
    };
    core.save_book_settings(&id, chatterbox_voice)
        .await
        .unwrap();

    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    let voices: Vec<String> = engine.requests().into_iter().map(|r| r.voice_id).collect();
    assert!(
        !voices.is_empty() && voices.iter().all(|v| v == "Hazel.wav"),
        "{voices:?}"
    );
    let warning = core
        .book_overview(&id)
        .unwrap()
        .voice_warning
        .expect("a voice warning");
    assert!(
        warning.contains("Abigail") && warning.contains("Hazel"),
        "{warning}"
    );
    assert_eq!(
        core.book_settings(&id).voice_id.as_deref(),
        Some("Abigail.wav"),
        "the book keeps its choice for when that engine is back"
    );
}

#[tokio::test]
async fn a_queue_resumed_at_startup_does_not_send_a_voice_the_engine_lacks() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder_with(dir.path(), "", "voiceId = \"Emily.wav\"");
    queued_by_an_earlier_run(dir.path(), &id);
    let library = Library::open(&dir.path().join("data/library.sqlite3")).unwrap();
    library
        .set_setting(
            &format!("book.{id}.settings"),
            r#"{"voiceId":"Abigail.wav"}"#,
        )
        .unwrap();
    drop(library);
    let engine = FakeEngine::with_voices(&["Hazel.wav"]);
    let (core, _events) = start(dir.path(), &engine);
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    let voices: Vec<String> = engine.requests().into_iter().map(|r| r.voice_id).collect();
    assert!(voices.iter().all(|v| v == "Hazel.wav"), "{voices:?}");
}

#[tokio::test]
async fn a_book_without_readable_text_fails_to_import_and_says_why() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let only_pictures =
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><img src="a.png"/></body></html>"#;
    let source = dir.path().join("Pictures.epub");
    std::fs::write(
        &source,
        epub_with(only_pictures.into(), only_pictures.into()),
    )
    .unwrap();

    let results = core.import_files(vec![source.to_string_lossy().into_owned()]);

    assert_eq!(results[0].book_id, None);
    let error = results[0].error.as_deref().expect("an import error");
    assert!(error.contains("no readable text"), "{error}");
    let failed: Vec<_> = core
        .list_books()
        .unwrap()
        .into_iter()
        .filter(|b| matches!(b.status, aoide_library::BookStatus::Error(_)))
        .collect();
    assert_eq!(failed.len(), 1, "listed as failed, with its reason");
}

#[tokio::test]
async fn shows_a_book_file_in_the_file_manager_and_says_when_it_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.show_in_folder(&id).unwrap();
    let shown: Vec<_> = events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|(n, _)| n == "test://shown-in-folder")
        .map(|(_, p)| p.clone())
        .collect();
    assert_eq!(
        shown,
        vec![serde_json::json!(dir.path().join("Books/quay-lights.epub"))]
    );

    std::fs::remove_file(dir.path().join("Books/quay-lights.epub")).unwrap();
    let error = core.show_in_folder(&id).unwrap_err();
    assert!(error.contains("missing"), "{error}");
}
