//! Recordings through Core: attach, sync with a scripted transcriber, play in place of the
//! speech engine. The test recording is a WAV file whose samples encode their own time, so the
//! scripted transcriber knows which stretch it is hearing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aoide_transcribe::{Error, Request, Transcriber, Word};
use aoide_tts::testing::FakeEngine;

use super::recording::Narration;
use super::syncing::Stage;
use super::tests::{library_folder, start_with, until};
use super::{Core, CoreEvent, TranscriberFactory};

/// Sample value per millisecond of the recording.
const SCALE: f32 = 100_000.0;
const RATE: u32 = 16_000;

type Script = Arc<Mutex<Vec<(u64, u64, String)>>>;

/// Hears the words of the script that fall in the stretch it is given.
struct Scripted {
    script: Script,
    heard: Arc<Mutex<Vec<u64>>>,
}

impl Transcriber for Scripted {
    fn id(&self) -> &str {
        "script"
    }

    fn transcribe(&self, request: &Request<'_>) -> Result<Vec<Word>, Error> {
        let start = (request.samples[0] * SCALE).round() as u64;
        let end = start + request.samples.len() as u64 * 1_000 / u64::from(RATE);
        self.heard.lock().unwrap().push(start);
        Ok(self
            .script
            .lock()
            .unwrap()
            .iter()
            .filter(|(s, _, _)| (start..end).contains(s))
            .map(|(s, e, text)| Word {
                text: text.clone(),
                start_ms: s - start,
                end_ms: (*e).min(end) - start,
                probability: 1.0,
            })
            .collect())
    }
}

fn scripted() -> (TranscriberFactory, Script, Arc<Mutex<Vec<u64>>>) {
    let script: Script = Arc::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let (s, h) = (Arc::clone(&script), Arc::clone(&heard));
    let factory: TranscriberFactory = Arc::new(move |_| {
        Ok(Box::new(Scripted {
            script: Arc::clone(&s),
            heard: Arc::clone(&h),
        }) as Box<dyn Transcriber>)
    });
    (factory, script, heard)
}

/// `texts` read aloud from `from_ms`, 400 ms a word.
fn read(texts: &[String], from_ms: u64) -> Vec<(u64, u64, String)> {
    let mut at = from_ms;
    let mut out = Vec::new();
    for text in texts {
        for word in text.split_whitespace() {
            out.push((at, at + 350, word.to_owned()));
            at += 400;
        }
        at += 600;
    }
    out
}

/// A mono 32-bit float WAV of `total_ms` whose every sample is its own time over [`SCALE`].
fn write_recording(path: &Path, total_ms: u64) {
    let frames = total_ms * u64::from(RATE) / 1_000;
    let mut data = Vec::with_capacity(frames as usize * 4);
    for n in 0..frames {
        let ms = n as f32 * 1_000.0 / RATE as f32;
        data.extend_from_slice(&(ms / SCALE).to_le_bytes());
    }
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 4).to_le_bytes());
    wav.extend_from_slice(&4u16.to_le_bytes());
    wav.extend_from_slice(&32u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
    wav.extend_from_slice(&data);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, wav).unwrap();
}

fn put_model(root: &Path) {
    std::fs::create_dir_all(root.join("data/models")).unwrap();
    std::fs::write(root.join("data/models/ggml-base.en.bin"), b"model").unwrap();
}

fn stage(core: &Core, id: &str) -> Option<Stage> {
    core.recording(id).map(|r| r.stage)
}

fn sync(core: &Core) {
    while core.sync_step() {}
}

fn texts(core: &Core, id: &str) -> Vec<String> {
    let book = core.load_book(id).unwrap();
    book.chapters.iter().map(|c| c.text.clone()).collect()
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    id: String,
    core: Arc<Core>,
    events: Arc<super::tests::Recorder>,
    engine: Arc<FakeEngine>,
    script: Script,
    heard: Arc<Mutex<Vec<u64>>>,
    audio: PathBuf,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    let id = library_folder(&root, "");
    let engine = FakeEngine::new();
    let (factory, script, heard) = scripted();
    let (core, events) = start_with(&root, &engine, factory);
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 1)
    })
    .await;
    Fixture {
        audio: root.join("Audio/quay-lights.wav"),
        _dir: dir,
        root,
        id,
        core,
        events,
        engine,
        script,
        heard,
    }
}

