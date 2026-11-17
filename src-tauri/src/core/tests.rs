//! Core through its public interface: a temporary library folder, a fake engine, a silent
//! audio sink and a recorder for the events the UI would get.

use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aoide_library::Library;
use aoide_tts::testing::FakeEngine;

use super::notices::testing::Recorded;
use super::preparation::Preparation;
use super::settings::{AppSettingsPatch, LibrarySort};
use super::{BookSettings, Core, CoreEvent, Environment, Events, Paths, PrepareStage};

#[derive(Default)]
pub(super) struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

impl Events for Recorder {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((name.into(), payload));
    }
}

impl Recorder {
    pub(super) fn count(&self, name: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == name)
            .count()
    }

    /// The payloads of the events named `name`, in order.
    pub(super) fn payloads(&self, name: &str) -> Vec<serde_json::Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, payload)| payload.clone())
            .collect()
    }
}

pub(super) fn paths(root: &Path) -> Paths {
    Paths {
        home_dir: root.join("Home"),
        library_dir: root.join("Books"),
        covers_dir: root.join("data/covers"),
        audio_dir: root.join("cache/audio"),
        db_path: root.join("data/library.sqlite3"),
        config_dir: root.join("config"),
        models_dir: root.join("data/models"),
        articles_dir: root.join("data/articles"),
        browser_dir: root.join("data/browser"),
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
pub(super) fn epub_with(one: String, two: String) -> Vec<u8> {
    epub_with_subjects(one, two, &[])
}

/// [`epub_with`], its package listing `subjects`.
pub(super) fn epub_with_subjects(one: String, two: String, subjects: &[&str]) -> Vec<u8> {
    epub_of(&[one, two], subjects)
}

/// A book of one chapter per titled text, each a heading and a paragraph.
pub(super) fn epub_of_chapters(chapters: &[(&str, &str)]) -> Vec<u8> {
    let documents: Vec<String> = chapters
        .iter()
        .map(|(title, body)| {
            format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>{title}</h1><p>{body}</p></body></html>"#
            )
        })
        .collect();
    epub_of(&documents, &[])
}

/// A book with front and back matter around two chapters.
pub(super) fn book_with_matter() -> Vec<u8> {
    epub_of_chapters(&[
        ("Contents", "Two chapters and a page of notes."),
        (
            "The Quay",
            "Gulls argued over the nets. The tide came in slowly.",
        ),
        (
            "The Lamp",
            "Someone lit the lamp at dusk. It burned until morning.",
        ),
        ("Notes", "Quay: a stone landing where boats tie up."),
    ])
}

/// The test book's package with these chapter documents, listing `subjects`.
fn epub_of(documents: &[String], subjects: &[&str]) -> Vec<u8> {
    let subjects: String = subjects
        .iter()
        .map(|s| format!("<dc:subject>{s}</dc:subject>"))
        .collect();
    // The first two named as they always were, so the two-chapter book keeps its id.
    let names: Vec<String> = (0..documents.len())
        .map(|i| match i {
            0 => "one".to_string(),
            1 => "two".to_string(),
            i => format!("c{i}"),
        })
        .collect();
    let manifest: String = names
        .iter()
        .map(|n| format!(r#"<item id="{n}" href="{n}.xhtml" media-type="application/xhtml+xml"/>"#))
        .collect();
    let spine: String = names
        .iter()
        .map(|n| format!(r#"<itemref idref="{n}"/>"#))
        .collect();
    let mut files = vec![
        ("mimetype".to_string(), "application/epub+zip".to_string()),
        ("META-INF/container.xml".into(), r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.into()),
        ("OPS/book.opf".into(), format!(r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Quay Lights</dc:title><dc:creator>Test Author</dc:creator><dc:language>en</dc:language>{subjects}</metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#)),
    ];
    for (name, document) in names.iter().zip(documents) {
        files.push((format!("OPS/{name}.xhtml"), document.clone()));
    }
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in files {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// A library folder holding the test book, with the fake engine selected. Returns the book id.
pub(super) fn library_folder(root: &Path, settings: &str) -> String {
    library_folder_with(root, settings, "voiceId = \"Test.wav\"")
}

pub(super) fn library_folder_with(root: &Path, settings: &str, voice: &str) -> String {
    library_folder_holding(root, settings, voice, &epub())
}

/// [`library_folder_with`], the library holding `bytes` as its one book.
pub(super) fn library_folder_holding(
    root: &Path,
    settings: &str,
    voice: &str,
    bytes: &[u8],
) -> String {
    std::fs::create_dir_all(root.join("Books")).unwrap();
    std::fs::write(root.join("Books/quay-lights.epub"), bytes).unwrap();
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
    aoide_library::content_hash(bytes).0
}

fn start(root: &Path, engine: &Arc<FakeEngine>) -> (Arc<Core>, Arc<Recorder>) {
    start_with(
        root,
        engine,
        Arc::new(|_| Err("no speech recognition in this test".into())),
    )
}

pub(super) fn start_with(
    root: &Path,
    engine: &Arc<FakeEngine>,
    transcribers: super::recording::TranscriberFactory,
) -> (Arc<Core>, Arc<Recorder>) {
    start_full(
        root,
        engine,
        transcribers,
        super::articles::offline(),
        Arc::new(super::browsing::testing::ScriptedBrowser::default()),
    )
}

/// A core whose articles are fetched by `fetcher`.
pub(super) fn start_fetching(
    root: &Path,
    engine: &Arc<FakeEngine>,
    fetcher: Arc<dyn super::articles::Fetcher>,
) -> (Arc<Core>, Arc<Recorder>) {
    start_browsing(root, engine, fetcher, Arc::default())
}

/// A core whose articles are fetched by `fetcher`, and rendered by `browser`.
pub(super) fn start_browsing(
    root: &Path,
    engine: &Arc<FakeEngine>,
    fetcher: Arc<dyn super::articles::Fetcher>,
    browser: Arc<super::browsing::testing::ScriptedBrowser>,
) -> (Arc<Core>, Arc<Recorder>) {
    start_full(
        root,
        engine,
        Arc::new(|_| Err("no speech recognition in this test".into())),
        fetcher,
        browser,
    )
}

fn start_full(
    root: &Path,
    engine: &Arc<FakeEngine>,
    transcribers: super::recording::TranscriberFactory,
    fetcher: Arc<dyn super::articles::Fetcher>,
    browser: Arc<dyn super::browsing::BrowserDriver>,
) -> (Arc<Core>, Arc<Recorder>) {
    start_downloading(root, engine, transcribers, fetcher, browser, None)
}

/// A core that downloads the browser from `browser_download`.
pub(super) fn start_downloading(
    root: &Path,
    engine: &Arc<FakeEngine>,
    transcribers: super::recording::TranscriberFactory,
    fetcher: Arc<dyn super::articles::Fetcher>,
    browser: Arc<dyn super::browsing::BrowserDriver>,
    browser_download: Option<super::browser_download::Source>,
) -> (Arc<Core>, Arc<Recorder>) {
    start_environment(
        root,
        engine,
        transcribers,
        fetcher,
        browser,
        browser_download,
        Arc::default(),
        super::environment::Clock::default(),
    )
}

/// A core whose notices are kept by the returned notifier (the window not in front), timing
/// jobs with a clock that moves on a minute and a second each time it is read: every job is
/// a long one.
pub(super) fn start_noticing(
    root: &Path,
    engine: &Arc<FakeEngine>,
) -> (Arc<Core>, Arc<Recorder>, Arc<Recorded>) {
    let notifier = Arc::new(Recorded::default());
    let (core, events) = start_environment(
        root,
        engine,
        Arc::new(|_| Err("no speech recognition in this test".into())),
        super::articles::offline(),
        Arc::new(super::browsing::testing::ScriptedBrowser::default()),
        None,
        Arc::clone(&notifier),
        super::environment::Clock::stepping(Duration::from_secs(61)),
    );
    (core, events, notifier)
}

#[allow(clippy::too_many_arguments)]
fn start_environment(
    root: &Path,
    engine: &Arc<FakeEngine>,
    transcribers: super::recording::TranscriberFactory,
    fetcher: Arc<dyn super::articles::Fetcher>,
    browser: Arc<dyn super::browsing::BrowserDriver>,
    browser_download: Option<super::browser_download::Source>,
    notifier: Arc<Recorded>,
    clock: super::environment::Clock,
) -> (Arc<Core>, Arc<Recorder>) {
    let events = Arc::new(Recorder::default());
    let mut engines = aoide_tts::Registry::new();
    engine.register(&mut engines);
    let shown = Arc::clone(&events);
    let opened = Arc::clone(&events);
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
        open_link: Arc::new(move |url| {
            opened.emit("test://opened-link", serde_json::json!(url));
            Ok(())
        }),
        transcribers,
        fetcher,
        browser,
        browser_download,
        notifier,
        clock,
    };
    (Core::start(environment, paths(root)).unwrap(), events)
}

/// Waits (up to a few seconds) for `check` to hold, letting Core's tasks run.
pub(super) async fn until(what: &str, check: impl Fn() -> bool) {
    for _ in 0..400 {
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
async fn a_book_prepared_while_aoide_is_not_in_front_ends_with_one_notice_that_opens_it() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events, notifier) = start_noticing(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    until("the notice", || !notifier.sent().is_empty()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let sent = notifier.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].title, "Quay Lights");
    assert_eq!(sent[0].opens.as_deref(), Some(id.as_str()));
}

#[tokio::test]
async fn exporting_an_unprepared_book_ends_with_one_notice_not_two() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events, notifier) = start_noticing(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    let exported = core.export_book(&id).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let sent = notifier.sent();
    assert_eq!(sent.len(), 1, "{sent:?}");
    match exported {
        Ok(_) => assert_eq!(sent[0].body, "Exported to the Exports folder."),
        // Exporting needs ffmpeg, which a test machine may not have.
        Err(error) => {
            assert!(error.contains("ffmpeg"), "{error}");
            assert_eq!(sent[0].body, "The export failed.");
        }
    }
    assert_eq!(sent[0].opens.as_deref(), Some(id.as_str()));
}

#[tokio::test]
async fn with_notifications_turned_off_a_book_prepared_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events, notifier) = start_noticing(dir.path(), &engine);
    core.update_app_settings(AppSettingsPatch {
        notices: Some(false),
        ..Default::default()
    })
    .unwrap();
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(notifier.sent(), []);
}

#[tokio::test]
async fn a_book_prepared_while_aoide_is_in_front_says_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events, notifier) = start_noticing(dir.path(), &engine);
    notifier.set_focused(true);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(notifier.sent(), []);
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
        .set_book_value(
            &aoide_library::BookId(id.clone()),
            aoide_library::BookKey::Settings,
            &serde_json::json!({ "voiceId": "Abigail.wav" }),
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

#[tokio::test]
async fn preparing_one_chapter_makes_only_its_audio_and_reports_it_per_chapter() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    assert!(
        core.prepare_chapter(&id, 9).await.is_err(),
        "no such chapter"
    );

    core.prepare_chapter(&id, 1).await.unwrap();
    until("the chapter to finish", || {
        !engine.requests().is_empty() && core.prepare_view(&id).is_none()
    })
    .await;

    let said: Vec<String> = engine.requests().into_iter().map(|r| r.text).collect();
    assert!(
        said.iter()
            .all(|t| !t.contains("Quay") && !t.contains("Gulls")),
        "{said:?}"
    );
    assert!(said.iter().any(|t| t.contains("Lamp")), "{said:?}");
    let chapters = core.chapter_audio(&id).await.unwrap();
    assert_eq!(chapters.len(), 2);
    assert_eq!(chapters[0].done, 0);
    assert!(
        chapters[1].total > 0 && chapters[1].done == chapters[1].total,
        "{chapters:?}"
    );
    assert_eq!(
        stage(&core, &id),
        None,
        "one chapter does not make the book ready"
    );
}

#[tokio::test]
async fn volume_is_reported_saved_and_restored_with_the_speed() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "[player]\nspeed = 1.5\n");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    assert_eq!(core.player_state().volume, 1.0, "full volume by default");

    core.set_volume(0.4);
    assert_eq!(core.player_state().volume, 0.4);
    assert_eq!(core.player_state().speed, 1.5, "speed kept");
    drop(core);

    let (core, _events) = start(dir.path(), &engine);
    assert_eq!(core.player_state().volume, 0.4);
    assert_eq!(core.player_state().speed, 1.5);
}

#[tokio::test]
async fn patches_to_different_app_settings_both_survive() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    let limit = AppSettingsPatch {
        cache_limit_bytes: Some(Some(5_000_000_000)),
        ..Default::default()
    };
    core.update_app_settings(limit).unwrap();

    // A second window's change, sent without having read the first.
    let merged = core
        .update_app_settings(AppSettingsPatch {
            library_sort: Some(LibrarySort::Title),
            ..Default::default()
        })
        .unwrap();

    assert_eq!(merged.cache_limit_bytes, Some(5_000_000_000));
    assert_eq!(merged.library_sort, LibrarySort::Title);
    assert_eq!(core.app_settings(), merged);
    let cleared = core
        .update_app_settings(serde_json::from_str(r#"{"cacheLimitBytes":null}"#).unwrap())
        .unwrap();
    assert_eq!(cleared.cache_limit_bytes, None, "null clears a setting");
    assert_eq!(cleared.library_sort, LibrarySort::Title);
    let invalid = core.update_app_settings(
        serde_json::from_str(r#"{"aligner":{"baseUrl":"not a url"}}"#).unwrap(),
    );
    assert!(invalid.is_err());
    assert_eq!(
        core.app_settings(),
        cleared,
        "an invalid patch changes nothing"
    );
}

#[tokio::test]
async fn a_change_that_cannot_be_saved_in_the_background_is_reported_once() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, events) = start(dir.path(), &engine);
    let config = dir.path().join("config");
    let writable = |on: bool| {
        use std::os::unix::fs::PermissionsExt;
        let mode = if on { 0o755 } else { 0o555 };
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(mode)).unwrap();
    };
    writable(false);

    core.set_volume(0.4);
    core.set_volume(0.5);
    writable(true);

    let problems = events.payloads(CoreEvent::PROBLEM);
    assert_eq!(
        problems.len(),
        1,
        "the same failure is told once: {problems:?}"
    );
    let problem = &problems[0];
    assert!(
        problem["message"]
            .as_str()
            .unwrap()
            .starts_with("Could not save the player settings"),
        "{problem}"
    );
    assert_eq!(problem["error"]["kind"], "io");
}

// ------------------------------------------------------------ saving the place

/// Core playing chapter `chapter` of the test book, with nothing saved yet; and what is saved
/// (the chapter) once asked.
async fn playing_chapter(
    root: &Path,
    chapter: usize,
) -> (Arc<Core>, String, impl Fn(&Core) -> Option<u32>) {
    let engine = super::engine_tests::Scripted::new(
        "main",
        aoide_tts::EngineStatus::Ready(None),
        &["A.wav"],
        false,
    );
    let (core, _events, id) =
        super::engine_tests::start(root, &[&engine], "voiceId = \"A.wav\"").await;
    core.play(&id, Some(chapter), None).await.unwrap();
    let book = id.clone();
    let saved = move |core: &Core| {
        core.book_overview(&book)
            .unwrap()
            .progress
            .map(|p| p.chapter)
    };
    assert_eq!(saved(&core), None, "playing alone saves nothing at once");
    (core, id, saved)
}

#[tokio::test]
async fn pausing_saves_the_place() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _id, saved) = playing_chapter(dir.path(), 1).await;
    core.pause();
    assert_eq!(saved(&core), Some(1));
}

