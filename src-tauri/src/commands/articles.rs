use super::prelude::*;

#[tauri::command]
pub fn save_article(core: CoreState<'_>, url: String) -> Result<String, CoreError> {
    Ok(core.save_article(&url)?)
}

#[tauri::command]
pub fn refetch_article(
    core: CoreState<'_>,
    book_id: String,
    source: crate::core::ArticleSource,
) -> Result<(), CoreError> {
    Ok(core.refetch_article(&book_id, source)?)
}

#[tauri::command]
pub fn article(
    core: CoreState<'_>,
    book_id: String,
) -> Result<crate::core::ArticleView, CoreError> {
    Ok(core.article(&book_id)?)
}
