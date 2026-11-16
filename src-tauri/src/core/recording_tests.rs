//! Recordings through Core: attach, sync with a scripted transcriber, play in place of the
//! speech engine. The test recording is a WAV file whose samples encode their own time, so the
//! scripted transcriber knows which stretch it is hearing.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use aoide_transcribe::{Error, Request, Transcriber, Word};
use aoide_tts::testing::FakeEngine;

use super::recording::TranscriberFactory;
use super::recording::{Narration, RecognitionEngine, TranscriberChoice};
use super::syncing::Stage;
use super::tests::{epub, epub_with, library_folder, start_with, until};
use super::{Core, CoreEvent};

/// Sample value per millisecond of the recording.
pub(super) const SCALE: f32 = 100_000.0;
pub(super) const RATE: u32 = 16_000;

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
pub(super) fn scripted() -> (TranscriberFactory, Script, Arc<Mutex<Vec<u64>>>, Choices) {
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
pub(super) fn read(texts: &[String], from_ms: u64) -> Vec<(u64, u64, String)> {
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
pub(super) fn write_recording(path: &Path, total_ms: u64) {
    write_recording_part(path, 0, total_ms);
}

/// One file of a recording split into parts: its samples go on from `from_ms`.
pub(super) fn write_recording_part(path: &Path, from_ms: u64, total_ms: u64) {
    let frames = total_ms * u64::from(RATE) / 1_000;
    let mut data = Vec::with_capacity(frames as usize * 4);
    for n in 0..frames {
        let ms = from_ms as f32 + n as f32 * 1_000.0 / RATE as f32;
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
    fixture_with(None).await
}

/// The fixture with `book` in place of the test book.
async fn fixture_with(book: Option<Vec<u8>>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_owned();
    let mut id = library_folder(&root, "");
    if let Some(bytes) = book {
        std::fs::write(root.join("Books/quay-lights.epub"), &bytes).unwrap();
        id = aoide_library::content_hash(&bytes).0;
    }
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
async fn the_word_being_heard_moves_on_as_the_recording_plays() {
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

    // The second chapter: its two-word heading, then its paragraph, 400 ms a word.
    f.core.play(&f.id, Some(1), None).await.unwrap();
    until("the third word of the paragraph", || {
        let state = f.core.player_state();
        state.segment_id.as_deref() == Some("1:1:0")
            && state.word_index.is_some_and(|word| word >= 2)
    })
    .await;
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
async fn what_the_recording_lacks_is_spoken_by_the_speech_engine_when_asked() {
    let f = fixture().await;
    put_model(&f.root);
    // Only the first chapter is narrated.
    let chapters = texts(&f.core, &f.id);
    let words = read(&chapters[..1], 0);
    let total = words.last().unwrap().1 + 500;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
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

    let mut settings = f.core.book_settings(&f.id);
    assert!(!settings.speak_missing, "off unless asked");
    settings.speak_missing = true;
    f.core.save_book_settings(&f.id, settings).await.unwrap();
    assert_eq!(
        f.core.chapter(&f.id, 1).unwrap().not_narrated,
        Some(vec![]),
        "nothing is passed over"
    );

    f.core.play(&f.id, Some(1), None).await.unwrap();
    until("the speech engine to speak the second chapter", || {
        let state = f.core.player_state();
        state.chapter == 1 && state.word_index.is_some()
    })
    .await;
    let spoken = f.engine.calls();
    assert!(
        spoken.iter().any(|t| chapters[1].contains(t.as_str())),
        "{spoken:?}"
    );
    assert!(
        !spoken.iter().any(|t| chapters[0].contains(t.as_str())),
        "the narrated chapter is not spoken: {spoken:?}"
    );
    let state = f.core.player_state();
    assert_eq!(state.source, Narration::Recording);
    assert!(!state.waiting_for_sync);
    assert_eq!(
        f.core.generation_state().book_id.as_deref(),
        Some(f.id.as_str())
    );

    // The narrated chapter still plays the recording.
    f.core.play(&f.id, Some(0), None).await.unwrap();
    until("a word of the recording", || {
        let state = f.core.player_state();
        state.chapter == 0 && state.word_index.is_some()
    })
    .await;
    f.core.stop();
}

#[tokio::test]
async fn text_the_sync_finds_missing_while_playback_waits_for_it_is_spoken_when_asked() {
    // No word of the second chapter is in the first, so none of it can be matched there.
    let chapter = |title: &str, body: &str| {
        format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>{title}</h1><p>{body}</p></body></html>"#
        )
    };
    let f = fixture_with(Some(epub_with(
        chapter(
            "Harbour",
            "Gulls argued over the nets. The tide came in slowly.",
        ),
        chapter(
            "Embers",
            "Someone lit lanterns at dusk. Coals glowed until morning.",
        ),
    )))
    .await;
    put_model(&f.root);
    let chapters = texts(&f.core, &f.id);
    // Only the first chapter is narrated, so late that the first block of recognition (about
    // five minutes) ends inside it; the recording ends soon after it.
    let words = read(&chapters[..1], 340_000);
    let total = words.last().unwrap().1 + 500;
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
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Transcribing));
    f.core
        .set_narration(&f.id, Narration::Recording)
        .await
        .unwrap();
    let mut settings = f.core.book_settings(&f.id);
    settings.speak_missing = true;
    f.core.save_book_settings(&f.id, settings).await.unwrap();

    f.core.play(&f.id, Some(1), None).await.unwrap();
    until("playback to wait for the sync", || {
        f.core.player_state().waiting_for_sync
    })
    .await;
    // Playback settles into waiting, as it does while the sync runs beside it in the app.
    tokio::time::sleep(super::TICK * 4).await;
    sync(&f.core);
    if stage(&f.core, &f.id) == Some(Stage::PoorMatch) {
        f.core
            .sync_action(&f.id, super::SyncAction::Accept)
            .unwrap();
        sync(&f.core);
    }
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));
    assert_eq!(
        f.core.chapter(&f.id, 1).unwrap().not_narrated,
        Some(vec![]),
        "nothing is passed over"
    );
    until("the speech engine to speak the second chapter", || {
        let state = f.core.player_state();
        state.chapter == 1 && state.word_index.is_some()
    })
    .await;
    let spoken = f.engine.calls();
    assert!(
        spoken.iter().any(|t| chapters[1].contains(t.as_str())),
        "{spoken:?}"
    );
    f.core.stop();
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
    assert!(partial.listenable, "what is synced so far can be heard");
    // Nothing was chosen: whoever paired an audiobook wants to hear it.
    assert!(partial.narrates, "the recording narrates by default");
    assert_eq!(
        f.events.payloads(CoreEvent::SYNC_STATE).last().unwrap()["narrates"],
        true,
        "the views are told"
    );

    f.core
        .set_narration(&f.id, Narration::Speech)
        .await
        .unwrap();
    assert!(
        !f.core.recording(&f.id).unwrap().narrates,
        "speech, once chosen, stays"
    );
    f.core
        .set_narration(&f.id, Narration::Recording)
        .await
        .expect("a recording can be listened to while it syncs");
    assert!(f.core.recording(&f.id).unwrap().narrates);
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

/// An aligner that places word k at 50·k ms of whatever it is sent.
fn fifty_ms_aligner() -> String {
    use std::io::{BufRead, BufReader, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
            let words = request["words"].as_array().map_or(0, Vec::len);
            let spans: Vec<[u32; 2]> = (0..words as u32).map(|k| [k * 50, k * 50 + 40]).collect();
            let reply = serde_json::json!({ "words": spans }).to_string();
            let mut stream = stream;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    url
}

#[tokio::test]
async fn a_recording_gets_exact_word_times_from_the_aligner() {
    let f = fixture().await;
    put_model(&f.root);
    let words = read(&texts(&f.core, &f.id), 1_000);
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    let mut settings = f.core.app_settings();
    settings.aligner = Some(super::settings::AlignerSettings {
        base_url: fifty_ms_aligner(),
        network: aoide_tts::NetworkScope::Loopback,
        launch: None,
    });
    f.core.save_app_settings(settings).unwrap();

    let book = f.core.load_book(&f.id).unwrap();
    let index = book.chapter_start(1).unwrap() + 1;
    f.core
        .play(
            &f.id,
            Some(1),
            Some(book.segment(index).unwrap().id.clone()),
        )
        .await
        .unwrap();
    until("the aligner's word times to arrive", || {
        let starts: Vec<u32> = f
            .core
            .playback
            .timings_of(index)
            .unwrap_or_default()
            .iter()
            .map(|t| t.start_ms)
            .collect();
        starts.len() > 1 && starts.windows(2).all(|w| w[1] == w[0] + 50)
    })
    .await;
    f.core.stop();
}

#[tokio::test]
async fn a_synced_recording_exports_as_a_read_along_epub() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        println!("skipped: the export needs ffmpeg to turn WAV into AAC");
        return;
    }
    let f = fixture().await;
    put_model(&f.root);
    let words = read(&texts(&f.core, &f.id), 500);
    let total = words.last().unwrap().1 + 500;
    // Two files, the second starting mid-sentence.
    let split = words[words.len() / 2].0 + 100;
    *f.script.lock().unwrap() = words;
    let parts = [
        f.root.join("Audio/part 1.wav"),
        f.root.join("Audio/part 2.wav"),
    ];
    write_recording_part(&parts[0], 0, split);
    write_recording_part(&parts[1], split, total - split);
    f.core
        .attach_recording(
            &f.id,
            parts.iter().map(|p| p.to_string_lossy().into()).collect(),
        )
        .unwrap();
    assert!(
        f.core.export_read_along(&f.id).await.is_err(),
        "nothing to export before the sync is done"
    );
    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));

    let path = f.core.export_read_along(&f.id).await.unwrap();
    assert!(path.ends_with(" (read-along).epub"), "{path}");
    let bytes = std::fs::read(&path).unwrap();
    let parsed = aoide_epub::parse_epub(&bytes, &aoide_epub::ParseOptions::default()).unwrap();
    let exported: Vec<String> = parsed.chapters.iter().map(|c| c.text.clone()).collect();
    assert_eq!(
        exported,
        texts(&f.core, &f.id),
        "the same text, chapter by chapter"
    );

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    {
        let first = zip.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), zip::CompressionMethod::Stored);
    }
    let read_entry = |zip: &mut zip::ZipArchive<std::io::Cursor<&Vec<u8>>>, name: &str| {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut zip.by_name(name).unwrap(), &mut text).unwrap();
        text
    };
    let package = read_entry(&mut zip, "OEBPS/package.opf");
    assert!(package.contains("media-overlay="), "{package}");
    assert!(package.contains("property=\"media:duration\""));
    let audio: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with("OEBPS/audio/"))
        .map(str::to_owned)
        .collect();
    assert_eq!(audio, ["OEBPS/audio/01.m4a", "OEBPS/audio/02.m4a"]);
    assert!(package.contains("media-type=\"audio/mp4\""));

    // Every narrated sentence is in an overlay, played in order, without gaps or overlaps.
    let smils: Vec<String> = zip
        .file_names()
        .filter(|n| n.ends_with(".smil"))
        .map(str::to_owned)
        .collect();
    assert_eq!(smils.len(), 2, "one overlay per chapter");
    let starts = [0, split];
    let mut voiced = Vec::new();
    let mut spans = Vec::new();
    for name in smils {
        let smil = read_entry(&mut zip, &name);
        let doc = roxmltree::Document::parse(&smil).unwrap();
        for par in doc.descendants().filter(|n| n.has_tag_name("par")) {
            let text = par.children().find(|n| n.has_tag_name("text")).unwrap();
            let audio = par.children().find(|n| n.has_tag_name("audio")).unwrap();
            let fragment = text.attribute("src").unwrap().split('#').nth(1).unwrap();
            let file = match audio.attribute("src").unwrap() {
                "audio/01.m4a" => 0,
                "audio/02.m4a" => 1,
                other => panic!("unexpected audio {other}"),
            };
            let seconds = |name: &str| -> u64 {
                let value = audio.attribute(name).unwrap();
                let mut total = 0.0;
                for part in value.split(':') {
                    total = total * 60.0 + part.parse::<f64>().unwrap();
                }
                (total * 1_000.0).round() as u64
            };
            spans.push((
                starts[file] + seconds("clipBegin"),
                starts[file] + seconds("clipEnd"),
            ));
            if voiced.last() != Some(&fragment.to_owned()) {
                voiced.push(fragment.to_owned());
            }
        }
    }
    let book = f.core.load_book(&f.id).unwrap();
    let narrated: Vec<String> = book
        .chapters
        .iter()
        .flat_map(|c| &c.segments)
        .filter(|s| !s.speech.is_empty())
        .map(|s| format!("s{}", s.id.replace(':', "-")))
        .collect();
    assert_eq!(voiced, narrated);
    assert!(
        spans.windows(2).all(|w| w[0].1 == w[1].0),
        "each clip starts where the last ended: {spans:?}"
    );
    assert!(
        spans.iter().any(|s| s.1 == split),
        "a clip is cut at the file boundary"
    );
}

