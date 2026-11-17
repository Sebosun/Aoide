use super::prelude::*;

/// Saves a link as an article (or finds it saved) and queues its fetch; returns its id. An invalid
/// link is refused. "article://state" reports the fetch.
#[tauri::command]
pub fn save_article(core: CoreState<'_>, url: String) -> Result<String, CoreError> {
    Ok(core.save_article(&url)?)
}

/// Fetches an article again, from the page or an archived copy.
#[tauri::command]
pub fn refetch_article(
    core: CoreState<'_>,
    book_id: String,
    source: crate::core::ArticleSource,
) -> Result<(), CoreError> {
    Ok(core.refetch_article(&book_id, source)?)
}

/// An article's source and where its fetch stands.
#[tauri::command]
pub fn article(
    core: CoreState<'_>,
    book_id: String,
) -> Result<crate::core::ArticleView, CoreError> {
    Ok(core.article(&book_id)?)
}

/// The system's file dialog for another app's export of links; null when cancelled.
#[tauri::command]
pub async fn choose_links_file(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Import links from another app")
            .add_filter(
                "Exports and lists of links",
                &["csv", "html", "htm", "json", "zip", "txt"],
            )
            .pick_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}

/// Reads the export at `path`: which links it has. Nothing is made yet.
#[tauri::command]
pub async fn read_links(
    core: CoreState<'_>,
    path: String,
) -> Result<crate::core::LinkImportView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.read_links(std::path::Path::new(&path))).await
}

/// Imports the links chosen from those read.
#[tauri::command]
pub async fn start_link_import(
    core: CoreState<'_>,
    choice: crate::core::LinkChoice,
) -> Result<crate::core::LinkImportView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.start_link_import(choice)).await
}

/// The import of links as it is now.
#[tauri::command]
pub fn link_import(core: CoreState<'_>) -> crate::core::LinkImportView {
    core.link_import()
}

#[tauri::command]
pub fn pause_link_import(
    core: CoreState<'_>,
    paused: bool,
) -> Result<crate::core::LinkImportView, CoreError> {
    Ok(core.pause_link_import(paused)?)
}

/// Stops the import: links not fetched yet leave the library.
#[tauri::command]
pub async fn cancel_link_import(
    core: CoreState<'_>,
) -> Result<crate::core::LinkImportView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.cancel_link_import()).await
}

/// Tries an archived copy of each link that failed.
#[tauri::command]
pub async fn retry_link_import_archived(
    core: CoreState<'_>,
) -> Result<crate::core::LinkImportView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.retry_link_import_archived()).await
}

/// The summary was read: the import is over.
#[tauri::command]
pub fn dismiss_link_import(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.dismiss_link_import()?)
}