#[tokio::test]
async fn stopping_saves_the_place() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _id, saved) = playing_chapter(dir.path(), 1).await;
    core.stop();
    assert_eq!(saved(&core), Some(1));
}

#[tokio::test]
async fn moving_the_playhead_saves_the_new_place() {
    let dir = tempfile::tempdir().unwrap();
    let (core, _id, saved) = playing_chapter(dir.path(), 1).await;
    core.skip_chapter(-1).await.unwrap();
    assert_eq!(saved(&core), Some(0));
    core.stop();
}

// ------------------------------------------------------------ what screens show

#[tokio::test]
async fn a_book_listened_to_has_speech_and_moves_from_up_next_to_continue() {
    let dir = tempfile::tempdir().unwrap();
    let engine = super::engine_tests::Scripted::new(
        "main",
        aoide_tts::EngineStatus::Ready(None),
        &["A.wav"],
        false,
    );
    let (core, _events, id) =
        super::engine_tests::start(dir.path(), &[&engine], "voiceId = \"A.wav\"").await;
    let audio = |core: &Core| core.list_books().unwrap()[0].audio;
    assert_eq!(audio(&core), super::facets::AudioSource::None);
    let home = core.home().unwrap();
    assert_eq!(
        (home.continue_ids, home.up_next_ids),
        (vec![], vec![id.clone()])
    );

    core.play(&id, Some(1), None).await.unwrap();
    core.pause();
    let home = core.home().unwrap();
    assert_eq!(
        (home.continue_ids, home.up_next_ids),
        (vec![id.clone()], vec![])
    );
    until("speech audio for the book", || {
        audio(&core) == super::facets::AudioSource::Speech
    })
    .await;
}

