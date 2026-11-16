//! What the speech engine is asked to make after each kind of change, from Core's public seam:
//! one test per event that moves generation (listening, the prepare queue, the engine, lost
//! audio, the worker's own progress).

use std::sync::Arc;
use std::time::Duration;

use aoide_tts::EngineStatus;

use super::engine_tests::{profile, start, Scripted};
use super::tests::{until, Recorder};
use super::{BookSettings, Core, CoreEvent, PrepareStage};

/// The test book's spoken segments, each with its chapter.
fn speech(core: &Core, id: &str) -> Vec<(usize, String)> {
    let book = core.load_book(id).unwrap();
    (0..book.total_entries())
        .filter_map(|index| {
            let segment = book.segment(index)?;
            let (chapter, _) = book.locate(index)?;
            (!segment.speech.is_empty()).then(|| (chapter, segment.speech.clone()))
        })
        .collect()
}

fn texts(core: &Core, id: &str, chapter: Option<usize>) -> Vec<String> {
    speech(core, id)
        .into_iter()
        .filter(|(c, _)| chapter.is_none_or(|wanted| *c == wanted))
        .map(|(_, text)| text)
        .collect()
}

fn stage(core: &Core, id: &str) -> Option<PrepareStage> {
    core.prepare_view(id).map(|v| v.state)
}

/// Lets a moment pass for work that should not happen.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

fn last_generation(events: &Recorder) -> serde_json::Value {
    events
        .payloads(CoreEvent::GENERATION_STATE)
        .pop()
        .unwrap_or_default()
}

async fn ready(
    voices: &[&str],
) -> (
    tempfile::TempDir,
    Arc<Scripted>,
    Arc<Core>,
    Arc<Recorder>,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new("main", EngineStatus::Ready(None), voices, false);
    let (core, events, id) = start(dir.path(), &[&engine], "voiceId = \"A.wav\"").await;
    (dir, engine, core, events, id)
}

// ------------------------------------------------------------ listening

#[tokio::test]
async fn playing_from_a_chapter_asks_for_that_chapter_first() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    core.play(&id, Some(1), None).await.unwrap();
    until("speech for the playhead", || !engine.texts().is_empty()).await;
    assert_eq!(engine.texts()[0], texts(&core, &id, Some(1))[0]);
    core.stop();
}

#[tokio::test]
async fn moving_the_playhead_asks_for_speech_at_the_new_place() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.play(&id, Some(0), None).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;

    core.skip_chapter(1).await.unwrap();
    engine.allow(1);
    until("the next request", || engine.texts().len() >= 2).await;

    assert_eq!(engine.texts()[1], texts(&core, &id, Some(1))[0]);
    engine.let_go();
    core.stop();
}

#[tokio::test]
async fn stopping_asks_for_nothing_more_and_clears_the_generation_state() {
    let (_dir, engine, core, events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.play(&id, Some(0), None).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;

    core.stop();
    engine.let_go();
    settle().await;

    assert_eq!(engine.texts().len(), 1);
    assert_eq!(core.generation_state().book_id, None);
    assert_eq!(last_generation(&events)["bookId"], serde_json::Value::Null);
}

// ------------------------------------------------------------ the prepare queue

#[tokio::test]
async fn queueing_a_book_asks_for_all_of_it_and_finishes_it() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    core.prepare_book(&id).await.unwrap();
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    assert_eq!(engine.texts(), texts(&core, &id, None));
}

#[tokio::test]
async fn queueing_a_chapter_asks_for_that_chapter_only() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    core.prepare_chapter(&id, 1).await.unwrap();
    until("the chapter's speech", || {
        engine.texts().len() == texts(&core, &id, Some(1)).len()
    })
    .await;
    settle().await;
    assert_eq!(engine.texts(), texts(&core, &id, Some(1)));
}

