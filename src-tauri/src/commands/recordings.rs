use super::prelude::*;

#[tauri::command]
pub async fn inspect_recording(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<crate::core::InspectionView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.inspect_recording(paths))).await
}

#[tauri::command]
pub async fn attach_recording(
    core: CoreState<'_>,
    book_id: String,
    paths: Vec<String>,
    as_listed: Option<bool>,
) -> Result<crate::core::RecordingView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || {
        if as_listed == Some(true) {
            core.attach_recording_as_listed(&book_id, paths)
        } else {
            core.attach_recording(&book_id, paths)
        }
    })
    .await
}

#[tauri::command]
pub async fn recording(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Option<crate::core::RecordingView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.recording(&book_id))).await
}

#[tauri::command]
pub async fn recordings(core: CoreState<'_>) -> Result<Vec<crate::core::RecordingView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.recordings())).await
}

#[tauri::command]
pub async fn sync_report(
    core: CoreState<'_>,
    book_id: String,
) -> Result<crate::core::SyncReportView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.sync_report(&book_id)).await
}

#[tauri::command]
pub async fn sync_action(
    core: CoreState<'_>,
    book_id: String,
    action: crate::core::SyncAction,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.sync_action(&book_id, action)).await
}

#[tauri::command]
pub async fn remove_recording(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.remove_recording(&book_id).await?)
}

#[tauri::command]
pub async fn set_narration(
    core: CoreState<'_>,
    book_id: String,
    narration: crate::core::Narration,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.set_narration(&book_id, narration).await?)
}

#[tauri::command]
pub fn recognition_options(core: CoreState<'_>) -> crate::core::RecognitionOptions {
    core.recognition_options()
}

#[tauri::command]
pub async fn download_model(core: CoreState<'_>, model_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.download_model(&model_id).await?)
}

#[tauri::command]
pub async fn play_recording_at(
    core: CoreState<'_>,
    book_id: String,
    at_ms: u64,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.play_recording_at(&book_id, at_ms).await?)
}