#[tokio::test]
async fn find_books_offers_audiobooks_kept_beside_epubs_and_pairs_them_on_import() {
    let f = fixture().await;
    // A copy of the book already in the library, its recording beside it.
    let shelf = f.root.join("Shelf/Quay Lights");
    std::fs::create_dir_all(&shelf).unwrap();
    std::fs::write(shelf.join("quay-lights.epub"), epub()).unwrap();
    write_recording(&shelf.join("quay-lights.wav"), 3_000);
    // A new book, its recording in a folder beside it.
    let other = f.root.join("Other");
    std::fs::create_dir_all(other.join("Harbour [Audiobook]")).unwrap();
    let html = |text: &str| {
        format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>{text}</p></body></html>"#)
    };
    let harbour = epub_with(html("Nets dried on the wall."), html("Boats slept."));
    std::fs::write(other.join("Harbour.epub"), &harbour).unwrap();
    write_recording(&other.join("Harbour [Audiobook]/01.wav"), 3_000);
    let harbour_id = aoide_library::content_hash(&harbour).0;

    let mut settings = f.core.app_settings();
    settings.scan.roots = vec![
        f.root.join("Shelf").to_string_lossy().into(),
        other.to_string_lossy().into(),
    ];
    f.core.save_app_settings(settings).unwrap();
    let found = f.core.scan_for_books().await.unwrap();
    let audiobook = |id: &str| {
        found
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.audiobook.clone())
            .unwrap()
    };
    let path = |p: PathBuf| p.to_string_lossy().into_owned();
    assert_eq!(audiobook(&f.id), vec![path(shelf.join("quay-lights.wav"))]);
    assert_eq!(
        audiobook(&harbour_id),
        vec![path(other.join("Harbour [Audiobook]"))]
    );

    // Paired books are not offered again.
    f.core.attach_recording(&f.id, audiobook(&f.id)).unwrap();
    let again = f.core.discovered_books().unwrap();
    let ours = again.iter().find(|c| c.id == f.id).unwrap();
    assert!(ours.audiobook.is_empty());

    let results = f
        .core
        .import_discovered(vec![super::ImportChoice {
            id: harbour_id.clone(),
            apply_suggestion: false,
            pair_audiobook: true,
        }])
        .await
        .unwrap();
    assert!(results.iter().all(|r| r.error.is_none()), "{results:?}");
    let paired = f.core.recording(&harbour_id).expect("paired on import");
    assert_eq!(paired.files.len(), 1);
    assert_eq!(paired.total_ms, 3_000);
}

