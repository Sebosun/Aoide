mod book;
mod core;
mod logging;
mod media;
mod rename;
#[cfg(debug_assertions)]
mod smoke;

use std::sync::Arc;

use tauri::{Manager, State};

use crate::book::ChapterView;
use crate::core::{
    AppInfo, AppSettings, AppSettingsPatch, BookProgressView, BookSettings, BookSummary, BookView,
    Core, CoreError, EngineOptions, EngineStatusView, HomeView, ImportResult, Paths,
    PreferencesView, ReadingMode,
};

type CoreState<'a> = State<'a, Arc<Core>>;

/// Writes a line from the window to Aoide's log.
#[tauri::command]
fn log_ui(entry: logging::UiLog) {
    logging::ui(entry);
}

#[tauri::command]
fn app_info(core: CoreState<'_>) -> AppInfo {
    core.app_info()
}

#[tauri::command]
async fn list_books(core: CoreState<'_>) -> Result<Vec<BookSummary>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.list_books()).await
}

#[tauri::command]
async fn home(core: CoreState<'_>) -> Result<HomeView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.home()).await
}

#[tauri::command]
async fn import_files(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<Vec<ImportResult>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.import_files(paths))).await
}

#[tauri::command]
async fn rescan_library(core: CoreState<'_>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.rescan()).await
}

#[tauri::command]
async fn remove_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.remove_book(&book_id)).await
}

#[tauri::command]
async fn show_in_folder(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.show_in_folder(&book_id)).await
}

#[tauri::command]
async fn open_book(core: CoreState<'_>, book_id: String) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.open_book(&book_id)).await
}

#[tauri::command]
async fn book_overview(core: CoreState<'_>, book_id: String) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.book_overview(&book_id)).await
}

#[tauri::command]
async fn mark_finished(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.mark_finished(&book_id)).await
}

/// Forgets that the book was finished and where the user was in it.
#[tauri::command]
async fn mark_unread(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.mark_unread(&book_id)).await
}

/// Where the user is reading silently, and the progress it makes; ignored while that book plays.
#[tauri::command]
async fn set_reading_place(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
    segment_id: String,
) -> Result<Option<BookProgressView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.set_reading_place(&book_id, chapter, &segment_id)).await
}

/// The user is reading the book silently now (sent every 15 s at most).
#[tauri::command]
async fn reading_activity(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || {
        core.reading_activity(&book_id);
        Ok::<(), String>(())
    })
    .await
}

/// Reads book `book_id` silently or listens to it; reading the playing book pauses it.
#[tauri::command]
async fn set_mode(
    core: CoreState<'_>,
    book_id: String,
    mode: ReadingMode,
) -> Result<BookView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.set_mode(&book_id, mode)).await
}

#[tauri::command]
async fn chapter(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
) -> Result<ChapterView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.chapter(&book_id, chapter)).await
}

#[tauri::command]
async fn play(
    core: CoreState<'_>,
    book_id: String,
    chapter: Option<usize>,
    segment_id: Option<String>,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.play(&book_id, chapter, segment_id).await?)
}

#[tauri::command]
fn pause(core: CoreState<'_>) {
    core.pause();
}

#[tauri::command]
fn resume(core: CoreState<'_>) -> Result<(), CoreError> {
    Ok(core.resume()?)
}

#[tauri::command]
fn stop(core: CoreState<'_>) {
    core.stop();
}

#[tauri::command]
fn set_speed(core: CoreState<'_>, speed: f32) {
    core.set_speed(speed);
}

#[tauri::command]
fn set_volume(core: CoreState<'_>, volume: f32) {
    core.set_volume(volume);
}

#[tauri::command]
fn engine_status(core: CoreState<'_>) -> EngineStatusView {
    core.engine_status()
}

#[tauri::command]
fn book_settings(core: CoreState<'_>, book_id: String) -> BookSettings {
    core.book_settings(&book_id)
}

#[tauri::command]
async fn save_book_settings(
    core: CoreState<'_>,
    book_id: String,
    settings: BookSettings,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.save_book_settings(&book_id, settings).await?)
}

#[tauri::command]
fn engine_options(core: CoreState<'_>) -> EngineOptions {
    core.engine_options()
}

