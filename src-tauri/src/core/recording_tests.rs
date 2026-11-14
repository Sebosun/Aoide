//! Recordings through Core: attach, sync with a scripted transcriber, play in place of the
//! speech engine. The test recording is a WAV file whose samples encode their own time, so the
//! scripted transcriber knows which stretch it is hearing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aoide_transcribe::{Error, Request, Transcriber, Word};
use aoide_tts::testing::FakeEngine;

use super::recording::{Narration, RecognitionEngine, TranscriberChoice};
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
        // Every sample is its own time, so the audio it is given must run on without a jump.
        let per_ms = f64::from(RATE) / 1_000.0;
        if let Some((n, _)) = request.samples.iter().enumerate().find(|(n, &sample)| {
            let expected = start as f64 + *n as f64 / per_ms;
            (f64::from(sample) * f64::from(SCALE) - expected).abs() > 2.0
        }) {
            return Err(Error::Failed(format!(
                "the audio from {start} ms jumps after {} ms",
                n as f64 / per_ms
            )));
        }
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

type Choices = Arc<Mutex<Vec<TranscriberChoice>>>;

/// A factory of scripted transcribers that keeps what each was asked to load.
fn scripted() -> (TranscriberFactory, Script, Arc<Mutex<Vec<u64>>>, Choices) {
    let script: Script = Arc::default();
    let heard = Arc::new(Mutex::new(Vec::new()));
    let choices: Choices = Arc::default();
    let (s, h, c) = (
        Arc::clone(&script),
        Arc::clone(&heard),
        Arc::clone(&choices),
    );
    let factory: TranscriberFactory = Arc::new(move |choice: &TranscriberChoice| {
        c.lock().unwrap().push(choice.clone());
        Ok(Box::new(Scripted {
            script: Arc::clone(&s),
            heard: Arc::clone(&h),
        }) as Box<dyn Transcriber>)
    });
    (factory, script, heard, choices)
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
    choices: Choices,
    audio: PathBuf,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    let id = library_folder(&root, "");
    let engine = FakeEngine::new();
    let (factory, script, heard, choices) = scripted();
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
        choices,
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
    let storage = f.core.storage_report().unwrap();
    assert!(
        storage.sync_bytes > 0,
        "the transcript and sync map count as disk use"
    );
    assert!(storage.models_bytes > 0, "and so does the model");

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
    let generation = f.core.generation_state();
    assert_eq!(
        (
            generation.book_id,
            generation.buffered_ahead_ms,
            generation.pending
        ),
        (None, 0, 0),
        "nothing is being made for a recording"
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

    let (factory, script, heard, _) = scripted();
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

    let (factory, _, _, _) = scripted();
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
    // The last chapter runs to the end of the recording: what is left is known, not estimated.
    let left = state
        .chapter_left_ms
        .expect("exact time left for a recording");
    let expected = total - second_chapter;
    assert!(
        left + 1_500 > expected && left < expected + 500,
        "{left} ms left, expected about {expected}"
    );
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

#[tokio::test]
async fn sentences_the_recording_lacks_are_marked_in_the_chapter() {
    let f = fixture().await;
    put_model(&f.root);
    // Only the first chapter is narrated.
    let first = texts(&f.core, &f.id).remove(0);
    let words = read(&[first], 0);
    let total = words.last().unwrap().1 + 500;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    let speech = f.core.chapter(&f.id, 1).unwrap();
    assert_eq!(
        speech.not_narrated, None,
        "nothing is marked while speech narrates"
    );
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    if stage(&f.core, &f.id) == Some(Stage::PoorMatch) {
        f.core
            .sync_action(&f.id, super::SyncAction::Accept)
            .unwrap();
        sync(&f.core);
    }
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));

    assert_eq!(f.core.chapter(&f.id, 0).unwrap().not_narrated, Some(vec![]));
    let second = f.core.chapter(&f.id, 1).unwrap();
    let speakable: Vec<String> = second
        .blocks
        .iter()
        .flat_map(|b| &b.segments)
        .filter(|s| s.speakable)
        .map(|s| s.id.clone())
        .collect();
    assert!(!speakable.is_empty());
    assert_eq!(second.not_narrated, Some(speakable));
}