// ------------------------------------------------------------ finding audiobooks

/// Two audiobooks named like the test book in `Audiobooks`, tried in this order: the first reads
/// another book where the quick check listens, the second reads this one when `second_reads` is
/// set (else it too reads another book).
fn two_audiobooks(f: &Fixture, second_reads: bool) -> (PathBuf, PathBuf) {
    let other = "The orchard behind the mill had not been pruned in years, and the pears that \
                 fell there were left for the wasps and the badgers, who quarrelled at dusk.";
    let mut script = if second_reads {
        read(&texts(&f.core, &f.id), 0)
    } else {
        read(&[other.to_owned()], 0)
    };
    let book_end = script.last().unwrap().1 + 500;
    for at in [40_000, 100_000, 160_000] {
        script.extend(read(&[other.to_owned()], at));
    }
    *f.script.lock().unwrap() = script;
    let (first, second) = (
        f.root.join("Audiobooks/A Quay Lights"),
        f.root.join("Audiobooks/Quay Lights"),
    );
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    write_recording(&first.join("part.wav"), 200_000);
    write_recording(&second.join("part.wav"), book_end);
    (first, second)
}

fn search(f: &Fixture, verify: bool) {
    // The test recordings are seconds long, far below an audiobook's size.
    f.core
        .audiobooks
        .min_audiobook_bytes
        .store(0, std::sync::atomic::Ordering::Relaxed);
    let mut settings = f.core.app_settings();
    settings.audiobooks.roots = vec![f.root.join("Audiobooks").to_string_lossy().into_owned()];
    settings.verify_audiobooks = verify;
    f.core.save_app_settings(settings).unwrap();
}

