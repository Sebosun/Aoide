use super::prelude::*;

#[tauri::command]
pub fn browser_status(core: CoreState<'_>) -> crate::core::BrowserStatusView {
    core.browser_status()
}

#[tauri::command]
pub async fn download_browser(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.download_browser().await?)
}

#[tauri::command]
pub fn cancel_browser_download(core: CoreState<'_>) {
    core.cancel_browser_download();
}

#[tauri::command]
pub async fn remove_browser(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.remove_browser().await?)
}

#[tauri::command]
pub fn sites(core: CoreState<'_>) -> Result<Vec<crate::core::SiteView>, CoreError> {
    Ok(core.sites()?)
}

#[tauri::command]
pub fn set_site_rule(
    core: CoreState<'_>,
    host: String,
    fetch: Option<aoide_config::SiteFetch>,
    site_data: aoide_config::SiteData,
) -> Result<(), CoreError> {
    Ok(core.set_site_rule(&host, fetch, site_data)?)
}

#[tauri::command]
pub async fn sign_out_site(core: CoreState<'_>, host: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.sign_out_site(&host).await?)
}

#[tauri::command]
pub async fn forget_site(core: CoreState<'_>, host: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.forget_site(&host).await?)
}

#[tauri::command]
pub async fn clear_browser_data(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.clear_browser_data().await?)
}

#[tauri::command]
pub fn start_sign_in(
    core: CoreState<'_>,
    book_id: String,
) -> Result<crate::core::SignInView, CoreError> {
    Ok(core.start_sign_in(&book_id)?)
}

#[tauri::command]
pub fn sign_in(core: CoreState<'_>) -> Option<crate::core::SignInView> {
    core.sign_in()
}

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

#[tauri::command]
pub fn stay_signed_in(core: CoreState<'_>, stay: bool) -> Result<(), CoreError> {
    Ok(core.stay_signed_in(stay)?)
}

/// The system's file chooser for the Chrome or Chromium program to fetch with; `None` when
/// cancelled.
#[tauri::command]
pub async fn choose_browser(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Choose the Chrome or Chromium to fetch articles with")
            .pick_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}