#[tauri::command]
async fn engine_voices(core: CoreState<'_>) -> Result<Vec<aoide_tts::Voice>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.engine_voices().await?)
}

#[tauri::command]
fn collections(core: CoreState<'_>) -> Result<Vec<aoide_library::Collection>, CoreError> {
    Ok(core.collections()?)
}

#[tauri::command]
fn create_collection(core: CoreState<'_>, name: String) -> Result<i64, CoreError> {
    Ok(core.create_collection(&name)?)
}

#[tauri::command]
fn rename_collection(core: CoreState<'_>, id: i64, name: String) -> Result<(), CoreError> {
    Ok(core.rename_collection(id, &name)?)
}

#[tauri::command]
fn delete_collection(core: CoreState<'_>, id: i64) -> Result<(), CoreError> {
    Ok(core.delete_collection(id)?)
}

#[tauri::command]
fn set_book_collections(
    core: CoreState<'_>,
    book_id: String,
    ids: Vec<i64>,
) -> Result<(), CoreError> {
    Ok(core.set_book_collections(&book_id, &ids)?)
}

#[tauri::command]
fn storage_report(core: CoreState<'_>) -> Result<crate::core::StorageReport, CoreError> {
    Ok(core.storage_report()?)
}

#[tauri::command]
fn clean_unused_files(core: CoreState<'_>) -> Result<crate::core::CleanResult, CoreError> {
    Ok(core.clean_unused_files()?)
}

/// The WAV goes back as raw bytes (an ArrayBuffer in the UI), not as a JSON number array.
#[tauri::command]
async fn preview_voice(
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
async fn save_engine_profile(
    core: CoreState<'_>,
    profile: aoide_tts::EngineProfile,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.save_engine_profile(profile).await?)
}

#[tauri::command]
async fn prepare_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.prepare_book(&book_id).await?)
}

#[tauri::command]
async fn prepare_chapter(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.prepare_chapter(&book_id, chapter).await?)
}

#[tauri::command]
async fn chapter_audio(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<crate::core::ChapterAudioView>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.chapter_audio(&book_id).await?)
}

#[tauri::command]
fn set_prepare_paused(core: CoreState<'_>, book_id: String, paused: bool) {
    core.set_prepare_paused(&book_id, paused);
}

#[tauri::command]
fn cancel_prepare(core: CoreState<'_>, book_id: String) {
    core.cancel_prepare(&book_id);
}

#[tauri::command]
async fn regenerate_book(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.regenerate_book(&book_id).await?)
}

#[tauri::command]
fn set_sleep_timer(core: CoreState<'_>, minutes: Option<u32>, end_of_chapter: bool) {
    core.set_sleep_timer(minutes, end_of_chapter);
}

#[tauri::command]
async fn search_book(
    core: CoreState<'_>,
    book_id: String,
    query: String,
) -> Result<Vec<crate::book::SearchHit>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.search_book(&book_id, &query)).await
}

#[tauri::command]
async fn skip_chapter(core: CoreState<'_>, delta: i64) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.skip_chapter(delta).await?)
}

#[tauri::command]
async fn bookmarks(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::Bookmark>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.bookmarks(&book_id)).await
}

#[tauri::command]
async fn add_bookmark(
    core: CoreState<'_>,
    book_id: String,
    note: String,
) -> Result<aoide_library::Bookmark, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.add_bookmark(&book_id, &note)).await
}

#[tauri::command]
fn update_bookmark_note(core: CoreState<'_>, id: i64, note: String) -> Result<(), CoreError> {
    Ok(core.update_bookmark_note(id, &note)?)
}

#[tauri::command]
fn delete_bookmark(core: CoreState<'_>, id: i64) -> Result<(), CoreError> {
    Ok(core.delete_bookmark(id)?)
}

#[tauri::command]
async fn book_listening(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::DayListening>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.book_listening(&book_id)).await
}

/// Reading and listening time, per day and per book.
#[tauri::command]
async fn activity_stats(core: CoreState<'_>) -> Result<aoide_library::ActivityStats, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.activity_stats()).await
}