fn match_stage(core: &Core, id: &str) -> Option<super::audiobooks::MatchStage> {
    core.audiobook_matches()
        .into_iter()
        .find(|m| m.book_id == id)
        .map(|m| m.stage)
}

fn paired_files(core: &Core, id: &str) -> Vec<String> {
    core.recording(id)
        .map(|r| r.files.into_iter().map(|f| f.path).collect())
        .unwrap_or_default()
}

#[tokio::test]
async fn found_audiobooks_wait_for_the_user_who_pairs_the_one_they_pick() {
    use super::audiobooks::{AudiobookChoice, MatchStage};
    let f = fixture().await;
    put_model(&f.root);
    let (first, second) = two_audiobooks(&f, true);
    search(&f, false);

    let found = f.core.scan_for_audiobooks().await.unwrap();

    assert_eq!(found.len(), 1);
    let paths: Vec<Vec<String>> = found[0]
        .candidates
        .iter()
        .map(|c| c.paths.clone())
        .collect();
    let path = |p: &PathBuf| p.to_string_lossy().into_owned();
    assert_eq!(paths, vec![vec![path(&first)], vec![path(&second)]]);
    let second_found = &found[0].candidates[1];
    assert!(second_found.folder);
    assert_eq!(second_found.files, vec![path(&second.join("part.wav"))]);
    assert_eq!(
        second_found.bytes,
        std::fs::metadata(second.join("part.wav")).unwrap().len()
    );
    assert_eq!(found[0].stage, MatchStage::Found);
    assert!(
        f.core.recording(&f.id).is_none(),
        "nothing paired before the user says so"
    );

    let outcome = f.core.pair_audiobooks(vec![AudiobookChoice {
        book_id: f.id.clone(),
        candidate: 1,
    }]);
    assert!(outcome.iter().all(|o| o.error.is_none()), "{outcome:?}");
    assert_eq!(
        match_stage(&f.core, &f.id),
        Some(MatchStage::Paired { candidate: 1 })
    );
    assert_eq!(
        paired_files(&f.core, &f.id),
        vec![path(&second.join("part.wav"))]
    );
    f.core.stop();
}

