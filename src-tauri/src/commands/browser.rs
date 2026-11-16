use super::prelude::*;

/// What browser would run for articles, and its download.
#[tauri::command]
pub fn browser_status(core: CoreState<'_>) -> crate::core::BrowserStatusView {
    core.browser_status()
}

/// Downloads the pinned Chrome for Testing (progress on "browser://download"); refused while the
/// browser is off. Resolves once done, cancelled or failed.
#[tauri::command]
pub async fn download_browser(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.download_browser().await?)
}

/// Stops a browser download; downloading again continues it.
#[tauri::command]
pub fn cancel_browser_download(core: CoreState<'_>) {
    core.cancel_browser_download();
}

/// Deletes the downloaded browser.
#[tauri::command]
pub async fn remove_browser(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.remove_browser().await?)
}

/// Sites that needed the user or have a rule, by host.
#[tauri::command]
pub fn sites(core: CoreState<'_>) -> Result<Vec<crate::core::SiteView>, CoreError> {
    Ok(core.sites()?)
}

/// How a site is fetched (null: as the global choice) and whether its data is kept; written to
/// site-rules.toml.
#[tauri::command]
pub fn set_site_rule(
    core: CoreState<'_>,
    host: String,
    fetch: Option<aoide_config::SiteFetch>,
    site_data: aoide_config::SiteData,
) -> Result<(), CoreError> {
    Ok(core.set_site_rule(&host, fetch, site_data)?)
}

/// Clears the site's cookies and storage in the browser (refused while it is off).
#[tauri::command]
pub async fn sign_out_site(core: CoreState<'_>, host: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.sign_out_site(&host).await?)
}

/// Signs out of the site and drops its row and rule.
#[tauri::command]
pub async fn forget_site(core: CoreState<'_>, host: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.forget_site(&host).await?)
}

/// Removes the browser's profile: every sign-in and all site data.
#[tauri::command]
pub async fn clear_browser_data(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.clear_browser_data().await?)
}

/// Opens a browser window on the article's page to sign in or pass a check; the article is fetched
/// again once the window shows it whole or `finishSignIn` is called. One at a time; refused while
/// the browser is off. Its stages arrive on "signin://state".
#[tauri::command]
pub fn start_sign_in(
    core: CoreState<'_>,
    book_id: String,
) -> Result<crate::core::SignInView, CoreError> {
    Ok(core.start_sign_in(&book_id)?)
}

/// The sign-in open or last finished, if any.
#[tauri::command]
pub fn sign_in(core: CoreState<'_>) -> Option<crate::core::SignInView> {
    core.sign_in()
}

/// "I'm signed in, fetch again".
#[tauri::command]
pub fn finish_sign_in(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.finish_sign_in()?)
}

#[tauri::command]
pub fn cancel_sign_in(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.cancel_sign_in()?)
}

#[tauri::command]
pub fn show_sign_in_window(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.show_sign_in_window()?)
}

/// Whether the site's cookies are kept after the fetch.
#[tauri::command]
pub fn stay_signed_in(core: CoreState<'_>, stay: bool) -> Result<(), CoreError> {
    Ok(core.stay_signed_in(stay)?)
}

/// The system's file chooser for the browser's program; null when cancelled.
#[tauri::command]
pub async fn choose_browser(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Choose the Chrome or Chromium to fetch articles with")
            .pick_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}