#[tauri::command]
async fn book_details(
    core: CoreState<'_>,
    book_id: String,
) -> Result<aoide_library::BookDetails, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.book_details(&book_id)).await
}

#[tauri::command]
fn app_settings(core: CoreState<'_>) -> AppSettings {
    core.app_settings()
}

/// Merges a change into the app settings; returns the settings now saved.
#[tauri::command]
async fn update_app_settings(
    core: CoreState<'_>,
    patch: AppSettingsPatch,
) -> Result<AppSettings, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.update_app_settings(patch)).await
}

#[tauri::command]
fn preferences(core: CoreState<'_>) -> PreferencesView {
    core.preferences()
}

#[tauri::command]
async fn save_appearance(
    core: CoreState<'_>,
    appearance: aoide_config::Appearance,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.save_appearance(appearance)).await
}

#[tauri::command]
async fn lookup_metadata(
    core: CoreState<'_>,
    book_id: String,
    refresh: bool,
) -> Result<Vec<aoide_metadata::Match>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.lookup_metadata(&book_id, refresh).await?)
}

#[tauri::command]
async fn apply_metadata(
    core: CoreState<'_>,
    book_id: String,
    candidate: aoide_metadata::Match,
) -> Result<aoide_library::BookDetails, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.apply_metadata(&book_id, candidate).await?)
}

#[tauri::command]
async fn export_book(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    let started = std::time::Instant::now();
    let result = core.export_book(&book_id).await;
    log_export("m4b", &book_id, started, &result);
    Ok(result?)
}

/// Logs how an export ended.
fn log_export(
    kind: &str,
    book_id: &str,
    started: std::time::Instant,
    result: &Result<String, String>,
) {
    let book = logging::book(book_id);
    let took_ms = logging::took(started);
    match result {
        Ok(path) => {
            tracing::info!(event = "export.done", kind, book, took_ms, file = %logging::file(std::path::Path::new(path)), outcome = "ok", "Exported a book")
        }
        Err(error) => {
            tracing::warn!(event = "export.done", kind, book, took_ms, outcome = "failed", %error, "Export failed")
        }
    }
}

#[tauri::command]
async fn export_read_along(core: CoreState<'_>, book_id: String) -> Result<String, CoreError> {
    let core = Arc::clone(&core);
    let started = std::time::Instant::now();
    let result = core.export_read_along(&book_id).await;
    log_export("read-along", &book_id, started, &result);
    Ok(result?)
}

#[tauri::command]
async fn scan_for_books(core: CoreState<'_>) -> Result<Vec<crate::core::Candidate>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.scan_for_books().await?)
}

#[tauri::command]
fn cancel_scan(core: CoreState<'_>) {
    core.cancel_scan();
}

#[tauri::command]
async fn discovered_books(core: CoreState<'_>) -> Result<Vec<crate::core::Candidate>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.discovered_books()).await
}

#[tauri::command]
async fn scan_for_audiobooks(
    core: CoreState<'_>,
) -> Result<Vec<crate::core::AudiobookMatch>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.scan_for_audiobooks().await?)
}

#[tauri::command]
fn audiobook_matches(core: CoreState<'_>) -> Vec<crate::core::AudiobookMatch> {
    core.audiobook_matches()
}

#[tauri::command]
fn cancel_audiobook_scan(core: CoreState<'_>) {
    core.cancel_audiobook_scan();
}

/// After books were added: how many of them have audiobooks waiting for the user.
#[tauri::command]
async fn look_for_audiobooks(core: CoreState<'_>, ids: Vec<String>) -> Result<usize, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.look_for_audiobooks(ids).await?)
}

#[tauri::command]
async fn pair_audiobooks(
    core: CoreState<'_>,
    choices: Vec<crate::core::AudiobookChoice>,
) -> Result<Vec<crate::core::PairOutcome>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.pair_audiobooks(choices))).await
}

#[tauri::command]
async fn fetch_discovered_details(core: CoreState<'_>, ids: Vec<String>) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.fetch_discovered_details(ids).await?)
}

#[tauri::command]
async fn import_discovered(
    core: CoreState<'_>,
    choices: Vec<crate::core::ImportChoice>,
) -> Result<Vec<ImportResult>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.import_discovered(choices).await?)
}

