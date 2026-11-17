use super::prelude::*;
use std::path::Path;

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

/// The answer to "Keep working in the background?" (`window://close-asked`); `remember` keeps
/// it for next time. The window then hides, minimises or quits as the core says.
#[tauri::command]
pub fn choose_close(
    app: tauri::AppHandle,
    core: CoreState<'_>,
    keep: bool,
    remember: bool,
) -> Result<(), CoreError> {
    let action = core.choose_close(keep, remember)?;
    crate::background::act(&app, action);
    Ok(())
}

/// Writes a backup of the library to `path`.
#[tauri::command]
pub async fn back_up(
    core: CoreState<'_>,
    path: String,
    options: crate::core::BackupOptions,
) -> Result<crate::core::BackupWritten, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.back_up(Path::new(&path), options)).await
}

/// What the backup at `path` holds, and whether it can be restored here.
#[tauri::command]
pub async fn inspect_backup(
    core: CoreState<'_>,
    path: String,
) -> Result<crate::core::BackupInfoView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.inspect_backup(Path::new(&path))).await
}

/// Restores the backup at `path`: it is checked and set up (the current library saved first),
/// then Aoide restarts and puts it in place before opening the library.
#[tauri::command]
pub async fn restore_backup(
    app: tauri::AppHandle,
    core: CoreState<'_>,
    path: String,
) -> Result<(), CoreError> {
    let restoring = Arc::clone(&core);
    blocking(move || restoring.restore(Path::new(&path))).await?;
    tracing::info!(
        event = "backup.restored",
        outcome = "staged",
        "Restarting to restore a backup"
    );
    app.restart();
}

/// The system's save dialog for a backup; null when cancelled.
#[tauri::command]
pub async fn choose_backup_target(app: tauri::AppHandle) -> Option<String> {
    let name = format!(
        "Aoide backup {}.{}",
        chrono::Local::now().format("%Y-%m-%d"),
        crate::core::BACKUP_EXTENSION
    );
    pick(&app, move |dialog, done| {
        dialog
            .set_title("Back up the library")
            .set_file_name(&name)
            .add_filter("Aoide backups", &[crate::core::BACKUP_EXTENSION])
            .save_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}

/// The system's open dialog for a backup to restore; null when cancelled.
#[tauri::command]
pub async fn choose_backup_file(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Restore a backup")
            .add_filter("Aoide backups", &[crate::core::BACKUP_EXTENSION])
            .pick_file(move |p| done(p.map(|p| p.to_string())))
    })
    .await
}