#[tokio::test]
async fn with_verifying_on_the_next_audiobook_is_tried_when_the_quick_check_fails() {
    use super::audiobooks::MatchStage;
    let f = fixture().await;
    put_model(&f.root);
    let (first, second) = two_audiobooks(&f, true);
    search(&f, true);

    f.core.scan_for_audiobooks().await.unwrap();
    assert_eq!(
        match_stage(&f.core, &f.id),
        Some(MatchStage::Verifying { candidate: 0 })
    );
    let path = |p: PathBuf| p.to_string_lossy().into_owned();
    assert_eq!(
        paired_files(&f.core, &f.id),
        vec![path(first.join("part.wav"))]
    );

    sync(&f.core);

    assert_eq!(
        match_stage(&f.core, &f.id),
        Some(MatchStage::Paired { candidate: 1 })
    );
    assert_eq!(
        paired_files(&f.core, &f.id),
        vec![path(second.join("part.wav"))]
    );
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));
    f.core.stop();
}

#[tokio::test]
async fn with_verifying_on_a_book_none_of_whose_audiobooks_pass_is_left_unpaired() {
    use super::audiobooks::MatchStage;
    let f = fixture().await;
    put_model(&f.root);
    two_audiobooks(&f, false);
    search(&f, true);

    f.core.scan_for_audiobooks().await.unwrap();
    sync(&f.core);

    assert_eq!(match_stage(&f.core, &f.id), Some(MatchStage::NoneVerified));
    assert!(f.core.recording(&f.id).is_none());
    f.core.stop();
}