#[tokio::test]
async fn cancelling_a_book_asks_for_nothing_more_of_it() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;

    core.cancel_prepare(&id);
    engine.let_go();
    settle().await;

    assert_eq!(engine.texts().len(), 1);
    assert_eq!(stage(&core, &id), None, "no longer queued");
}

#[tokio::test]
async fn pausing_a_book_holds_its_speech_until_it_is_resumed() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;

    core.set_prepare_paused(&id, true);
    engine.let_go();
    settle().await;
    assert_eq!(engine.texts().len(), 1);

    core.set_prepare_paused(&id, false);
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    assert_eq!(engine.texts(), texts(&core, &id, None));
}

#[tokio::test]
async fn removing_a_book_asks_for_nothing_more_of_it() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;

    core.remove_book(&id).unwrap();
    engine.let_go();
    settle().await;

    assert_eq!(engine.texts().len(), 1);
}

#[tokio::test]
async fn changing_a_books_voice_while_it_is_prepared_asks_for_the_rest_in_the_new_voice() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav", "B.wav"]).await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;

    let settings = BookSettings {
        voice_id: Some("B.wav".into()),
        ..Default::default()
    };
    core.save_book_settings(&id, settings).await.unwrap();
    engine.let_go();
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    let voices = engine.voices_used();
    assert_eq!(voices[0], "A.wav");
    assert!(voices[1..].iter().all(|v| v == "B.wav"), "{voices:?}");
    assert_eq!(
        voices.len(),
        texts(&core, &id, None).len() + 1,
        "the first again in B"
    );
}

// ------------------------------------------------------------ the engine

#[tokio::test]
async fn switching_engines_asks_the_new_one_for_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let first = Scripted::new("first", EngineStatus::Ready(None), &["A.wav"], false);
    let second = Scripted::new("second", EngineStatus::Ready(None), &["A.wav"], false);
    let (core, _events, id) = start(dir.path(), &[&first, &second], "voiceId = \"A.wav\"").await;
    first.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || first.texts().len() == 1).await;

    let mut next = profile("second");
    next.voice_id = Some("A.wav".into());
    core.save_engine_profile(next).await.unwrap();
    first.let_go();
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    // The old engine's worker may finish what it had started; the new one makes the rest.
    assert!(!second.texts().is_empty());
    let made: Vec<String> = first.texts().into_iter().chain(second.texts()).collect();
    assert!(
        texts(&core, &id, None).iter().all(|t| made.contains(t)),
        "{made:?}"
    );
}

#[tokio::test]
async fn once_the_engines_voices_are_known_the_rest_is_asked_for_in_one_it_offers() {
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new("late", EngineStatus::Offline, &["Real.wav"], false);
    engine.voices_fail.store(true, Ordering::SeqCst);
    let (core, _events, id) = start(dir.path(), &[&engine], "voiceId = \"Gone.wav\"").await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;
    // Paused, so nothing is being made while the engine is looked at.
    core.set_prepare_paused(&id, true);
    engine.let_go();
    settle().await;

    engine.voices_fail.store(false, Ordering::SeqCst);
    let listed = engine.listings_answered.load(Ordering::SeqCst);
    engine.set_status(EngineStatus::Ready(None));
    for _ in 0..400 {
        if engine.listings_answered.load(Ordering::SeqCst) > listed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    settle().await;
    core.set_prepare_paused(&id, false);
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    let voices = engine.voices_used();
    assert_eq!(voices[0], "Gone.wav");
    assert!(voices[1..].iter().all(|v| v == "Real.wav"), "{voices:?}");
}

// ------------------------------------------------------------ lost audio

#[tokio::test]
async fn audio_deleted_to_keep_the_cache_small_is_asked_for_again_when_needed() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    core.prepare_book(&id).await.unwrap();
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    let made = engine.texts().len();

    core.update_app_settings(super::AppSettingsPatch {
        cache_limit_bytes: Some(Some(1)),
        ..Default::default()
    })
    .unwrap();
    assert_ne!(
        stage(&core, &id),
        Some(PrepareStage::Done),
        "no longer prepared"
    );

    core.play(&id, Some(0), None).await.unwrap();
    until("speech made again", || engine.texts().len() > made).await;
    assert_eq!(engine.texts()[made], texts(&core, &id, Some(0))[0]);
    core.stop();
}