#[tokio::test]
async fn the_synced_part_plays_while_the_rest_is_still_syncing() {
    let f = fixture().await;
    put_model(&f.root);
    let texts = texts(&f.core, &f.id);
    // The first chapter is read early on, the second only after the first block of recognition
    // (about five minutes) ends. (The test chapters are too short to be found among other
    // narration, so there is silence in between.)
    let mut words = read(&texts[..1], 2_000);
    words.extend(read(&texts[1..], 400_000));
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    // Check, promote, quick check (the samples are silence), then one block.
    for _ in 0..3 {
        assert!(f.core.sync_step());
    }
    f.core
        .sync_action(&f.id, super::SyncAction::Accept)
        .unwrap();
    while f.core.recording(&f.id).unwrap().transcribed_ms == 0 {
        assert!(f.core.sync_step());
    }
    let partial = f.core.recording(&f.id).unwrap();
    assert_eq!(partial.stage, Stage::Transcribing);
    assert!(partial.transcribed_ms < 400_000, "{partial:?}");

    f.core
        .set_narration(&f.id, Narration::Recording)
        .await
        .expect("a recording can be listened to while it syncs");
    f.core.play(&f.id, Some(0), None).await.unwrap();
    until("the synced first chapter to play", || {
        let state = f.core.player_state();
        state.source == Narration::Recording && state.word_index.is_some()
    })
    .await;
    f.core.play(&f.id, Some(1), None).await.unwrap();
    until(
        "playback to wait for the sync to reach the second chapter",
        || f.core.player_state().waiting_for_sync,
    )
    .await;

    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));
    until("the second chapter to play once synced", || {
        let state = f.core.player_state();
        state.chapter == 1 && !state.waiting_for_sync && state.word_index.is_some()
    })
    .await;
    assert!(f.engine.calls().is_empty(), "{:?}", f.engine.calls());
    f.core.stop();
}

#[tokio::test]
async fn a_recognition_server_on_this_computer_needs_no_model_here() {
    let f = fixture().await;
    let words = read(&texts(&f.core, &f.id), 1_000);
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::NeedsModel));

    let mut settings = f.core.app_settings();
    settings.recognition.engine = RecognitionEngine::Server;
    settings.recognition.server.base_url = "http://192.168.1.20:8000".into();
    assert!(
        f.core.save_app_settings(settings.clone()).is_err(),
        "another computer only with the home network allowed"
    );
    settings.recognition.server.base_url = "http://127.0.0.1:8000".into();
    settings.recognition.server.model = "base.en".into();
    f.core.save_app_settings(settings).unwrap();
    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));
    assert_eq!(
        f.choices.lock().unwrap().as_slice(),
        [TranscriberChoice::Server {
            base_url: "http://127.0.0.1:8000".into(),
            model: "base.en".into(),
            network: aoide_tts::NetworkScope::Loopback,
        }]
    );
}

#[tokio::test]
async fn files_put_in_order_by_hand_keep_that_order() {
    let f = fixture().await;
    let dir = f.root.join("Audio");
    write_recording(&dir.join("part 1.wav"), 2_000);
    write_recording(&dir.join("part 2.wav"), 2_000);
    let listed = |names: &[&str]| -> Vec<String> {
        names
            .iter()
            .map(|n| dir.join(n).to_string_lossy().into_owned())
            .collect()
    };
    let names = |view: super::RecordingView| -> Vec<String> {
        view.files.into_iter().map(|file| file.name).collect()
    };
    let sorted = f
        .core
        .attach_recording(&f.id, listed(&["part 2.wav", "part 1.wav"]))
        .unwrap();
    assert_eq!(
        names(sorted),
        ["part 1.wav", "part 2.wav"],
        "sorted by name"
    );
    let by_hand = f
        .core
        .attach_recording_as_listed(&f.id, listed(&["part 2.wav", "part 1.wav"]))
        .unwrap();
    assert_eq!(names(by_hand), ["part 2.wav", "part 1.wav"]);
    assert_eq!(
        names(f.core.recording(&f.id).unwrap()),
        ["part 2.wav", "part 1.wav"]
    );
}
