//! The queue through Core: items lined up play one after another.

use std::path::Path;
use std::sync::Arc;

use aoide_tts::testing::FakeEngine;

use super::queue::QueuePlace;
use super::tests::{epub_of_chapters, library_folder, start_with, until};
use super::Core;

/// Core with two items in its library: the test book and a short second one. Returns the
/// core and the two ids.
async fn two_items(root: &Path) -> (Arc<Core>, String, String) {
    let first = library_folder(root, "");
    let bytes = epub_of_chapters(&[("The Ferry", "The ferry left at noon.")]);
    std::fs::write(root.join("Books/the-ferry.epub"), &bytes).unwrap();
    let second = aoide_library::content_hash(&bytes).0;
    let engine = FakeEngine::new();
    let (core, _events) = start_with(
        root,
        &engine,
        Arc::new(|_| Err("no speech recognition in this test".into())),
    );
    until("the library scan", || {
        core.list_books().is_ok_and(|b| b.len() == 2)
    })
    .await;
    (core, first, second)
}

fn playing(core: &Core) -> Option<String> {
    core.player_state().book_id
}

#[tokio::test]
async fn an_item_that_plays_to_its_end_hands_over_to_the_queued_one() {
    let dir = tempfile::tempdir().unwrap();
    let (core, first, second) = two_items(dir.path()).await;
    core.queue_add(&second, QueuePlace::Last).unwrap();
    assert_eq!(core.queue.ids().unwrap(), std::slice::from_ref(&second));

    core.play(&first, Some(1), None).await.unwrap();
    until("the queued item to play", || {
        playing(&core).as_deref() == Some(second.as_str())
    })
    .await;
    assert!(
        core.queue.ids().unwrap().is_empty(),
        "taken off as it starts"
    );
    core.stop();
}

#[tokio::test]
async fn stopping_in_the_gap_keeps_the_queued_item_for_later() {
    let dir = tempfile::tempdir().unwrap();
    let (core, first, second) = two_items(dir.path()).await;
    core.queue_add(&second, QueuePlace::Last).unwrap();
    core.play(&first, Some(1), None).await.unwrap();
    until("the first item to end", || {
        core.playback.position().status == aoide_audio::Status::Finished
    })
    .await;
    core.stop();
    tokio::time::sleep(super::queue::GAP + std::time::Duration::from_millis(1500)).await;
    assert_eq!(playing(&core), None);
    assert_eq!(core.queue.ids().unwrap(), [second]);
}

#[tokio::test]
async fn an_item_that_ends_where_the_sleep_timer_waits_for_the_chapter_end_hands_over_to_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (core, first, second) = two_items(dir.path()).await;
    core.queue_add(&second, QueuePlace::Last).unwrap();
    core.play(&first, Some(1), None).await.unwrap();
    core.set_sleep_timer(None, true);
    until("the first item to end", || {
        core.playback.position().status == aoide_audio::Status::Finished
    })
    .await;
    tokio::time::sleep(super::queue::GAP + std::time::Duration::from_millis(1500)).await;
    assert_eq!(playing(&core).as_deref(), Some(first.as_str()));
    assert_eq!(core.queue.ids().unwrap(), [second]);
    core.stop();
}

#[tokio::test]
async fn a_queued_article_whose_fetch_got_nothing_is_marked_and_passed_over() {
    use super::queue::WhyNot;
    let dir = tempfile::tempdir().unwrap();
    let (core, first, second) = two_items(dir.path()).await;
    let article = core.save_article("https://quay.example/tides").unwrap();
    core.queue_add(&article, QueuePlace::Last).unwrap();
    core.queue_add(&second, QueuePlace::Last).unwrap();
    until("the fetch to fail", || {
        core.queue().unwrap().items[0].why_not == Some(WhyNot::NotFetched)
    })
    .await;
    assert_eq!(core.queue().unwrap().items[1].why_not, None);

    core.play(&first, Some(1), None).await.unwrap();
    until("the queued book to play", || {
        playing(&core).as_deref() == Some(second.as_str())
    })
    .await;
    assert_eq!(core.queue.ids().unwrap(), [article]);
    core.stop();
}