#[tokio::test]
async fn added_books_with_audiobooks_waiting_are_counted_unless_looking_is_off_or_aoide_verifies() {
    use super::audiobooks::MatchStage;
    let f = fixture().await;
    put_model(&f.root);
    two_audiobooks(&f, true);
    let ids = vec![f.id.clone()];

    search(&f, false);
    let mut settings = f.core.app_settings();
    settings.audiobooks.on_add = false;
    f.core.save_app_settings(settings.clone()).unwrap();
    assert_eq!(f.core.look_for_audiobooks(ids.clone()).await, Ok(0));
    assert!(f.core.audiobook_matches().is_empty(), "not looked for");

    settings.audiobooks.on_add = true;
    f.core.save_app_settings(settings.clone()).unwrap();
    assert_eq!(f.core.look_for_audiobooks(ids.clone()).await, Ok(1));

    settings.verify_audiobooks = true;
    f.core.save_app_settings(settings).unwrap();
    assert_eq!(
        match_stage(&f.core, &f.id),
        Some(MatchStage::Verifying { candidate: 0 }),
        "turning verifying on starts on what waits"
    );
    f.core.stop();
}

/// Another edition of the test book, so both come equally close to the same audiobooks.
/// Returns its id.
async fn second_edition(f: &Fixture) -> String {
    let html = |text: &str| {
        format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>{text}</p></body></html>"#)
    };
    let edition = epub_with(html("Gulls argued over the nets."), html("Boats slept."));
    std::fs::write(f.root.join("Books/quay-lights-2.epub"), &edition).unwrap();
    f.core.rescan().unwrap();
    until("the second edition", || {
        f.core.list_books().is_ok_and(|b| b.len() == 2)
    })
    .await;
    aoide_library::content_hash(&edition).0
}

#[tokio::test]
async fn an_audiobook_is_paired_with_one_book_and_leaves_the_other_matches() {
    use super::audiobooks::AudiobookChoice;
    let f = fixture().await;
    put_model(&f.root);
    let (first, second) = two_audiobooks(&f, true);
    let other = second_edition(&f).await;
    search(&f, false);
    let found = f.core.scan_for_audiobooks().await.unwrap();
    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|m| m.candidates.len() == 2));
    let path = |p: &PathBuf| p.to_string_lossy().into_owned();

    // "Pair all": each book's closest, the same audiobook for both.
    let outcome = f.core.pair_audiobooks(vec![
        AudiobookChoice {
            book_id: f.id.clone(),
            candidate: 1,
        },
        AudiobookChoice {
            book_id: other.clone(),
            candidate: 1,
        },
    ]);

    assert_eq!(outcome[0].error, None);
    assert!(outcome[1].error.is_some(), "{outcome:?}");
    assert_eq!(
        paired_files(&f.core, &f.id),
        vec![path(&second.join("part.wav"))]
    );
    assert!(f.core.recording(&other).is_none());
    let left: Vec<Vec<String>> = f
        .core
        .audiobook_matches()
        .into_iter()
        .find(|m| m.book_id == other)
        .map(|m| m.candidates.into_iter().map(|c| c.paths).collect())
        .unwrap();
    assert_eq!(
        left,
        vec![vec![path(&first)]],
        "the paired audiobook is offered no more"
    );
    f.core.stop();
}