#[tauri::command]
async fn inspect_recording(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<crate::core::InspectionView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.inspect_recording(paths))).await
}

#[tauri::command]
async fn attach_recording(
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
async fn recording(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Option<crate::core::RecordingView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.recording(&book_id))).await
}

#[tauri::command]
async fn recordings(core: CoreState<'_>) -> Result<Vec<crate::core::RecordingView>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.recordings())).await
}

#[tauri::command]
async fn sync_report(
    core: CoreState<'_>,
    book_id: String,
) -> Result<crate::core::SyncReportView, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.sync_report(&book_id)).await
}

#[tauri::command]
async fn sync_action(
    core: CoreState<'_>,
    book_id: String,
    action: crate::core::SyncAction,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.sync_action(&book_id, action)).await
}

#[tauri::command]
async fn remove_recording(core: CoreState<'_>, book_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.remove_recording(&book_id).await?)
}

#[tauri::command]
async fn set_narration(
    core: CoreState<'_>,
    book_id: String,
    narration: crate::core::Narration,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.set_narration(&book_id, narration).await?)
}

#[tauri::command]
fn recognition_options(core: CoreState<'_>) -> crate::core::RecognitionOptions {
    core.recognition_options()
}

#[tauri::command]
async fn download_model(core: CoreState<'_>, model_id: String) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.download_model(&model_id).await?)
}

#[tauri::command]
async fn play_recording_at(
    core: CoreState<'_>,
    book_id: String,
    at_ms: u64,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.play_recording_at(&book_id, at_ms).await?)
}

/// The system's file chooser for EPUB files to add. `None` when cancelled.
#[tauri::command]
async fn choose_books(app: tauri::AppHandle) -> Option<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Choose EPUB files to add")
        .add_filter("EPUB books", &["epub"])
        .pick_files(move |picked| {
            let _ = tx.send(picked.map(|files| files.iter().map(|p| p.to_string()).collect()));
        });
    rx.await.ok().flatten()
}

/// The system's file chooser for audiobook files, or a folder of them. `None` when cancelled.
#[tauri::command]
async fn choose_audio(app: tauri::AppHandle, folder: bool) -> Option<Vec<String>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file();
    if folder {
        dialog
            .set_title("Choose the audiobook's folder")
            .pick_folder(move |picked| {
                let _ = tx.send(picked.map(|p| vec![p.to_string()]));
            });
    } else {
        dialog
            .set_title("Choose the audiobook's files")
            .add_filter(
                "Audiobooks",
                &[
                    "m4b", "m4a", "mp4", "aac", "mp3", "flac", "ogg", "oga", "opus", "wav",
                ],
            )
            .pick_files(move |picked| {
                let _ = tx.send(picked.map(|files| files.iter().map(|p| p.to_string()).collect()));
            });
    }
    rx.await.ok().flatten()
}

