use super::prelude::*;

/// Subscriptions, by title, with their unread counts and health.
#[tauri::command]
pub fn feeds(core: CoreState<'_>) -> Result<Vec<crate::core::FeedView>, CoreError> {
    Ok(core.feeds()?)
}

/// A link given to Add: a feed is subscribed, anything else is saved as an article.
#[tauri::command]
pub async fn add_link(
    core: CoreState<'_>,
    url: String,
) -> Result<crate::core::AddedLink, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.add_link(&url).await?)
}

/// Subscribes to a feed address, or to the one feed a page links to; a page with several feeds
/// offers them to choose from. Subscribing again finds the same feed.
#[tauri::command]
pub async fn subscribe(
    core: CoreState<'_>,
    url: String,
) -> Result<crate::core::SubscribeResult, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.subscribe(&url).await?)
}

/// Removes a feed and its entries that were not saved.
#[tauri::command]
pub fn unsubscribe(core: CoreState<'_>, feed_id: i64) -> Result<(), CoreError> {
    Ok(core.unsubscribe(feed_id)?)
}

/// Chooses the collection a feed's entries go on when they are saved (`null`: none).
#[tauri::command]
pub fn set_feed_collection(
    core: CoreState<'_>,
    feed_id: i64,
    collection_id: Option<i64>,
) -> Result<(), CoreError> {
    Ok(core.set_feed_collection(feed_id, collection_id)?)
}

/// Puts a feed in a folder (`null`: in none). A folder exists while a feed is in it.
#[tauri::command]
pub fn set_feed_folder(
    core: CoreState<'_>,
    feed_id: i64,
    folder: Option<String>,
) -> Result<(), CoreError> {
    Ok(core.set_feed_folder(feed_id, folder.as_deref())?)
}

/// Renames a feed folder; onto another folder's name, the two become one.
#[tauri::command]
pub fn rename_feed_folder(core: CoreState<'_>, from: String, to: String) -> Result<(), CoreError> {
    Ok(core.rename_feed_folder(&from, &to)?)
}

/// Entries newest first, from one feed, the feeds in a folder, or all; the next page starts
/// `before` an entry id.
#[tauri::command]
pub fn feed_entries(
    core: CoreState<'_>,
    feed_id: Option<i64>,
    folder: Option<String>,
    filter: crate::core::EntryFilter,
    before: Option<i64>,
    limit: Option<u32>,
) -> Result<Vec<crate::core::EntryView>, CoreError> {
    let query = crate::core::EntryQuery {
        feed: feed_id,
        folder,
        filter,
        before,
        limit,
    };
    Ok(core.feed_entries(&query)?)
}

/// The item to read an entry as (made on the feed shelf when needed); marks it read.
#[tauri::command]
pub fn open_feed_entry(core: CoreState<'_>, entry_id: i64) -> Result<String, CoreError> {
    Ok(core.open_feed_entry(entry_id)?)
}

/// Keeps an entry's item in the library; returns its id.
#[tauri::command]
pub fn save_feed_entry(core: CoreState<'_>, entry_id: i64) -> Result<String, CoreError> {
    Ok(core.save_feed_entry(entry_id)?)
}

#[tauri::command]
pub fn mark_entry_read(core: CoreState<'_>, entry_id: i64, read: bool) -> Result<(), CoreError> {
    Ok(core.mark_entry_read(entry_id, read)?)
}

/// Marks one feed's entries read, a folder's, or every feed's.
#[tauri::command]
pub fn mark_all_read(
    core: CoreState<'_>,
    feed_id: Option<i64>,
    folder: Option<String>,
) -> Result<(), CoreError> {
    Ok(core.mark_all_read(feed_id, folder.as_deref())?)
}

/// Keeps an item opened from a feed in the library (the reader's "Save to library").
#[tauri::command]
pub fn save_to_library(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    Ok(core.save_to_library(&book_id)?)
}

/// Opens a feed's site (its own address when it names none) in the system's browser.
#[tauri::command]
pub fn open_feed_site(core: CoreState<'_>, feed_id: i64) -> Result<(), CoreError> {
    Ok(core.open_feed_site(feed_id)?)
}

/// Checks every feed now ("feeds://changed" reports what changed).
#[tauri::command]
pub async fn check_feeds(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    core.check_feeds().await;
    Ok(())
}

/// The system's file chooser for a subscription list (OPML) to import; null when cancelled.
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

/// The system's save dialog for exporting the subscription list; null when cancelled.
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

/// Subscribes to the feeds an OPML file lists that are not subscribed yet.
#[tauri::command]
pub async fn import_opml(
    core: CoreState<'_>,
    path: String,
) -> Result<crate::core::OpmlImport, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.import_opml(&path)).await
}

/// Writes every subscription to an OPML file; resolves with how many.
#[tauri::command]
pub async fn export_opml(core: CoreState<'_>, path: String) -> Result<u32, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.export_opml(&path)).await
}