#[tokio::test]
async fn with_verifying_on_a_book_skips_the_audiobook_another_is_being_checked_with() {
    use super::audiobooks::MatchStage;
    let f = fixture().await;
    put_model(&f.root);
    two_audiobooks(&f, true);
    let other = second_edition(&f).await;
    search(&f, false);
    f.core.scan_for_audiobooks().await.unwrap();

    let mut settings = f.core.app_settings();
    settings.verify_audiobooks = true;
    f.core.save_app_settings(settings).unwrap();

    let stages = [match_stage(&f.core, &f.id), match_stage(&f.core, &other)];
    assert!(
        stages.contains(&Some(MatchStage::Verifying { candidate: 0 }))
            && stages.contains(&Some(MatchStage::Verifying { candidate: 1 })),
        "{stages:?}"
    );
    f.core.stop();
}

// ------------------------------------------------------------ the line of recordings

/// A second book in the fixture's library, read aloud by its own recording, and both paired:
/// the test book's first, so it is first in line. Returns the second book's id.
async fn two_in_line(f: &Fixture) -> String {
    let chapter = |title: &str, body: &str| {
        format!(
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>{title}</h1><p>{body}</p></body></html>"#
        )
    };
    let bytes = epub_with(
        chapter(
            "The Mill",
            "The wheel turned all night. Flour drifted on the stair.",
        ),
        chapter(
            "The Pond",
            "Ducks slept by the reeds. A heron watched them.",
        ),
    );
    std::fs::write(f.root.join("Books/mill.epub"), &bytes).unwrap();
    let other = aoide_library::content_hash(&bytes).0;
    f.core.rescan().unwrap();
    until("the second book", || {
        f.core.list_books().is_ok_and(|b| b.len() == 2)
    })
    .await;
    // Both recordings hear the same script: the test book's text, then the other's.
    let mut script = read(&texts(&f.core, &f.id), 0);
    let first_end = script.last().unwrap().1 + 500;
    script.extend(read(&texts(&f.core, &other), 0));
    *f.script.lock().unwrap() = script;
    write_recording(&f.audio, first_end);
    let second = f.root.join("Audio/mill.wav");
    write_recording(&second, first_end);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    f.core
        .attach_recording(&other, vec![second.to_string_lossy().into()])
        .unwrap();
    other
}

/// Runs the sync job until book `id`'s recording is being transcribed.
fn until_transcribing(core: &Core, id: &str) {
    for _ in 0..20 {
        if stage(core, id) == Some(Stage::Transcribing) {
            return;
        }
        core.sync_step();
    }
    panic!("{id} never started: {:?}", stage(core, id));
}

#[tokio::test]
async fn pausing_the_recording_being_synced_starts_the_next_in_line() {
    let f = fixture().await;
    put_model(&f.root);
    let other = two_in_line(&f).await;
    until_transcribing(&f.core, &f.id);
    assert_eq!(stage(&f.core, &other), Some(Stage::Waiting));

    f.core.sync_action(&f.id, super::SyncAction::Pause).unwrap();
    until_transcribing(&f.core, &other);
    sync(&f.core);

    assert_eq!(stage(&f.core, &f.id), Some(Stage::Paused));
    assert_ne!(stage(&f.core, &other), Some(Stage::Waiting));
    f.core.stop();
}

#[tokio::test]
async fn removing_the_recording_being_synced_starts_the_next_in_line() {
    let f = fixture().await;
    put_model(&f.root);
    let other = two_in_line(&f).await;
    until_transcribing(&f.core, &f.id);

    f.core.remove_recording(&f.id).await.unwrap();
    until_transcribing(&f.core, &other);
    f.core.stop();
}

/// The UI mock's recording: one the core synced, as its view shows it (see `core/fixtures.rs`).
#[tokio::test]
async fn fixture_of_a_synced_recording_for_the_ui_mock() {
    if !super::fixtures::wanted() {
        return;
    }
    let f = fixture().await;
    put_model(&f.root);
    let words = read(&texts(&f.core, &f.id), 1_000);
    let total = words.last().unwrap().1 + 1_000;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    f.core
        .attach_recording(&f.id, vec![f.audio.to_string_lossy().into()])
        .unwrap();
    sync(&f.core);
    let view = f.core.recording(&f.id).unwrap();
    assert_eq!(view.stage, Stage::Ready, "{view:?}");
    super::fixtures::write(
        "recording",
        &view,
        &[
            (f.id.clone(), "book-0".into()),
            (f.root.to_string_lossy().into_owned(), "~".into()),
        ],
    );
}

