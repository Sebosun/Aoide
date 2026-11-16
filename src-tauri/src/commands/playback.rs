use super::prelude::*;

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

/// Output gain, 0–1; saved with the speed.
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
