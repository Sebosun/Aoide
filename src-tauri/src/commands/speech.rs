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

/// What went wrong with the speech engine lately, newest first (Settings › Speech engine).
#[tauri::command]
pub fn speech_problems(core: CoreState<'_>) -> Vec<crate::core::SpeechProblem> {
    core.speech_problems()
}

/// Forgets the speech engine's problems listed so far.
#[tauri::command]
pub fn clear_speech_problems(core: CoreState<'_>) {
    core.clear_speech_problems();
}

/// Try again: starts a speech server that failed once more.
#[tauri::command]
pub fn retry_speech_server(core: CoreState<'_>) {
    core.retry_speech_server();
}

/// Stop now: stops the speech server Aoide started; it starts again when speech is needed.
#[tauri::command]
pub fn stop_speech_server(core: CoreState<'_>) {
    core.stop_speech_server();
}

/// Shows the speech server's log (`speech-server.log`) in the file manager.
#[tauri::command]
pub fn show_speech_server_log(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.show_speech_server_log()?)
}

/// Registered adapters and the active global profile.
#[tauri::command]
pub fn engine_options(core: CoreState<'_>) -> EngineOptions {
    core.engine_options()
}

/// Voices of the active engine (fails when the engine is offline).
#[tauri::command]
pub async fn engine_voices(core: CoreState<'_>) -> Result<Vec<aoide_tts::Voice>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.engine_voices().await?)
}

/// Speaks a sample sentence with `profile` (saved or not) and returns a WAV file. `params` override
/// the profile's; a null voice means the profile's voice, else the engine's first.
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

/// Validates and activates a profile; regenerates upcoming audio with it.
#[tauri::command]
pub async fn save_engine_profile(
    core: CoreState<'_>,
    profile: aoide_tts::EngineProfile,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.save_engine_profile(profile).await?)
}

/// Queue whole-book generation (progress arrives via BookSummary.prepare + library://changed).
#[tauri::command]
pub async fn prepare_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.prepare_book(&book_id).await?)
}

/// Queues one chapter (from 0); it joins the book's entry, or is already covered by a whole book.
#[tauri::command]
pub async fn prepare_chapter(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.prepare_chapter(&book_id, chapter).await?)
}

/// How much of each chapter's audio is made, in chapter order.
#[tauri::command]
pub async fn chapter_audio(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<crate::core::ChapterAudioView>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.chapter_audio(&book_id).await?)
}

/// Pause or resume a queued book's preparation (remembered across restarts).
#[tauri::command]
pub fn set_prepare_paused(core: CoreState<'_>, book_id: String, paused: bool) {
    core.set_prepare_paused(&book_id, paused);
}

#[tauri::command]
pub fn cancel_prepare(core: CoreState<'_>, book_id: String) {
    core.cancel_prepare(&book_id);
}

/// Delete the book's generated audio so it is regenerated.
#[tauri::command]
pub async fn regenerate_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.regenerate_book(&book_id).await?)
}