/// The test book read aloud into an audiobook added on its own, and where its second
/// chapter's third word is heard.
async fn audiobook_of_the_book(f: &Fixture) -> (String, u64) {
    let texts = texts(&f.core, &f.id);
    let words = read(&texts, 1_000);
    let total = words.last().unwrap().1 + 1_000;
    let first_chapter = texts[0].split_whitespace().count();
    let at = words[first_chapter + 2].0;
    *f.script.lock().unwrap() = words;
    write_recording(&f.audio, total);
    let added = f.core.add_audiobook(vec![f.audio.to_string_lossy().into()]);
    (added.book_id.expect("added"), at)
}

fn pending_place(core: &Core, id: &str) -> Option<u64> {
    super::lock(&core.library)
        .book_value(
            &aoide_library::BookId(id.into()),
            aoide_library::BookKey::PendingPlace,
        )
        .unwrap()
}

#[tokio::test]
async fn an_audiobook_given_its_text_syncs_and_picks_up_where_it_was() {
    let f = fixture().await;
    let (audiobook, at) = audiobook_of_the_book(&f).await;
    f.core.seek_to(&audiobook, at).unwrap();
    f.core.add_bookmark(&audiobook, "here").unwrap();
    super::lock(&f.core.library)
        .record_listening(
            &aoide_library::BookId(audiobook.clone()),
            "2026-11-14",
            4_000,
        )
        .unwrap();

    f.core.merge_audiobook(&audiobook, &f.id).unwrap();
    let books = f.core.list_books().unwrap();
    assert_eq!(books.len(), 1, "the audiobook item is gone");
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Checking));
    assert_eq!(f.core.recording(&audiobook), None);
    let marks = f.core.bookmarks(&f.id).unwrap();
    assert_eq!((marks.len(), marks[0].position_ms), (1, Some(at)));
    assert!(f
        .core
        .book_listening(&f.id)
        .unwrap()
        .iter()
        .any(|d| d.ms == 4_000));
    assert_eq!(pending_place(&f.core, &f.id), Some(at));
    assert!(
        f.core.merge_audiobook(&audiobook, &f.id).is_err(),
        "done once"
    );

    put_model(&f.root);
    f.core.models_changed();
    sync(&f.core);
    assert_eq!(stage(&f.core, &f.id), Some(Stage::Ready));
    assert_eq!(pending_place(&f.core, &f.id), None, "settled");
    let progress = super::lock(&f.core.library)
        .progress(&aoide_library::BookId(f.id.clone()))
        .unwrap()
        .expect("the place became progress");
    let book = f.core.load_book(&f.id).unwrap();
    let segment = progress.segment_id.expect("a sentence");
    let index = book.index_of(&segment).unwrap();
    // The sentence of chapter two that holds its third word.
    let mut words = 0;
    let holding = book.chapters[1]
        .segments
        .iter()
        .position(|segment| {
            words += segment.words.len();
            words > 2
        })
        .unwrap();
    assert_eq!(progress.chapter, 1);
    assert_eq!(book.locate(index).map(|(_, local)| local), Some(holding));
}

#[tokio::test]
async fn reading_a_merged_book_first_drops_the_place_it_kept() {
    let f = fixture().await;
    let (audiobook, at) = audiobook_of_the_book(&f).await;
    f.core.seek_to(&audiobook, at).unwrap();
    f.core.merge_audiobook(&audiobook, &f.id).unwrap();
    let book = f.core.load_book(&f.id).unwrap();
    let first = book.chapters[0].segments[0].id.clone();
    f.core.set_reading_place(&f.id, 0, &first).unwrap();
    assert_eq!(pending_place(&f.core, &f.id), None);

    put_model(&f.root);
    f.core.models_changed();
    sync(&f.core);
    let progress = super::lock(&f.core.library)
        .progress(&aoide_library::BookId(f.id.clone()))
        .unwrap()
        .unwrap();
    assert_eq!(
        (progress.chapter, progress.segment_id),
        (0, Some(first)),
        "where the reader is, not where the audiobook was"
    );
}
