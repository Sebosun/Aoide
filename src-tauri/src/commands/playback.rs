use super::prelude::*;
use crate::core::{QueuePlace, QueueView};

/// Starts playback at a segment (or the saved position when segmentId is null).
#[tauri::command]
pub async fn play(
    core: CoreState<'_>,
    book_id: String,
    chapter: Option<usize>,
    segment_id: Option<String>,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.play(&book_id, chapter, segment_id).await?)
}

#[tauri::command]
pub fn pause(core: CoreState<'_>) {
    core.pause();
}

#[tauri::command]
pub fn resume(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.resume()?)
}

#[tauri::command]
pub fn stop(core: CoreState<'_>) {
    core.stop();
}

#[tauri::command]
pub fn set_speed(core: CoreState<'_>, speed: f32) {
    core.set_speed(speed);
}

/// Gives the book its own playback speed (on: the speed it plays at, or the usual one when it
/// is not playing), or takes it away (off: it plays at the usual speed).
#[tauri::command]
pub fn set_book_speed(core: CoreState<'_>, book_id: String, on: bool) -> Result<(), CoreError> {
    Ok(core.set_book_speed(&book_id, on)?)
}

/// Output gain, 0–1; saved with the usual speed.
#[tauri::command]
pub fn set_volume(core: CoreState<'_>, volume: f32) {
    core.set_volume(volume);
}

/// minutes: pause after that long; endOfChapter: pause at the chapter end; neither: cancel.
#[tauri::command]
pub fn set_sleep_timer(core: CoreState<'_>, minutes: Option<u32>, end_of_chapter: bool) {
    core.set_sleep_timer(minutes, end_of_chapter);
}

/// Jump to the start of the next (+1) or previous (-1) chapter of the playing book.
#[tauri::command]
pub async fn skip_chapter(core: CoreState<'_>, delta: i64) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.skip_chapter(delta).await?)
}

/// Moves to positionMs on an audiobook's timeline (its saved place when it is not playing).
#[tauri::command]
pub fn seek(core: CoreState<'_>, book_id: String, position_ms: u64) -> Result<(), CoreError> {
    Ok(core.seek_to(&book_id, position_ms)?)
}

/// Moves deltaMs back or forward on an audiobook's timeline (the ±30 s buttons).
#[tauri::command]
pub fn skip(core: CoreState<'_>, book_id: String, delta_ms: i64) -> Result<(), CoreError> {
    Ok(core.skip(&book_id, delta_ms)?)
}

/// The queue: what plays next, each item with why it cannot be listened to now, if it cannot.
#[tauri::command]
pub fn queue(core: CoreState<'_>) -> Result<QueueView, CoreError> {
    Ok(core.queue()?)
}

/// Puts an item in the queue, next or last (moving it there when it is queued already).
#[tauri::command]
pub fn queue_add(core: CoreState<'_>, book_id: String, place: QueuePlace) -> Result<(), CoreError> {
    Ok(core.queue_add(&book_id, place)?)
}

/// Puts a feed entry's item in the queue without marking the entry read; returns the item.
#[tauri::command]
pub fn queue_add_entry(
    core: CoreState<'_>,
    entry_id: i64,
    place: QueuePlace,
) -> Result<String, CoreError> {
    Ok(core.queue_add_entry(entry_id, place)?)
}

#[tauri::command]
pub fn queue_remove(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    Ok(core.queue_remove(&book_id)?)
}

/// Moves a queued item to place `to` (0 is first).
#[tauri::command]
pub fn queue_move(core: CoreState<'_>, book_id: String, to: usize) -> Result<(), CoreError> {
    Ok(core.queue_move(&book_id, to)?)
}

#[tauri::command]
pub fn queue_clear(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.queue_clear()?)
}
