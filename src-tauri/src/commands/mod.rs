//! The Tauri commands the window calls, one module per area. Each forwards to [`Core`]; the
//! TypeScript contract (`src/api/contract.ts`) is generated from these functions.

pub mod app;
pub mod articles;
pub mod browser;
pub mod discovery;
pub mod feeds;
pub mod library;
pub mod playback;
pub mod reading;
pub mod recordings;
pub mod speech;

use std::sync::Arc;

use tauri::State;

use crate::core::{Core, CoreError};

/// What every command module needs.
mod prelude {
    pub(crate) use super::{blocking, pick, CoreState};
    pub(crate) use crate::book::ChapterView;
    pub(crate) use crate::core::{
        AppInfo, AppSettings, AppSettingsPatch, BookProgressView, BookSettings, BookSummary,
        BookView, CoreError, EngineOptions, EngineStatusView, HomeView, ImportResult,
        PreferencesView, ReadingMode,
    };
    pub(crate) use crate::logging;
    pub(crate) use std::sync::Arc;
}

pub type CoreState<'a> = State<'a, Arc<Core>>;

/// Runs `f` on a blocking thread; its error, whichever type, comes back as a `CoreError`.
pub async fn blocking<T: Send + 'static, E: Into<CoreError> + Send + 'static>(
    f: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, CoreError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(CoreError::internal)?
        .map_err(Into::into)
}

/// Shows one of the system's file dialogs: `show` opens it with the callback that receives
/// what was picked. `None` when cancelled.
pub async fn pick<T: Send + 'static>(
    app: &tauri::AppHandle,
    show: impl FnOnce(
        tauri_plugin_dialog::FileDialogBuilder<tauri::Wry>,
        Box<dyn FnOnce(Option<T>) + Send>,
    ),
) -> Option<T> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    show(
        app.dialog().file(),
        Box::new(move |picked| {
            let _ = tx.send(picked);
        }),
    );
    rx.await.ok().flatten()
}
