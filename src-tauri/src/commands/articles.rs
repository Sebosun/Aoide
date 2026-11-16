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
