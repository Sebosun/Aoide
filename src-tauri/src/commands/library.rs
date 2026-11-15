use super::prelude::*;

#[tauri::command]
pub async fn list_books(core: CoreState<'_>) -> Result<Vec<BookSummary>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.list_books()).await
}

#[tauri::command]
pub async fn home(core: CoreState<'_>) -> Result<HomeView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.home()).await
}

/// Adds an audiobook with no EPUB: an M4B, audio files, or a folder of them.
#[tauri::command]
pub async fn add_audiobook(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<ImportResult, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, String>(core.add_audiobook(paths))).await
}

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

#[tauri::command]
pub async fn show_in_folder(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.show_in_folder(&book_id)).await
}

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

#[tauri::command]
pub fn collections(core: CoreState<'_>) -> Result<Vec<aoide_library::Collection>, CoreError> {
    Ok(core.collections()?)
}

#[tauri::command]
pub fn create_collection(core: CoreState<'_>, name: String) -> Result<i64, CoreError> {
    Ok(core.create_collection(&name)?)
}

#[tauri::command]
pub fn rename_collection(core: CoreState<'_>, id: i64, name: String) -> Result<(), CoreError> {
    Ok(core.rename_collection(id, &name)?)
}

#[tauri::command]
pub fn delete_collection(core: CoreState<'_>, id: i64) -> Result<(), CoreError> {
    Ok(core.delete_collection(id)?)
}

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

#[tauri::command]
pub fn clean_unused_files(core: CoreState<'_>) -> Result<crate::core::CleanResult, CoreError> {
    Ok(core.clean_unused_files()?)
}

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

#[tauri::command]
pub async fn lookup_metadata(
    core: CoreState<'_>,
    book_id: String,
    refresh: bool,
) -> Result<Vec<aoide_metadata::Match>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.lookup_metadata(&book_id, refresh).await?)
}

#[tauri::command]
pub async fn apply_metadata(
    core: CoreState<'_>,
    book_id: String,
    candidate: aoide_metadata::Match,
) -> Result<aoide_library::BookDetails, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.apply_metadata(&book_id, candidate).await?)
}

#[tauri::command]
pub async fn export_book(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.export_book(&book_id).await?)
}

#[tauri::command]
pub async fn export_read_along(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.export_read_along(&book_id).await?)
}