#[tokio::test]
async fn a_recording_syncs_and_then_plays_in_place_of_the_speech_engine() {
    let f = fixture().await;
    // A second of credits the text lacks, then the book read aloud.
    let mut words = vec![(0, 300, "Read".to_owned()), (400, 700, "aloud".to_owned())];
    words.extend(read(&texts(&f.core, &f.id), 1_000));
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);

    let found = f
        .core
        .inspect_recording(vec![f.audio.to_string_lossy().into()]);
    assert_eq!((found.files.len(), found.total_ms), (1, total));
    let view = f
        .core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    assert_eq!(view.stage, Stage::Checking);

    sync(&f.core);
    assert_eq!(
        stage(&f.core, &f.id),
        Some(Stage::NeedsModel),
        "nothing is transcribed without the model"
    );
    put_model(&f.root);
    f.core.models_changed();
    sync(&f.core);

    let view = f.core.recording(&f.id).unwrap();
    assert_eq!(view.stage, Stage::Ready, "{view:?}");
    assert_eq!(view.transcribed_ms, total);
    let summary = view.summary.unwrap();
    assert_eq!(summary.matched_words, summary.words, "{summary:?}");
    assert!(f.events.count(CoreEvent::SYNC_STATE) >= 4);
    let report = f.core.sync_report(&f.id).unwrap();
    assert_eq!(report.chapters.len(), 2);
    assert_eq!(
        f.core.book_settings(&f.id).narration,
        Some(Narration::Recording),
        "a finished sync becomes the book's narration"
    );
    let summary = f.core.list_books().unwrap().remove(0);
    assert_eq!(summary.recording, Some(Stage::Ready));

    f.core.play(&f.id, Some(1), None).await.unwrap();
    until("a word of the recording to be heard", || {
        let state = f.core.player_state();
        state.source == Narration::Recording && state.word_index.is_some()
    })
    .await;
    assert!(
        f.engine.calls().is_empty(),
        "the speech engine is not used: {:?}",
        f.engine.calls()
    );
    f.core.stop();
}

#[tokio::test]
async fn a_recording_of_another_book_is_caught_by_the_quick_check_and_can_be_kept() {
    let f = fixture().await;
    put_model(&f.root);
    let other = read(
        &[
            "The orchard behind the mill had not been pruned in years, and the pears that fell \
           there were left for the wasps and the badgers, who quarrelled over them at dusk."
                .to_owned(),
        ],
        0,
    );
    let total = other.last().unwrap().1 + 500;
    *f.script.lock().unwrap() = other;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::SampleMismatch));
    assert_eq!(f.core.recording(&f.id).unwrap().transcribed_ms, 0);

    f.core
        .sync_action(&f.id, super::SyncAction::Accept)
        .unwrap();
    sync(&f.core);
    assert_eq!(
        stage(&f.core, &f.id),
        Some(Stage::PoorMatch),
        "nothing matched: it needs review"
    );
    assert_eq!(f.core.book_settings(&f.id).narration, None);
    f.core
        .sync_action(&f.id, super::SyncAction::Accept)
        .unwrap();
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));
}

#[tokio::test]
async fn a_sync_stopped_by_quitting_carries_on_from_its_last_block() {
    let f = fixture().await;
    put_model(&f.root);
    // Longer than one block: the book is read after six minutes of other narration.
    let mut words: Vec<(u64, u64, String)> = (0..700)
        .map(|n| (n * 500, n * 500 + 300, format!("w{n}")))
        .collect();
    words.extend(read(&texts(&f.core, &f.id), 360_000));
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    // Check, promote, quick check and first block, then quit.
    for _ in 0..3 {
        assert!(f.core.sync_step());
    }
    let first = f.core.recording(&f.id).unwrap();
    assert_eq!(first.stage, Stage::SampleMismatch, "{first:?}");
    f.core
        .sync_action(&f.id, super::SyncAction::Accept)
        .unwrap();
    for _ in 0..2 {
        assert!(f.core.sync_step());
    }
    let partial = f.core.recording(&f.id).unwrap();
    assert_eq!(partial.stage, Stage::Transcribing);
    assert!(partial.transcribed_ms > 0 && partial.transcribed_ms < total);
    f.core.shutdown();

    let (factory, script, heard) = scripted();
    *script.lock().unwrap() = f.script.lock().unwrap().clone();
    let (again, _) = start_with(&f.root, &f.engine, factory);
    sync(&again);
    assert_eq!(stage(&again, &f.id), Some(Stage::Ready));
    assert_eq!(
        heard.lock().unwrap().first(),
        Some(&partial.transcribed_ms),
        "it carries on where it stopped, without the quick check again"
    );
}

