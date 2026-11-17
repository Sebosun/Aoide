use super::prelude::*;

#[tauri::command]
pub async fn list_books(core: CoreState<'_>) -> Result<Vec<BookSummary>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.list_books()).await
}

/// Home's lists, as ids of library items.
#[tauri::command]
pub async fn home(core: CoreState<'_>) -> Result<HomeView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.home()).await
}

/// Adds an audiobook with no EPUB (an M4B, audio files or a folder of them); the same files again
/// are the same item. Refusals (DRM, unreadable) come back as the error.
#[tauri::command]
pub async fn add_audiobook(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<ImportResult, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, String>(core.add_audiobook(paths))).await
}

/// Copies dropped EPUB files into the library folder and imports them.
#[tauri::command]
pub async fn import_files(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<Vec<ImportResult>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.import_files(paths))).await
}

#[tauri::command]
pub async fn rescan_library(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.rescan()).await
}

#[tauri::command]
pub async fn remove_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.remove_book(&book_id)).await
}

/// Opens the file manager on the book's folder with its EPUB selected.
#[tauri::command]
pub async fn show_in_folder(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.show_in_folder(&book_id)).await
}

/// Opens an article's page, as it was kept, in the system's browser.
#[tauri::command]
pub async fn open_original(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.open_original(&book_id)).await
}

#[tauri::command]
pub async fn open_book(core: CoreState<'_>, book_id: String) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.open_book(&book_id)).await
}

/// Like openBook, without marking the book as recently opened.
#[tauri::command]
pub async fn book_overview(core: CoreState<'_>, book_id: String) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.book_overview(&book_id)).await
}

#[tauri::command]
pub async fn chapter(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
) -> Result<ChapterView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.chapter(&book_id, chapter)).await
}

/// By name.
#[tauri::command]
pub fn collections(core: CoreState<'_>) -> Result<Vec<aoide_library::Collection>, CoreError> {
    Ok(core.collections()?)
}

/// Fails when the name is empty or already used (ignoring case). Returns the new id.
#[tauri::command]
pub fn create_collection(core: CoreState<'_>, name: String) -> Result<i64, CoreError> {
    Ok(core.create_collection(&name)?)
}

#[tauri::command]
pub fn rename_collection(core: CoreState<'_>, id: i64, name: String) -> Result<(), CoreError> {
    Ok(core.rename_collection(id, &name)?)
}

/// The books stay in the library.
#[tauri::command]
pub fn delete_collection(core: CoreState<'_>, id: i64) -> Result<(), CoreError> {
    Ok(core.delete_collection(id)?)
}

/// Every tag on some item, with how many items have it, by name.
#[tauri::command]
pub fn tags(core: CoreState<'_>) -> Result<Vec<aoide_library::TagCount>, CoreError> {
    Ok(core.tags()?)
}

/// The user's tags for an item: exactly these are shown; the others stay removed (reading its
/// subjects again does not bring them back).
#[tauri::command]
pub fn set_book_tags(
    core: CoreState<'_>,
    book_id: String,
    tags: Vec<String>,
) -> Result<(), CoreError> {
    Ok(core.set_book_tags(&book_id, &tags)?)
}

/// Asks the tagging model (Settings › Library) for an item's tags; keeps and returns them.
#[tauri::command]
pub async fn tag_with_model(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<String>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.tag_with_model(&book_id).await?)
}

/// Asks the tagging model for the tags of every item that has none, one after another.
#[tauri::command]
pub async fn tag_untagged_with_model(
    core: CoreState<'_>,
) -> Result<crate::core::TaggingRun, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.tag_untagged_with_model().await?)
}

/// Archives an item (`on`), or takes it out of the archive. Nothing of it is deleted.
#[tauri::command]
pub fn archive(core: CoreState<'_>, book_id: String, on: bool) -> Result<(), CoreError> {
    Ok(core.archive(&book_id, on)?)
}

/// Puts the book on exactly these collections.
#[tauri::command]
pub fn set_book_collections(
    core: CoreState<'_>,
    book_id: String,
    ids: Vec<i64>,
) -> Result<(), CoreError> {
    Ok(core.set_book_collections(&book_id, &ids)?)
}

#[tauri::command]
pub fn storage_report(core: CoreState<'_>) -> Result<crate::core::StorageReport, CoreError> {
    Ok(core.storage_report()?)
}

/// Deletes unused files (see StorageReport) and returns what was freed.
#[tauri::command]
pub fn clean_unused_files(core: CoreState<'_>) -> Result<crate::core::CleanResult, CoreError> {
    Ok(core.clean_unused_files()?)
}

/// Case-insensitive text search (min 2 chars), at most 200 hits in reading order.
#[tauri::command]
pub async fn search_book(
    core: CoreState<'_>,
    book_id: String,
    query: String,
) -> Result<Vec<crate::book::SearchHit>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.search_book(&book_id, &query)).await
}

#[tauri::command]
pub async fn book_details(
    core: CoreState<'_>,
    book_id: String,
) -> Result<aoide_library::BookDetails, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.book_details(&book_id)).await
}

/// Searches Open Library for the book (fails when lookups are turned off). Cached for 30 days;
/// refresh asks Open Library again.
#[tauri::command]
pub async fn lookup_metadata(
    core: CoreState<'_>,
    book_id: String,
    refresh: bool,
) -> Result<Vec<aoide_metadata::Match>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.lookup_metadata(&book_id, refresh).await?)
}

/// Corrects an item's title, authors, cover (an image's path) or publication date by hand;
/// `null` sets a field back to the file's. The file is not changed.
#[tauri::command]
pub fn edit_details(
    core: CoreState<'_>,
    book_id: String,
    patch: crate::core::DetailsPatch,
) -> Result<(), CoreError> {
    Ok(core.edit_details(&book_id, patch)?)
}

/// Stores the picked match's details (and cover when the book has none).
#[tauri::command]
pub async fn apply_metadata(
    core: CoreState<'_>,
    book_id: String,
    candidate: aoide_metadata::Match,
) -> Result<aoide_library::BookDetails, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.apply_metadata(&book_id, candidate).await?)
}

/// Writes an M4B audiobook (generating missing audio first); resolves with the output path.
#[tauri::command]
pub async fn export_book(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.export_book(&book_id).await?)
}

/// Writes the book with its synced recording as an EPUB 3 with Media Overlays; returns its path.
#[tauri::command]
pub async fn export_read_along(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.export_read_along(&book_id).await?)
}