#[test]
fn the_first_poisoned_lock_is_reported_once() {
    let events = Arc::new(Recorder::default());
    let reporter = super::error::Reporter::new(Arc::clone(&events) as Arc<dyn Events>);
    let told = std::sync::atomic::AtomicBool::new(false);
    let mutex = Arc::new(Mutex::new(1));
    let held = Arc::clone(&mutex);
    let _ = std::thread::spawn(move || {
        let _guard = held.lock().unwrap();
        panic!("poisoning the lock on purpose");
    })
    .join();

    for _ in 0..2 {
        let guard = mutex
            .lock()
            .unwrap_or_else(|poisoned| super::recover(poisoned, &told, Some(&reporter)));
        assert_eq!(*guard, 1, "the data is kept");
    }

    let problems = events.payloads(CoreEvent::PROBLEM);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0]["error"]["kind"], "internal");
}

/// A library in memory holding books `ids` (rows only, no files).
pub(super) fn library_with(ids: &[&str]) -> Arc<Mutex<Library>> {
    let library = Library::open_in_memory().unwrap();
    for id in ids {
        library
            .upsert_book(
                &aoide_library::NewBook {
                    id: aoide_library::BookId((*id).into()),
                    path: format!("/books/{id}.epub").into(),
                    title: Some(format!("Book {id}")),
                    authors: vec![],
                    language: None,
                    cover_path: None,
                    error: None,
                    kind: aoide_library::ItemKind::Book,
                },
                0,
            )
            .unwrap();
    }
    Arc::new(Mutex::new(library))
}

