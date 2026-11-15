use super::prelude::*;

#[tauri::command]
pub fn feeds(core: CoreState<'_>) -> Result<Vec<crate::core::FeedView>, CoreError> {
    Ok(core.feeds()?)
}

#[tauri::command]
pub async fn add_link(
    core: CoreState<'_>,
    url: String,
) -> Result<crate::core::AddedLink, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.add_link(&url).await?)
}

#[tauri::command]
pub async fn subscribe(
    core: CoreState<'_>,
    url: String,
) -> Result<crate::core::SubscribeResult, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.subscribe(&url).await?)
}

#[tauri::command]
pub fn unsubscribe(core: CoreState<'_>, feed_id: i64) -> Result<(), CoreError> {
    Ok(core.unsubscribe(feed_id)?)
}

#[tauri::command]
pub fn feed_entries(
    core: CoreState<'_>,
    feed_id: Option<i64>,
    filter: crate::core::EntryFilter,
    before: Option<i64>,
    limit: Option<u32>,
) -> Result<Vec<crate::core::EntryView>, CoreError> {
    let query = crate::core::EntryQuery {
        feed: feed_id,
        filter,
        before,
        limit,
    };
    Ok(core.feed_entries(&query)?)
}

#[tauri::command]
pub fn open_feed_entry(core: CoreState<'_>, entry_id: i64) -> Result<String, CoreError> {
    Ok(core.open_feed_entry(entry_id)?)
}

#[tauri::command]
pub fn save_feed_entry(core: CoreState<'_>, entry_id: i64) -> Result<String, CoreError> {
    Ok(core.save_feed_entry(entry_id)?)
}

#[tauri::command]
pub fn mark_entry_read(core: CoreState<'_>, entry_id: i64, read: bool) -> Result<(), CoreError> {
    Ok(core.mark_entry_read(entry_id, read)?)
}

#[tauri::command]
pub fn mark_all_read(core: CoreState<'_>, feed_id: Option<i64>) -> Result<(), CoreError> {
    Ok(core.mark_all_read(feed_id)?)
}

#[tauri::command]
pub fn save_to_library(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    Ok(core.save_to_library(&book_id)?)
}

#[tauri::command]
pub fn open_feed_site(core: CoreState<'_>, feed_id: i64) -> Result<(), CoreError> {
    Ok(core.open_feed_site(feed_id)?)
}

#[tauri::command]
pub async fn check_feeds(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    core.check_feeds().await;
    Ok(())
}

/// The system's file chooser for a subscription list to import; `None` when cancelled.
#[tauri::command]
pub async fn choose_opml(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Choose a subscription list to import")
            .add_filter("Subscription lists (OPML)", &["opml", "xml"])
            .pick_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}

/// The system's save dialog for exporting the subscription list; `None` when cancelled.
#[tauri::command]
pub async fn choose_opml_target(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Export subscriptions")
            .set_file_name("subscriptions.opml")
            .add_filter("Subscription lists (OPML)", &["opml"])
            .save_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}

#[tauri::command]
pub async fn import_opml(
    core: CoreState<'_>,
    path: String,
) -> Result<crate::core::OpmlImport, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.import_opml(&path)).await
}

#[tauri::command]
pub async fn export_opml(core: CoreState<'_>, path: String) -> Result<u32, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.export_opml(&path)).await
}