/// Runs `f` on a blocking thread; its error, whichever type, comes back as a `CoreError`.
async fn blocking<T: Send + 'static, E: Into<CoreError> + Send + 'static>(
    f: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, CoreError> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(CoreError::internal)?
        .map_err(Into::into)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = app.path();
            let data = path.app_data_dir()?;
            #[allow(unused_mut)]
            let mut paths = Paths {
                home_dir: path.home_dir()?,
                library_dir: path.document_dir()?.join("Aoide").join("Books"),
                covers_dir: data.join("covers"),
                audio_dir: path.app_cache_dir()?.join("audio"),
                db_path: data.join("library.sqlite3"),
                models_dir: data.join("models"),
                config_dir: aoide_config::config_dir(
                    std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                    &path.home_dir()?,
                ),
            };
            let smoke_home =
                cfg!(debug_assertions) && std::env::var_os("AOIDE_SMOKE_HOME").is_some();
            let logs = match std::env::var_os("AOIDE_SMOKE_HOME") {
                #[cfg(debug_assertions)]
                Some(home) if smoke_home => smoke::paths(std::path::Path::new(&home))
                    .db_path
                    .with_file_name("logs"),
                _ => data.join("logs"),
            };
            logging::init(&logs);
            tracing::info!(
                event = "app.start",
                version = env!("CARGO_PKG_VERSION"),
                profile = if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
                gpu = aoide_transcribe::models::GPU_BUILD,
                os = std::env::consts::OS,
                smoke = smoke_home,
                "Aoide started"
            );
            if !smoke_home {
                // First start after the rename from Folio: bring its folders along.
                let cache = path.app_cache_dir()?;
                let documents = paths.library_dir.parent().map(std::path::Path::to_path_buf);
                let mut moves = vec![
                    (data.with_file_name(rename::OLD_IDENTIFIER), data.clone()),
                    (cache.with_file_name(rename::OLD_IDENTIFIER), cache),
                    (
                        paths.config_dir.with_file_name(rename::OLD_CONFIG),
                        paths.config_dir.clone(),
                    ),
                ];
                if let Some(documents) = documents {
                    moves.push((documents.with_file_name(rename::OLD_DOCUMENTS), documents));
                }
                for line in rename::move_folders(&moves, &paths.db_path) {
                    tracing::info!(event = "app.migrate", "{line}");
                }
            }
            // Debug builds can run against a throwaway folder instead of the real library.
            #[cfg(debug_assertions)]
            if let Some(home) = std::env::var_os("AOIDE_SMOKE_HOME") {
                paths = smoke::paths(std::path::Path::new(&home));
            }
            #[allow(unused_mut)]
            let mut environment = crate::core::Environment::desktop(app.handle().clone());
            // Debug smoke runs (and AOIDE_MUTE) play into a silent, real-time-paced sink.
            #[cfg(debug_assertions)]
            if std::env::var_os("AOIDE_SMOKE_SECONDS").is_some()
                || std::env::var_os("AOIDE_MUTE").is_some()
            {
                environment.audio = crate::core::Environment::silent_audio();
            }
            let core =
                tauri::async_runtime::block_on(async move { Core::start(environment, paths) })
                    .map_err(Box::<dyn std::error::Error>::from)?;
            #[cfg(debug_assertions)]
            if let Ok(seconds) = std::env::var("AOIDE_SMOKE_SECONDS") {
                smoke::spawn(
                    app.handle().clone(),
                    Arc::clone(&core),
                    seconds.parse().unwrap_or(20),
                );
            }
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            log_ui,
            app_info,
            list_books,
            home,
            set_mode,
            mark_finished,
            mark_unread,
            set_reading_place,
            reading_activity,
            activity_stats,
            import_files,
            rescan_library,
            remove_book,
            show_in_folder,
            open_book,
            book_overview,
            chapter,
            play,
            pause,
            resume,
            stop,
            set_speed,
            set_volume,
            engine_status,
            book_settings,
            save_book_settings,
            engine_options,
            engine_voices,
            preview_voice,
            storage_report,
            clean_unused_files,
            collections,
            create_collection,
            rename_collection,
            delete_collection,
            set_book_collections,
            save_engine_profile,
            prepare_book,
            cancel_prepare,
            prepare_chapter,
            chapter_audio,
            set_prepare_paused,
            regenerate_book,
            set_sleep_timer,
            search_book,
            skip_chapter,
            bookmarks,
            add_bookmark,
            update_bookmark_note,
            delete_bookmark,
            book_listening,
            book_details,
            app_settings,
            update_app_settings,
            preferences,
            save_appearance,
            export_book,
            export_read_along,
            inspect_recording,
            attach_recording,
            recording,
            recordings,
            sync_report,
            sync_action,
            remove_recording,
            set_narration,
            recognition_options,
            download_model,
            choose_audio,
            choose_books,
            scan_for_audiobooks,
            audiobook_matches,
            cancel_audiobook_scan,
            look_for_audiobooks,
            pair_audiobooks,
            play_recording_at,
            lookup_metadata,
            scan_for_books,
            cancel_scan,
            discovered_books,
            fetch_discovered_details,
            import_discovered,
            apply_metadata
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Aoide")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(core) = app.try_state::<Arc<Core>>() {
                    core.shutdown();
                }
                tracing::info!(event = "app.exit", "Aoide stopped");
                logging::flush();
            }
        });
}