#[tokio::test]
async fn a_new_book_is_read_silently_and_a_narrated_one_is_listened_to() {
    use super::{ReadingMode, Shape};
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let view = core.book_overview(&id).unwrap();
    assert_eq!((view.mode, view.shape), (ReadingMode::Read, Shape::Read));
    let summary = &core.list_books().unwrap()[0];
    assert_eq!(
        (summary.mode, summary.shape),
        (ReadingMode::Read, Shape::Read)
    );

    // Speech made for it ahead of time means it was set up for listening.
    core.prepare_book(&id).await.unwrap();
    until("preparation to finish", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    let view = core.book_overview(&id).unwrap();
    assert_eq!(
        (view.mode, view.shape),
        (ReadingMode::Listen, Shape::ReadAlong)
    );

    // The user's choice wins over the default, and survives a restart.
    core.set_mode(&id, ReadingMode::Read).unwrap();
    assert_eq!(core.book_overview(&id).unwrap().mode, ReadingMode::Read);
    drop(core);
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let view = core.book_overview(&id).unwrap();
    assert_eq!((view.mode, view.shape), (ReadingMode::Read, Shape::Read));
    assert_eq!(core.list_books().unwrap()[0].mode, ReadingMode::Read);
}

#[tokio::test]
async fn playing_a_book_listens_to_it_and_reading_it_pauses_it() {
    use super::{PlaybackStatus, ReadingMode};
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;

    core.play(&id, Some(0), None).await.unwrap();
    assert_eq!(core.book_overview(&id).unwrap().mode, ReadingMode::Listen);
    assert_ne!(core.player_state().status, PlaybackStatus::Paused);

    core.set_mode(&id, ReadingMode::Read).unwrap();
    assert_eq!(core.book_overview(&id).unwrap().mode, ReadingMode::Read);
    assert_eq!(core.player_state().status, PlaybackStatus::Paused);

    // Choosing to listen does not start playback by itself.
    core.set_mode(&id, ReadingMode::Listen).unwrap();
    assert_eq!(core.player_state().status, PlaybackStatus::Paused);
    core.stop();
}

/// The ids of the segments read aloud in chapter `chapter` of book `id`.
fn spoken(core: &Core, id: &str, chapter: usize) -> Vec<String> {
    core.chapter(id, chapter)
        .unwrap()
        .blocks
        .into_iter()
        .flat_map(|b| b.segments)
        .filter(|s| s.speakable)
        .map(|s| s.id)
        .collect()
}

#[tokio::test]
async fn reading_silently_saves_the_place_and_the_last_sentence_finishes_the_book() {
    use super::ReadState;
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let read_state = |core: &Core| core.list_books().unwrap()[0].read_state;
    assert_eq!(read_state(&core), ReadState::Unread);

    let second = spoken(&core, &id, 1);
    let progress = core.set_reading_place(&id, 1, &second[0]).unwrap().unwrap();
    assert_eq!(
        core.book_overview(&id).unwrap().progress,
        Some(progress.clone())
    );
    assert_eq!(
        (progress.chapter, progress.segment_id.as_deref()),
        (1, Some(second[0].as_str()))
    );
    assert!(
        progress.fraction > 0.0 && progress.fraction < 1.0,
        "{}",
        progress.fraction
    );
    assert_eq!(read_state(&core), ReadState::InProgress);
    assert!(
        core.set_reading_place(&id, 0, &second[0]).is_err(),
        "a segment of another chapter is not a place"
    );
    assert_eq!(core.home().unwrap().continue_ids, std::slice::from_ref(&id));

    // Reopened after a restart, the book is where the reader left it.
    drop(core);
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let progress = core.open_book(&id).unwrap().progress.unwrap();
    assert_eq!(progress.segment_id.as_deref(), Some(second[0].as_str()));

    core.set_reading_place(&id, 1, second.last().unwrap())
        .unwrap();
    assert_eq!(read_state(&core), ReadState::Finished);
    assert!(
        core.home().unwrap().continue_ids.is_empty(),
        "finished books are not continued"
    );
    // Reading an earlier page of a finished book keeps it finished.
    core.set_reading_place(&id, 0, &spoken(&core, &id, 0)[0])
        .unwrap();
    assert_eq!(read_state(&core), ReadState::Finished);

    core.mark_unread(&id).unwrap();
    assert_eq!(read_state(&core), ReadState::Unread);
    assert!(core.book_overview(&id).unwrap().progress.is_none());
    core.mark_finished(&id).unwrap();
    assert_eq!(read_state(&core), ReadState::Finished);
}

/// Saves the second spoken sentence of chapter 1 as book `id`'s place, `ago_ms` ago.
fn saved_place_ago(core: &Core, id: &str, ago_ms: i64) -> Vec<String> {
    let second = spoken(core, id, 1);
    let book = core.load_book(id).unwrap();
    let index = book.index_of(&second[1]).unwrap();
    let segment = book.segment(index).unwrap();
    super::lock(&core.library)
        .save_progress(
            &aoide_library::BookId(id.into()),
            &aoide_library::Progress {
                chapter: 1,
                char_offset: segment.range.start as u32,
                segment_id: Some(second[1].clone()),
                updated_at: super::now_ms() - ago_ms,
                position_ms: None,
            },
        )
        .unwrap();
    second
}

#[tokio::test]
async fn a_place_saved_hours_ago_is_played_from_a_little_earlier_and_one_just_saved_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let playing = |core: &Core| core.player_state().segment_id;

    let second = saved_place_ago(&core, &id, 2 * 60 * 60 * 1_000);
    core.play(&id, None, None).await.unwrap();
    assert_eq!(playing(&core).as_ref(), Some(&second[0]), "a sentence back");
    core.stop();

    saved_place_ago(&core, &id, 2_000);
    core.play(&id, None, None).await.unwrap();
    assert_eq!(playing(&core).as_ref(), Some(&second[1]), "where it was");
    core.stop();
}