#[tokio::test]
async fn removing_a_recording_keeps_its_files_and_goes_back_to_the_speech_engine() {
    let f = fixture().await;
    put_model(&f.root);
    let words = read(&texts(&f.core, &f.id), 0);
    let total = words.last().unwrap().1 + 500;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));

    f.core
        .set_narration(&f.id, Narration::Speech)
        .await
        .unwrap();
    assert_eq!(
        f.core.book_settings(&f.id).narration,
        Some(Narration::Speech)
    );
    f.core.remove_recording(&f.id).await.unwrap();
    assert!(f.core.recording(&f.id).is_none());
    assert!(f.audio.is_file(), "the audio file is left alone");
    assert_eq!(f.core.book_settings(&f.id).narration, None);
    assert!(f
        .core
        .set_narration(&f.id, Narration::Recording)
        .await
        .is_err());
}

#[tokio::test]
async fn moved_audio_makes_the_recording_missing_until_it_is_back() {
    let f = fixture().await;
    put_model(&f.root);
    let words = read(&texts(&f.core, &f.id), 0);
    let total = words.last().unwrap().1 + 500;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    let moved = f.root.join("elsewhere.wav");
    std::fs::rename(&f.audio, &moved).unwrap();
    f.core.shutdown();

    let (factory, _, _) = scripted();
    let (again, _) = start_with(&f.root, &f.engine, factory);
    sync(&again);
    assert_eq!(stage(&again, &f.id), Some(Stage::Missing));
    std::fs::rename(&moved, &f.audio).unwrap();
    again.look_for_missing_files();
    sync(&again);
    assert_eq!(stage(&again, &f.id), Some(Stage::Ready));
}

#[tokio::test]
async fn a_stretch_of_the_recording_plays_from_its_time() {
    let f = fixture().await;
    put_model(&f.root);
    let words = read(&texts(&f.core, &f.id), 0);
    let second_chapter = words
        .iter()
        .find(|(_, _, w)| w.as_str() == "Lamp")
        .unwrap()
        .0;
    let total = words.last().unwrap().1 + 500;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);

    f.core
        .play_recording_at(&f.id, second_chapter + 100)
        .await
        .unwrap();
    let state = f.core.player_state();
    assert_eq!(state.chapter, 1, "{state:?}");
    assert_eq!(state.source, Narration::Recording);
    f.core.stop();
}

#[tokio::test]
async fn a_long_recording_is_heard_block_after_block_without_gaps_or_overlaps() {
    let f = fixture().await;
    put_model(&f.root);
    // The book is read after twelve minutes of other narration: three blocks of recognition.
    let mut words: Vec<(u64, u64, String)> = (0..1_440)
        .map(|n| (n * 500, n * 500 + 300, format!("w{n}")))
        .collect();
    words.extend(read(&texts(&f.core, &f.id), 720_000));
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    for _ in 0..3 {
        assert!(f.core.sync_step());
    }
    f.core
        .sync_action(&f.id, super::SyncAction::Accept)
        .unwrap();
    f.heard.lock().unwrap().clear();

    let mut boundaries = vec![0];
    while boundaries.last() != Some(&total) {
        assert!(f.core.sync_step());
        let done = f.core.recording(&f.id).unwrap().transcribed_ms;
        if boundaries.last() != Some(&done) {
            boundaries.push(done);
        }
    }
    assert!(boundaries.len() >= 4, "{boundaries:?}");
    assert_eq!(
        *f.heard.lock().unwrap(),
        boundaries[..boundaries.len() - 1],
        "each block starts exactly where the last one ended"
    );
    sync(&f.core);
    let summary = f.core.recording(&f.id).unwrap().summary.unwrap();
    assert_eq!(summary.matched_words, summary.words, "{summary:?}");
}
