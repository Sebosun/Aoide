use super::prelude::*;
use crate::core::{HighlightAdded, HighlightView};

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

/// Where the user is reading silently; returns the book's progress now, or null when ignored
/// because that book is playing.
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

/// The user is reading the book silently now; send every 15 s at most.
#[tauri::command]
pub async fn reading_activity(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || {
        core.reading_activity(&book_id);
        Ok::<(), String>(())
    })
    .await
}

/// Reads the book silently or listens to it. Reading the playing book pauses it; choosing to listen
/// does not start playback (call play next).
#[tauri::command]
pub async fn set_mode(
    core: CoreState<'_>,
    book_id: String,
    mode: ReadingMode,
) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.set_mode(&book_id, mode)).await
}

/// Bookmarks of a book, in reading order.
#[tauri::command]
pub async fn bookmarks(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::Bookmark>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.bookmarks(&book_id)).await
}

/// Bookmarks sentence `segmentId` when given, else the current playhead (or the saved position
/// of bookId when not playing it).
#[tauri::command]
pub async fn add_bookmark(
    core: CoreState<'_>,
    book_id: String,
    note: String,
    segment_id: Option<String>,
) -> Result<aoide_library::Bookmark, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.add_bookmark_at(&book_id, &note, segment_id.as_deref())).await
}

#[tauri::command]
pub fn update_bookmark_note(core: CoreState<'_>, id: i64, note: String) -> Result<(), CoreError> {
    Ok(core.update_bookmark_note(id, &note)?)
}

#[tauri::command]
pub fn delete_bookmark(core: CoreState<'_>, id: i64) -> Result<(), CoreError> {
    Ok(core.delete_bookmark(id)?)
}

/// A book's highlights in reading order, each kept on its text (a highlight whose text is no
/// longer in its chapter is `detached`); none for an audiobook.
#[tauri::command]
pub async fn highlights(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<HighlightView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.highlights(&book_id)).await
}

/// Highlights characters `start..end` of a chapter's text (as `SegmentView.starts` counts
/// them), joined with highlights it overlaps or touches; resolves with the book's highlights.
#[tauri::command]
pub async fn add_highlight(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
    start: usize,
    end: usize,
) -> Result<Vec<HighlightView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.add_highlight(&book_id, chapter, start, end)).await
}

/// Highlights the sentence just spoken in the book being listened to.
#[tauri::command]
pub async fn highlight_spoken(
    core: CoreState<'_>,
    book_id: String,
) -> Result<HighlightAdded, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.highlight_spoken(&book_id)).await
}

/// Resolves with the highlight's book's highlights.
#[tauri::command]
pub async fn update_highlight_note(
    core: CoreState<'_>,
    id: i64,
    note: String,
) -> Result<Vec<HighlightView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.update_highlight_note(id, &note)).await
}

/// Resolves with the highlight's book's highlights.
#[tauri::command]
pub async fn delete_highlight(
    core: CoreState<'_>,
    id: i64,
) -> Result<Vec<HighlightView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.delete_highlight(id)).await
}

/// A book's highlights, their notes and its bookmarks as Markdown.
#[tauri::command]
pub async fn notes_markdown(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.notes_markdown(&book_id)).await
}

/// The system's save dialog for a book's notes, offering `fileName`; null when cancelled.
#[tauri::command]
pub async fn choose_notes_target(app: tauri::AppHandle, file_name: String) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Export notes")
            .set_file_name(file_name)
            .add_filter("Markdown", &["md"])
            .save_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}

/// Writes a book's notes as Markdown to `path`.
#[tauri::command]
pub async fn export_notes(
    core: CoreState<'_>,
    book_id: String,
    path: String,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.export_notes(&book_id, &path)).await
}

/// The system's folder chooser for exporting every book's notes; null when cancelled.
#[tauri::command]
pub async fn choose_notes_folder(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Export all notes into a folder")
            .pick_folder(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}

/// Writes the notes of every book that has some into `folder`, one Markdown file per book;
/// resolves with how many files.
#[tauri::command]
pub async fn export_all_notes(core: CoreState<'_>, folder: String) -> Result<u32, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.export_all_notes(&folder)).await
}

/// One book's listened time per local day, oldest first.
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