#[tokio::test]
async fn with_stepping_back_turned_off_a_place_saved_hours_ago_is_played_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    assert!(core.app_settings().step_back_on_resume, "on by default");
    core.update_app_settings(AppSettingsPatch {
        step_back_on_resume: Some(false),
        ..Default::default()
    })
    .unwrap();

    let second = saved_place_ago(&core, &id, 2 * 60 * 60 * 1_000);
    core.play(&id, None, None).await.unwrap();
    assert_eq!(core.player_state().segment_id.as_ref(), Some(&second[1]));
    core.stop();
}

#[tokio::test]
async fn the_playing_book_keeps_its_place_and_playing_it_to_the_end_finishes_it() {
    use super::ReadState;
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let first = spoken(&core, &id, 0);
    let second = spoken(&core, &id, 1);
    core.play(&id, Some(1), Some(second[0].clone()))
        .await
        .unwrap();
    // Playback owns the place while the book plays: a reading place is ignored.
    assert_eq!(core.set_reading_place(&id, 0, &first[0]).unwrap(), None);
    until("playback to reach the end", || {
        core.list_books().unwrap()[0].read_state == ReadState::Finished
    })
    .await;
    let progress = core.book_overview(&id).unwrap().progress.unwrap();
    assert_eq!(progress.chapter, 1);
    core.stop();
}

