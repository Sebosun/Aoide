mod book;
mod core;
mod media;
mod rename;

use std::sync::Arc;

use tauri::{Manager, State};

use crate::book::ChapterView;
use crate::core::{
    AppInfo, AppSettings, BookSettings, BookSummary, BookView, Core, EngineOptions,
    EngineStatusView, ImportResult, ListeningStatsView, Paths, PreferencesView,
};

type CoreState<'a> = State<'a, Arc<Core>>;

#[tauri::command]
fn app_info(core: CoreState<'_>) -> AppInfo {
    core.app_info()
}

#[tauri::command]
async fn list_books(core: CoreState<'_>) -> Result<Vec<BookSummary>, String> {
    let core = Arc::clone(&core);
    blocking(move || core.list_books()).await
}

#[tauri::command]
async fn import_files(
    core: CoreState<'_>,
    paths: Vec<String>,
) -> Result<Vec<ImportResult>, String> {
    let core = Arc::clone(&core);
    blocking(move || Ok(core.import_files(paths))).await
}

#[tauri::command]
async fn rescan_library(core: CoreState<'_>) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || core.rescan()).await
}

#[tauri::command]
async fn remove_book(core: CoreState<'_>, book_id: String) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || core.remove_book(&book_id)).await
}

#[tauri::command]
async fn show_in_folder(core: CoreState<'_>, book_id: String) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || core.show_in_folder(&book_id)).await
}

#[tauri::command]
async fn open_book(core: CoreState<'_>, book_id: String) -> Result<BookView, String> {
    let core = Arc::clone(&core);
    blocking(move || core.open_book(&book_id)).await
}

#[tauri::command]
async fn book_overview(core: CoreState<'_>, book_id: String) -> Result<BookView, String> {
    let core = Arc::clone(&core);
    blocking(move || core.book_overview(&book_id)).await
}

#[tauri::command]
async fn chapter(
    core: CoreState<'_>,
    book_id: String,
    chapter: usize,
) -> Result<ChapterView, String> {
    let core = Arc::clone(&core);
    blocking(move || core.chapter(&book_id, chapter)).await
}

#[tauri::command]
async fn play(
    core: CoreState<'_>,
    book_id: String,
    chapter: Option<usize>,
    segment_id: Option<String>,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.play(&book_id, chapter, segment_id).await
}

#[tauri::command]
fn pause(core: CoreState<'_>) {
    core.pause();
}

#[tauri::command]
fn resume(core: CoreState<'_>) -> Result<(), String> {
    core.resume()
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
) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.save_book_settings(&book_id, settings).await
}

#[tauri::command]
fn engine_options(core: CoreState<'_>) -> EngineOptions {
    core.engine_options()
}

#[tauri::command]
async fn engine_voices(core: CoreState<'_>) -> Result<Vec<aoide_tts::Voice>, String> {
    let core = Arc::clone(&core);
    core.engine_voices().await
}

#[tauri::command]
fn collections(core: CoreState<'_>) -> Result<Vec<aoide_library::Collection>, String> {
    core.collections()
}

#[tauri::command]
fn create_collection(core: CoreState<'_>, name: String) -> Result<i64, String> {
    core.create_collection(&name)
}

#[tauri::command]
fn rename_collection(core: CoreState<'_>, id: i64, name: String) -> Result<(), String> {
    core.rename_collection(id, &name)
}

#[tauri::command]
fn delete_collection(core: CoreState<'_>, id: i64) -> Result<(), String> {
    core.delete_collection(id)
}

#[tauri::command]
fn set_book_collections(core: CoreState<'_>, book_id: String, ids: Vec<i64>) -> Result<(), String> {
    core.set_book_collections(&book_id, &ids)
}

#[tauri::command]
fn storage_report(core: CoreState<'_>) -> Result<crate::core::StorageReport, String> {
    core.storage_report()
}

#[tauri::command]
fn clean_unused_files(core: CoreState<'_>) -> Result<crate::core::CleanResult, String> {
    core.clean_unused_files()
}

