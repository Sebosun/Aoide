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
