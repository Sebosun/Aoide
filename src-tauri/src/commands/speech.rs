use super::prelude::*;

#[tauri::command]
pub fn engine_status(core: CoreState<'_>) -> EngineStatusView {
    core.engine_status()
}

#[tauri::command]
pub fn book_settings(core: CoreState<'_>, book_id: String) -> BookSettings {
    core.book_settings(&book_id)
}

#[tauri::command]
pub async fn save_book_settings(
    core: CoreState<'_>,
    book_id: String,
    settings: BookSettings,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.save_book_settings(&book_id, settings).await?)
}

#[tauri::command]
pub fn engine_options(core: CoreState<'_>) -> EngineOptions {
    core.engine_options()
}

#[tauri::command]
pub async fn engine_voices(core: CoreState<'_>) -> Result<Vec<aoide_tts::Voice>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.engine_voices().await?)
}

/// The WAV goes back as raw bytes (an ArrayBuffer in the UI), not as a JSON number array.
#[tauri::command]
pub async fn preview_voice(
    core: CoreState<'_>,
    profile: aoide_tts::EngineProfile,
    voice_id: Option<String>,
    params: aoide_tts::Params,
    language: Option<String>,
) -> Result<tauri::ipc::Response, CoreError> {
    let core = Arc::clone(&core);
    let wav = core
        .preview_voice(profile, voice_id, params, language)
        .await?;
    Ok(tauri::ipc::Response::new(wav))
}

#[tauri::command]
pub async fn save_engine_profile(
    core: CoreState<'_>,
    profile: aoide_tts::EngineProfile,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.save_engine_profile(profile).await?)
}

#[tauri::command]
pub async fn prepare_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.prepare_book(&book_id).await?)
}

#[tauri::command]
pub async fn prepare_chapter(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.prepare_chapter(&book_id, chapter).await?)
}

#[tauri::command]
pub async fn chapter_audio(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<crate::core::ChapterAudioView>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.chapter_audio(&book_id).await?)
}

#[tauri::command]
pub fn set_prepare_paused(core: CoreState<'_>, book_id: String, paused: bool) {
    core.set_prepare_paused(&book_id, paused);
}

#[tauri::command]
pub fn cancel_prepare(core: CoreState<'_>, book_id: String) {
    core.cancel_prepare(&book_id);
}

#[tauri::command]
pub async fn regenerate_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.regenerate_book(&book_id).await?)
}
