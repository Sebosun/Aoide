use super::prelude::*;

#[tauri::command]
pub async fn mark_finished(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.mark_finished(&book_id)).await
}

/// Forgets that the book was finished and where the user was in it.
#[tauri::command]
pub async fn mark_unread(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.mark_unread(&book_id)).await
}

/// Where the user is reading silently, and the progress it makes; ignored while that book plays.
#[tauri::command]
pub async fn set_reading_place(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
    segment_id: String,
) -> Result<Option<BookProgressView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.set_reading_place(&book_id, chapter, &segment_id)).await
}

/// The user is reading the book silently now (sent every 15 s at most).
#[tauri::command]
pub async fn reading_activity(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || {
        core.reading_activity(&book_id);
        Ok::<(), String>(())
    })
    .await
}

/// Reads book `book_id` silently or listens to it; reading the playing book pauses it.
#[tauri::command]
pub async fn set_mode(
    core: CoreState<'_>,
    book_id: String,
    mode: ReadingMode,
) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.set_mode(&book_id, mode)).await
}

#[tauri::command]
pub async fn bookmarks(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::Bookmark>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.bookmarks(&book_id)).await
}

#[tauri::command]
pub async fn add_bookmark(
    core: CoreState<'_>,
    book_id: String,
    note: String,
) -> Result<aoide_library::Bookmark, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.add_bookmark(&book_id, &note)).await
}

#[tauri::command]
pub fn update_bookmark_note(core: CoreState<'_>, id: i64, note: String) -> Result<(), CoreError> {
    Ok(core.update_bookmark_note(id, &note)?)
}

#[tauri::command]
pub fn delete_bookmark(core: CoreState<'_>, id: i64) -> Result<(), CoreError> {
    Ok(core.delete_bookmark(id)?)
}

#[tauri::command]
pub async fn book_listening(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::DayListening>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.book_listening(&book_id)).await
}

/// Reading and listening time, per day and per book.
#[tauri::command]
pub async fn activity_stats(
    core: CoreState<'_>,
) -> Result<aoide_library::ActivityStats, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.activity_stats()).await
}