#[tokio::test]
async fn reading_time_adds_up_in_activity() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    core.reading_activity(&id);
    tokio::time::sleep(Duration::from_millis(60)).await;
    core.reading_activity(&id);
    let stats = core.activity_stats().unwrap();
    assert!(stats.reading_ms >= 50, "{stats:?}");
    assert_eq!(stats.listening_ms, 0);
    assert_eq!(stats.by_book[0].book_id.0, id);
}

#[tokio::test]
async fn reading_speed_is_kept_within_its_range() {
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    assert_eq!(core.app_settings().reading_wpm, 230);
    let speed = |wpm| AppSettingsPatch {
        reading_wpm: Some(wpm),
        ..Default::default()
    };
    assert_eq!(
        core.update_app_settings(speed(300)).unwrap().reading_wpm,
        300
    );
    assert!(core.update_app_settings(speed(50)).is_err());
    assert_eq!(core.app_settings().reading_wpm, 300);
}

/// The made audio files of the library at `root`, by the index.
fn audio_files(root: &Path) -> std::collections::HashSet<std::path::PathBuf> {
    Library::open(&root.join("data/library.sqlite3"))
        .unwrap()
        .segment_files()
        .unwrap()
}

#[tokio::test]
async fn a_sentence_behind_the_playhead_said_again_is_made_again_alone_as_a_new_take() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let first = spoken(&core, &id, 0);
    let second = spoken(&core, &id, 1);
    core.prepare_book(&id).await.unwrap();
    until("the whole short book to be made", || {
        stage(&core, &id) == Some(PrepareStage::Done) && audio_files(dir.path()).len() == 4
    })
    .await;
    core.play(&id, Some(1), Some(second[1].clone()))
        .await
        .unwrap();
    core.pause();
    let before = audio_files(dir.path());
    let said = engine.requests()[0].clone();
    assert_eq!(said.take, 0);

    core.say_again(&id, &first[1]).await.unwrap();

    let state = core.player_state();
    assert_eq!(state.segment_id.as_deref(), Some(first[1].as_str()));
    assert_eq!(state.status, super::PlaybackStatus::Paused, "pause is kept");
    until("the sentence to be made again", || {
        engine.calls().len() == 5 && audio_files(dir.path()).len() == 4
    })
    .await;
    let again = engine.requests()[4].clone();
    let original = engine
        .requests()
        .into_iter()
        .find(|r| r.text == again.text)
        .unwrap();
    assert!(again.text.starts_with("Gulls argued"), "{again:?}");
    assert_eq!(again.take, 1);
    assert_eq!(original.take, 0);
    let after = audio_files(dir.path());
    let kept: Vec<_> = before.intersection(&after).collect();
    assert_eq!(kept.len(), 3, "no other audio is touched");
    let gone = before.difference(&after).next().unwrap();
    assert!(!dir.path().join("cache/audio").join(gone).exists());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(engine.calls().len(), 5, "{:?}", engine.calls());
    assert_eq!(
        stage(&core, &id),
        Some(PrepareStage::Done),
        "the book is still prepared"
    );
    core.stop();
}