/// The WAV goes back as raw bytes (an ArrayBuffer in the UI), not as a JSON number array.
#[tauri::command]
async fn preview_voice(
    core: CoreState<'_>,
    profile: aoide_tts::EngineProfile,
    voice_id: Option<String>,
    params: aoide_tts::Params,
    language: Option<String>,
) -> Result<tauri::ipc::Response, String> {
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
) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.save_engine_profile(profile).await
}

#[tauri::command]
async fn prepare_book(core: CoreState<'_>, book_id: String) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.prepare_book(&book_id).await
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
async fn regenerate_book(core: CoreState<'_>, book_id: String) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.regenerate_book(&book_id).await
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
) -> Result<Vec<crate::book::SearchHit>, String> {
    let core = Arc::clone(&core);
    blocking(move || core.search_book(&book_id, &query)).await
}

#[tauri::command]
async fn skip_chapter(core: CoreState<'_>, delta: i64) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.skip_chapter(delta).await
}

#[tauri::command]
async fn bookmarks(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::Bookmark>, String> {
    let core = Arc::clone(&core);
    blocking(move || core.bookmarks(&book_id)).await
}

#[tauri::command]
async fn add_bookmark(
    core: CoreState<'_>,
    book_id: String,
    note: String,
) -> Result<aoide_library::Bookmark, String> {
    let core = Arc::clone(&core);
    blocking(move || core.add_bookmark(&book_id, &note)).await
}

#[tauri::command]
fn update_bookmark_note(core: CoreState<'_>, id: i64, note: String) -> Result<(), String> {
    core.update_bookmark_note(id, &note)
}

#[tauri::command]
fn delete_bookmark(core: CoreState<'_>, id: i64) -> Result<(), String> {
    core.delete_bookmark(id)
}

#[tauri::command]
async fn book_listening(
    core: CoreState<'_>,
    book_id: String,
) -> Result<Vec<aoide_library::DayListening>, String> {
    let core = Arc::clone(&core);
    blocking(move || core.book_listening(&book_id)).await
}

#[tauri::command]
async fn listening_stats(core: CoreState<'_>) -> Result<ListeningStatsView, String> {
    let core = Arc::clone(&core);
    blocking(move || core.listening_stats()).await
}

#[tauri::command]
async fn book_details(
    core: CoreState<'_>,
    book_id: String,
) -> Result<aoide_library::BookDetails, String> {
    let core = Arc::clone(&core);
    blocking(move || core.book_details(&book_id)).await
}

#[tauri::command]
fn app_settings(core: CoreState<'_>) -> AppSettings {
    core.app_settings()
}

#[tauri::command]
async fn save_app_settings(core: CoreState<'_>, settings: AppSettings) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || core.save_app_settings(settings)).await
}

#[tauri::command]
fn preferences(core: CoreState<'_>) -> PreferencesView {
    core.preferences()
}

#[tauri::command]
async fn save_appearance(
    core: CoreState<'_>,
    appearance: aoide_config::Appearance,
) -> Result<(), String> {
    let core = Arc::clone(&core);
    blocking(move || core.save_appearance(appearance)).await
}

#[tauri::command]
async fn lookup_metadata(
    core: CoreState<'_>,
    book_id: String,
    refresh: bool,
) -> Result<Vec<aoide_metadata::Match>, String> {
    let core = Arc::clone(&core);
    core.lookup_metadata(&book_id, refresh).await
}

#[tauri::command]
async fn apply_metadata(
    core: CoreState<'_>,
    book_id: String,
    candidate: aoide_metadata::Match,
) -> Result<aoide_library::BookDetails, String> {
    let core = Arc::clone(&core);
    core.apply_metadata(&book_id, candidate).await
}

#[tauri::command]
async fn export_book(core: CoreState<'_>, book_id: String) -> Result<String, String> {
    let core = Arc::clone(&core);
    core.export_book(&book_id).await
}

#[tauri::command]
async fn scan_for_books(core: CoreState<'_>) -> Result<Vec<crate::core::Candidate>, String> {
    let core = Arc::clone(&core);
    core.scan_for_books().await
}

#[tauri::command]
fn cancel_scan(core: CoreState<'_>) {
    core.cancel_scan();
}

#[tauri::command]
async fn discovered_books(core: CoreState<'_>) -> Result<Vec<crate::core::Candidate>, String> {
    let core = Arc::clone(&core);
    blocking(move || core.discovered_books()).await
}

#[tauri::command]
async fn fetch_discovered_details(core: CoreState<'_>, ids: Vec<String>) -> Result<(), String> {
    let core = Arc::clone(&core);
    core.fetch_discovered_details(ids).await
}

#[tauri::command]
async fn import_discovered(
    core: CoreState<'_>,
    choices: Vec<crate::core::ImportChoice>,
) -> Result<Vec<ImportResult>, String> {
    let core = Arc::clone(&core);
    core.import_discovered(choices).await
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

/// Debug-only headless check: plays the first present book whose title contains
/// `AOIDE_SMOKE_BOOK`, logs player/generation state to stderr, then exits.
#[cfg(debug_assertions)]
fn smoke(app: tauri::AppHandle, core: Arc<Core>, seconds: u64) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let wanted = std::env::var("AOIDE_SMOKE_BOOK")
            .unwrap_or_default()
            .to_lowercase();
        if let Ok(url) = std::env::var("AOIDE_SMOKE_ALIGNER") {
            let mut settings = core.app_settings();
            settings.aligner = Some(crate::core::AlignerSettings {
                base_url: url,
                network: Default::default(),
                launch: None,
            });
            eprintln!(
                "smoke: aligner settings: {:?}",
                core.save_app_settings(settings)
            );
        }
        if std::env::var("AOIDE_SMOKE_DISCOVER").is_ok() {
            for round in 1..=2 {
                let started = std::time::Instant::now();
                match core.scan_for_books().await {
                    Ok(found) => {
                        eprintln!("smoke: scan {round} took {:?}", started.elapsed());
                        for c in &found {
                            eprintln!(
                                "smoke:   {:10?} {:?} by {:?} x{} cover={} err={:?}",
                                c.status,
                                c.title,
                                c.authors,
                                c.paths.len(),
                                c.cover_path.is_some(),
                                c.error
                            );
                        }
                    }
                    Err(error) => eprintln!("smoke: scan failed: {error}"),
                }
            }
            let new: Vec<_> = core
                .discovered_books()
                .unwrap_or_default()
                .into_iter()
                .filter(|c| c.status == crate::core::CandidateStatus::New)
                .collect();
            if std::env::var("AOIDE_SMOKE_DETAILS").is_ok() {
                let ids = new.iter().map(|c| c.id.clone()).collect();
                eprintln!(
                    "smoke: details: {:?}",
                    core.fetch_discovered_details(ids).await
                );
                for c in core
                    .discovered_books()
                    .unwrap_or_default()
                    .iter()
                    .filter(|c| c.status == crate::core::CandidateStatus::New)
                {
                    eprintln!(
                        "smoke:   suggestion for {:?}: {:?}",
                        c.title,
                        c.suggestion.as_ref().map(|m| (&m.title, &m.key))
                    );
                }
            }
            let choices = new
                .iter()
                .map(|c| crate::core::ImportChoice {
                    id: c.id.clone(),
                    apply_suggestion: false,
                })
                .collect();
            eprintln!("smoke: import: {:?}", core.import_discovered(choices).await);
            let titles: Vec<String> = core
                .list_books()
                .unwrap_or_default()
                .into_iter()
                .map(|b| b.title)
                .collect();
            eprintln!("smoke: library now: {titles:?}");
            for c in core.discovered_books().unwrap_or_default() {
                eprintln!("smoke:   after import {:10?} {:?}", c.status, c.title);
            }
            app.exit(0);
            return;
        }
        let books = core.list_books().unwrap_or_default();
        eprintln!("smoke: {} books", books.len());
        let Some(book) = books.iter().find(|b| {
            matches!(b.status, aoide_library::BookStatus::Present)
                && b.title.to_lowercase().contains(&wanted)
        }) else {
            eprintln!("smoke: no matching book");
            app.exit(1);
            return;
        };
        let view = core.open_book(&book.id);
        eprintln!(
            "smoke: opened {:?}: {:?}",
            book.title,
            view.as_ref().map(|v| v.chapters.len())
        );
        if std::env::var("AOIDE_SMOKE_PREPARE").is_ok() {
            if let Err(error) = core.prepare_book(&book.id).await {
                eprintln!("smoke: prepare failed: {error}");
            }
            for t in 0..seconds {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let g = core.generation_state();
                eprintln!(
                    "smoke: t={t:>2}s preparing={:?} gen={:?} err={:?}",
                    g.preparing, g.generating_segment_id, g.error
                );
            }
            core.cancel_prepare(&book.id);
            app.exit(0);
            return;
        }
        if std::env::var("AOIDE_SMOKE_EXPORT").is_ok() {
            let started = std::time::Instant::now();
            let result = core.export_book(&book.id).await;
            eprintln!("smoke: export after {:?}: {result:?}", started.elapsed());
            app.exit(if result.is_ok() { 0 } else { 1 });
            return;
        }
        let chapter = std::env::var("AOIDE_SMOKE_CHAPTER")
            .ok()
            .and_then(|c| c.parse().ok());
        if let Err(error) = core.play(&book.id, chapter, None).await {
            eprintln!("smoke: play failed: {error}");
        }
        let started = std::time::Instant::now();
        while started.elapsed().as_secs() < seconds {
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            let p = core.player_state();
            let g = core.generation_state();
            eprintln!(
                "smoke: t={:>2}s {:?} seg={:?} word={:?} frac={:.3} | gen={:?} buffered={}ms pending={} err={:?}",
                started.elapsed().as_secs(), p.status, p.segment_id, p.word_index, p.chapter_fraction,
                g.generating_segment_id, g.buffered_ahead_ms, g.pending, g.error
            );
        }
        match core.add_bookmark(&book.id, "smoke note") {
            Ok(mark) => eprintln!(
                "smoke: bookmark {:?} at {:?}: {:?}",
                mark.id, mark.segment_id, mark.excerpt
            ),
            Err(error) => eprintln!("smoke: bookmark failed: {error}"),
        }
        core.stop();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        match core.listening_stats() {
            Ok(stats) => eprintln!(
                "smoke: listened total {} ms, days {:?}",
                stats.total_ms, stats.by_day
            ),
            Err(error) => eprintln!("smoke: stats failed: {error}"),
        }
        app.exit(0);
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
                config_dir: aoide_config::config_dir(
                    std::env::var_os("XDG_CONFIG_HOME").as_deref(),
                    &path.home_dir()?,
                ),
            };
            let smoke_home =
                cfg!(debug_assertions) && std::env::var_os("AOIDE_SMOKE_HOME").is_some();
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
                    eprintln!("aoide: {line}");
                }
            }
            // Debug builds can run against a throwaway folder instead of the real library.
            #[cfg(debug_assertions)]
            if let Ok(home) = std::env::var("AOIDE_SMOKE_HOME") {
                let home = std::path::PathBuf::from(home);
                paths = Paths {
                    home_dir: home.join("Home"),
                    library_dir: home.join("Books"),
                    covers_dir: home.join("data").join("covers"),
                    audio_dir: home.join("cache").join("audio"),
                    db_path: home.join("data").join("library.sqlite3"),
                    config_dir: home.join("config").join("aoide"),
                };
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
                smoke(
                    app.handle().clone(),
                    Arc::clone(&core),
                    seconds.parse().unwrap_or(20),
                );
            }
            app.manage(core);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            list_books,
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
            set_prepare_paused,
            regenerate_book,
            set_sleep_timer,
            search_book,
            skip_chapter,
            bookmarks,
            add_bookmark,
            update_bookmark_note,
            delete_bookmark,
            listening_stats,
            book_listening,
            book_details,
            app_settings,
            save_app_settings,
            preferences,
            save_appearance,
            export_book,
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
            }
        });
}