#[tokio::test]
async fn audio_deleted_to_be_made_again_is_asked_for_again() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    core.prepare_book(&id).await.unwrap();
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    let made = engine.texts().len();

    core.regenerate_book(&id).await.unwrap();
    assert_ne!(stage(&core, &id), Some(PrepareStage::Done));
    core.prepare_book(&id).await.unwrap();
    until("the book to be prepared again", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;

    assert_eq!(engine.texts()[made..], texts(&core, &id, None)[..]);
}

// ------------------------------------------------------------ the worker's progress

#[tokio::test]
async fn each_segment_made_asks_for_the_next() {
    let (_dir, engine, core, _events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the first request", || engine.texts().len() == 1).await;
    settle().await;
    assert_eq!(engine.texts().len(), 1, "one at a time");

    engine.allow(1);
    until("the second request", || engine.texts().len() == 2).await;
    engine.allow(1);
    until("the third request", || engine.texts().len() == 3).await;

    assert_eq!(engine.texts(), texts(&core, &id, None)[..3]);
    engine.let_go();
}

#[tokio::test]
async fn the_segment_being_made_is_reported() {
    let (_dir, engine, core, events, id) = ready(&["A.wav"]).await;
    engine.hold_synthesis();
    core.prepare_book(&id).await.unwrap();
    until("the segment being made", || {
        last_generation(&events)["generatingSegmentId"].is_string()
    })
    .await;
    engine.let_go();
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    until("nothing being made", || {
        last_generation(&events)["generatingSegmentId"].is_null()
    })
    .await;
}

#[tokio::test]
async fn a_failed_segment_is_reported() {
    let (_dir, engine, core, events, id) = ready(&["A.wav"]).await;
    engine
        .synthesis_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    core.prepare_book(&id).await.unwrap();
    until("the failure", || {
        last_generation(&events)["error"].is_string()
    })
    .await;
    assert!(last_generation(&events)["generatingSegmentId"].is_null());
    // The player says what happened; Settings has the whole error.
    assert_eq!(
        last_generation(&events)["error"],
        "The speech engine is unavailable"
    );
    let problems = core.speech_problems();
    assert_eq!(problems[0].message, "The speech engine is unavailable");
    assert!(
        problems[0].detail.contains("the engine fell over"),
        "{problems:?}"
    );
}

// ------------------------------------------------------------ a launched server

#[tokio::test]
async fn nothing_is_asked_of_a_launched_server_until_it_is_ready_and_no_problem_is_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let engine = Scripted::new("main", EngineStatus::Offline, &["A.wav"], false);
    engine
        .synthesis_fails
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let launch = "voiceId = \"A.wav\"\n[engine.launch]\nprogram = \"sh\"\nargs = [\"-c\", \"exec sleep 300\"]";
    let (core, _events, id) = start(dir.path(), &[&engine], launch).await;

    core.prepare_book(&id).await.unwrap();
    until("the launch", || {
        core.engine_status().detail.as_deref() == Some("Starting the voice…")
    })
    .await;
    settle().await;
    assert!(engine.texts().is_empty(), "{:?}", engine.texts());
    assert!(core.speech_problems().is_empty());

    engine
        .synthesis_fails
        .store(false, std::sync::atomic::Ordering::SeqCst);
    engine.set_status(EngineStatus::Ready(None));
    until("the book to be prepared", || {
        stage(&core, &id) == Some(PrepareStage::Done)
    })
    .await;
    assert!(core.speech_problems().is_empty());
    assert_eq!(engine.texts().len(), texts(&core, &id, None).len());
    core.shutdown();
}