#[tokio::test]
async fn saying_again_in_a_book_not_listened_to_listens_to_it_from_that_sentence() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let second = spoken(&core, &id, 1);

    core.say_again(&id, &second[1]).await.unwrap();

    let state = core.player_state();
    assert_eq!(state.book_id.as_deref(), Some(id.as_str()));
    assert_eq!(state.segment_id.as_deref(), Some(second[1].as_str()));
    until("the sentence to be made", || {
        engine
            .requests()
            .iter()
            .any(|r| r.text.starts_with("Someone lit") && r.take == 1)
    })
    .await;
    assert!(
        core.say_again(&id, "99:0:0").await.is_err(),
        "no such sentence"
    );
    core.stop();
}

#[tokio::test]
async fn a_prepared_book_with_a_sentence_said_again_still_exports() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let first = spoken(&core, &id, 0);
    core.prepare_book(&id).await.unwrap();
    until("the whole short book to be made", || {
        stage(&core, &id) == Some(PrepareStage::Done) && audio_files(dir.path()).len() == 4
    })
    .await;

    core.say_again(&id, &first[0]).await.unwrap();
    core.pause();
    until("the new take to be made", || {
        engine.requests().iter().any(|r| r.take == 1) && audio_files(dir.path()).len() == 4
    })
    .await;

    assert_eq!(stage(&core, &id), Some(PrepareStage::Done));
    match core.export_book(&id).await {
        Ok(path) => assert!(Path::new(&path).exists(), "{path}"),
        // Exporting needs ffmpeg, which a test machine may not have.
        Err(error) => assert!(error.contains("ffmpeg"), "{error}"),
    }
    assert_eq!(
        engine.requests().len(),
        5,
        "the export made nothing again: {:?}",
        engine.calls()
    );
    core.stop();
}

