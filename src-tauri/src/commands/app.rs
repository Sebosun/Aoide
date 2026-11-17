use super::prelude::*;

/// Writes a line to Aoide's log (a failure the window saw).
#[tauri::command]
pub fn log_ui(entry: logging::UiLog) {
    logging::ui(entry);
}

#[tauri::command]
pub fn app_info(core: CoreState<'_>) -> AppInfo {
    core.app_info()
}

/// The items a start of Aoide asked to open before the window was listening (a book opened
/// from the file manager); called once the window listens for `item://open`.
#[tauri::command]
pub fn take_open_requests(core: CoreState<'_>) -> Vec<String> {
    core.take_open_requests()
}

/// The inbox folder, and whether it is being watched.
#[tauri::command]
pub fn inbox_status(core: CoreState<'_>) -> crate::core::InboxView {
    core.inbox_status()
}

#[tauri::command]
pub fn app_settings(core: CoreState<'_>) -> AppSettings {
    core.app_settings()
}

/// Merges a change into the app settings; resolves with the settings now saved.
#[tauri::command]
pub async fn update_app_settings(
    core: CoreState<'_>,
    patch: AppSettingsPatch,
) -> Result<AppSettings, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.update_app_settings(patch)).await
}

/// Appearance, themes and config problems from ~/.config/aoide.
#[tauri::command]
pub fn preferences(core: CoreState<'_>) -> PreferencesView {
    core.preferences()
}

#[tauri::command]
pub async fn save_appearance(
    core: CoreState<'_>,
    appearance: aoide_config::Appearance,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.save_appearance(appearance)).await
}

/// Makes the window fill the screen, or windowed again. The window then says it changed
/// (`window://fullscreen`), as it does when the window manager's own shortcut changes it.
#[tauri::command]
pub fn set_fullscreen(window: tauri::WebviewWindow, on: bool) -> Result<(), CoreError> {
    window.set_fullscreen(on).map_err(|error| {
        tracing::warn!(
            event = "window.set_fullscreen",
            on,
            outcome = "failed",
            %error,
            "The window cannot change to or from fullscreen"
        );
        CoreError::from(error.to_string())
    })
}