#[tokio::test]
async fn a_pause_changed_replans_the_book_listened_to_without_asking_for_speech_again() {
    use super::settings::PauseSettings;
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let second = spoken(&core, &id, 1);
    core.prepare_book(&id).await.unwrap();
    until("the whole short book to be made", || {
        stage(&core, &id) == Some(PrepareStage::Done) && audio_files(dir.path()).len() == 4
    })
    .await;
    core.play(&id, Some(1), Some(second[1].clone()))
        .await
        .unwrap();
    core.pause();
    let pauses = |core: &Core| -> Vec<u32> {
        let book = core.playback.book().unwrap();
        (0..book.total_entries())
            .map(|i| book.segment(i).unwrap().pause_after_ms)
            .collect()
    };
    assert_eq!(pauses(&core), [700, 700, 700, 700]);
    let asked = engine.requests().len();

    let change = |pauses| AppSettingsPatch {
        pauses: Some(pauses),
        ..Default::default()
    };
    let longer = PauseSettings {
        paragraph_ms: 900,
        break_ms: 2000,
        heading_ms: 400,
    };
    core.update_app_settings(change(longer.clone())).unwrap();

    assert_eq!(core.app_settings().pauses, longer);
    assert_eq!(pauses(&core), [900, 900, 900, 900]);
    let state = core.player_state();
    assert_eq!(state.segment_id.as_deref(), Some(second[1].as_str()));
    assert_eq!(state.status, super::PlaybackStatus::Paused, "pause is kept");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(engine.requests().len(), asked, "{:?}", engine.calls());
    assert_eq!(stage(&core, &id), Some(PrepareStage::Done));

    let too_long = PauseSettings {
        break_ms: 3001,
        ..longer.clone()
    };
    assert!(core.update_app_settings(change(too_long)).is_err());
    assert_eq!(core.app_settings().pauses, longer);
    core.stop();
}

#[tokio::test]
async fn hearing_the_pauses_says_three_short_paragraphs_with_the_pauses_on_screen_between_them() {
    use super::settings::PauseSettings;
    let dir = tempfile::tempdir().unwrap();
    library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, _events) = start(dir.path(), &engine);
    let pauses = PauseSettings {
        paragraph_ms: 500,
        break_ms: 1000,
        heading_ms: 3000,
    };

    let wav = core.preview_pauses(pauses).await.unwrap();

    let said: Vec<String> = engine.requests().into_iter().map(|r| r.text).collect();
    assert_eq!(said.len(), 3, "{said:?}");
    assert!(
        engine.requests().iter().all(|r| r.voice_id == "Test.wav"),
        "in the voice books are read in"
    );
    let audio = aoide_tts::decode_wav(&wav).unwrap();
    // The fake engine says each paragraph in 300 ms; the last is followed by nothing.
    assert_eq!(audio.duration_ms(), 3 * 300 + 500 + 1000);
    assert_eq!(
        core.app_settings().pauses,
        PauseSettings::default(),
        "hearing saves nothing"
    );
}

#[tokio::test]
async fn an_archived_item_leaves_home_and_comes_back_when_taken_out() {
    let dir = tempfile::tempdir().unwrap();
    let id = library_folder(dir.path(), "");
    let engine = FakeEngine::new();
    let (core, events) = start(dir.path(), &engine);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    let archived = |core: &Core| core.list_books().unwrap()[0].archived;
    assert!(!archived(&core));
    assert_eq!(core.home().unwrap().up_next_ids, [id.as_str()]);
    let changes = events.count("library://changed");

    core.archive(&id, true).unwrap();
    assert!(archived(&core));
    assert_eq!(core.home().unwrap(), super::HomeView::default());
    assert_eq!(events.count("library://changed"), changes + 1);
    // Archiving it again changes nothing, and says nothing.
    core.archive(&id, true).unwrap();
    assert_eq!(events.count("library://changed"), changes + 1);

    core.archive(&id, false).unwrap();
    assert!(!archived(&core));
    assert_eq!(core.home().unwrap().up_next_ids, [id.as_str()]);
    assert!(core.archive("no-such-item", true).is_err());
}
