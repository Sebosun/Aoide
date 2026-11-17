mod book;
mod commands;
mod core;
mod launch;
mod logging;
mod media;
mod notify;
mod rename;
#[cfg(debug_assertions)]
mod smoke;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::core::{Core, Paths};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();
    // One running copy, so the library has one writer: a second start (a book opened from the
    // file manager, `aoide book.epub`) hands its arguments to the first and exits. A smoke run
    // uses a library of its own and is never handed to a running Aoide.
    if smoke_home().is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(second_start));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let core = start(app)?;
            app.manage(core);
            exit_on_signals(app.handle().clone());
            let argv: Vec<String> = std::env::args_os()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            let cwd = std::env::current_dir().unwrap_or_default();
            hand_over(
                app.handle(),
                launch::requests(launch::arguments(&argv), &cwd),
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::log_ui,
            commands::app::app_info,
            commands::app::take_open_requests,
            commands::library::list_books,
            commands::library::home,
            commands::library::add_audiobook,
            commands::library::import_files,
            commands::library::rescan_library,
            commands::library::remove_book,
            commands::library::show_in_folder,
            commands::library::open_original,
            commands::library::open_book,
            commands::library::book_overview,
            commands::reading::mark_finished,
            commands::reading::mark_unread,
            commands::reading::set_reading_place,
            commands::reading::reading_activity,
            commands::reading::set_mode,
            commands::library::chapter,
            commands::playback::play,
            commands::playback::pause,
            commands::playback::resume,
            commands::playback::stop,
            commands::playback::set_speed,
            commands::playback::set_volume,
            commands::speech::engine_status,
            commands::speech::speech_problems,
            commands::speech::clear_speech_problems,
            commands::speech::retry_speech_server,
            commands::speech::stop_speech_server,
            commands::speech::show_speech_server_log,
            commands::speech::book_settings,
            commands::speech::save_book_settings,
            commands::speech::set_left_out,
            commands::speech::engine_options,
            commands::speech::engine_voices,
            commands::library::collections,
            commands::library::create_collection,
            commands::library::rename_collection,
            commands::library::delete_collection,
            commands::library::set_book_collections,
            commands::library::tags,
            commands::library::set_book_tags,
            commands::library::tag_with_model,
            commands::library::tag_untagged_with_model,
            commands::library::storage_report,
            commands::library::clean_unused_files,
            commands::speech::preview_voice,
            commands::speech::preview_pauses,
            commands::speech::save_engine_profile,
            commands::speech::prepare_book,
            commands::speech::prepare_chapter,
            commands::speech::chapter_audio,
            commands::speech::set_prepare_paused,
            commands::speech::cancel_prepare,
            commands::speech::regenerate_book,
            commands::speech::say_again,
            commands::speech::pronunciations,
            commands::speech::pronunciation_affected,
            commands::speech::save_pronunciation,
            commands::speech::delete_pronunciation,
            commands::speech::preview_pronunciation,
            commands::playback::set_sleep_timer,
            commands::library::search_book,
            commands::playback::skip_chapter,
            commands::playback::seek,
            commands::playback::skip,
            commands::reading::bookmarks,
            commands::reading::add_bookmark,
            commands::reading::update_bookmark_note,
            commands::reading::delete_bookmark,
            commands::reading::highlights,
            commands::reading::add_highlight,
            commands::reading::highlight_spoken,
            commands::reading::update_highlight_note,
            commands::reading::delete_highlight,
            commands::reading::notes_markdown,
            commands::reading::choose_notes_target,
            commands::reading::export_notes,
            commands::reading::choose_notes_folder,
            commands::reading::export_all_notes,
            commands::reading::book_listening,
            commands::reading::activity_stats,
            commands::library::book_details,
            commands::app::app_settings,
            commands::app::update_app_settings,
            commands::app::preferences,
            commands::app::save_appearance,
            commands::library::lookup_metadata,
            commands::library::apply_metadata,
            commands::library::export_book,
            commands::library::export_read_along,
            commands::discovery::scan_for_books,
            commands::discovery::cancel_scan,
            commands::discovery::discovered_books,
            commands::discovery::scan_for_audiobooks,
            commands::discovery::audiobook_matches,
            commands::discovery::merge_audiobook,
            commands::articles::save_article,
            commands::browser::browser_status,
            commands::browser::download_browser,
            commands::browser::cancel_browser_download,
            commands::browser::remove_browser,
            commands::browser::sites,
            commands::browser::set_site_rule,
            commands::browser::sign_out_site,
            commands::browser::forget_site,
            commands::browser::clear_browser_data,
            commands::browser::start_sign_in,
            commands::browser::sign_in,
            commands::browser::finish_sign_in,
            commands::browser::cancel_sign_in,
            commands::browser::show_sign_in_window,
            commands::browser::stay_signed_in,
            commands::articles::refetch_article,
            commands::articles::article,
            commands::feeds::feeds,
            commands::feeds::add_link,
            commands::feeds::subscribe,
            commands::feeds::unsubscribe,
            commands::feeds::feed_entries,
            commands::feeds::open_feed_entry,
            commands::feeds::save_feed_entry,
            commands::feeds::mark_entry_read,
            commands::feeds::mark_all_read,
            commands::feeds::save_to_library,
            commands::feeds::open_feed_site,
            commands::feeds::check_feeds,
            commands::discovery::unmatched_audiobooks,
            commands::discovery::cancel_audiobook_scan,
            commands::discovery::look_for_audiobooks,
            commands::discovery::pair_audiobooks,
            commands::discovery::fetch_discovered_details,
            commands::discovery::import_discovered,
            commands::recordings::inspect_recording,
            commands::recordings::attach_recording,
            commands::recordings::recording,
            commands::recordings::recordings,
            commands::recordings::sync_report,
            commands::recordings::sync_action,
            commands::recordings::remove_recording,
            commands::recordings::set_narration,
            commands::recordings::recognition_options,
            commands::recordings::download_model,
            commands::recordings::play_recording_at,
            commands::discovery::choose_books,
            commands::feeds::choose_opml,
            commands::browser::choose_browser,
            commands::feeds::choose_opml_target,
            commands::feeds::import_opml,
            commands::feeds::export_opml,
            commands::discovery::choose_audio,
            commands::discovery::choose_folder,
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

/// Another start of Aoide handed its arguments over: the window comes to the front and does
/// what they ask.
fn second_start(app: &AppHandle, argv: Vec<String>, cwd: String) {
    tracing::info!(
        event = "launch.second_instance",
        arguments = launch::arguments(&argv).len(),
        "Another start of Aoide handed its arguments over"
    );
    show_window(app);
    hand_over(
        app,
        launch::requests(launch::arguments(&argv), Path::new(&cwd)),
    );
}

/// Has the core act on `requests`, off the main thread: a new book is added first.
fn hand_over(app: &AppHandle, requests: Vec<launch::Request>) {
    if requests
        .iter()
        .all(|request| *request == launch::Request::Show)
    {
        return;
    }
    let Some(core) = app.try_state::<Arc<Core>>() else {
        tracing::warn!(
            event = "launch.second_instance",
            outcome = "early",
            "Asked to open something before Aoide had started"
        );
        return;
    };
    let core = Arc::clone(&core);
    tauri::async_runtime::spawn_blocking(move || core.launch(requests));
}

/// Shows, unminimises and focuses the window.
pub(crate) fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Quits the app properly on Ctrl+C or SIGTERM, so `Core::shutdown` stops the servers Aoide
/// started: they run in process groups of their own and do not get the terminal's signals.
fn exit_on_signals(app: tauri::AppHandle) {
    #[cfg(unix)]
    tauri::async_runtime::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let (Ok(mut interrupt), Ok(mut terminate)) = (
            signal(SignalKind::interrupt()),
            signal(SignalKind::terminate()),
        ) else {
            tracing::warn!(
                event = "app.signal",
                outcome = "failed",
                "Cannot listen for Ctrl+C"
            );
            return;
        };
        let name = tokio::select! {
            _ = interrupt.recv() => "interrupt",
            _ = terminate.recv() => "terminate",
        };
        tracing::info!(event = "app.signal", signal = name, "Quitting on a signal");
        app.exit(0);
    });
    #[cfg(not(unix))]
    drop(app);
}

/// Starts logging and the core, against the user's folders or, in a debug smoke run, a
/// throwaway one.
fn start(app: &mut tauri::App) -> Result<Arc<Core>, Box<dyn std::error::Error>> {
    let mut paths = user_paths(app)?;
    let smoke_home = smoke_home();
    logging::init(&match &smoke_home {
        Some(home) => home.db_path.with_file_name("logs"),
        None => paths.db_path.with_file_name("logs"),
    });
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
        smoke = smoke_home.is_some(),
        "Aoide started"
    );
    match smoke_home {
        Some(home) => {
            // The asset scope only names the real covers folder.
            app.asset_protocol_scope()
                .allow_directory(&home.covers_dir, true)?;
            paths = home;
        }
        None => migrate_from_folio(app, &paths)?,
    }
    #[allow(unused_mut)]
    let mut environment = crate::core::Environment::desktop(app.handle().clone());
    // Debug smoke runs (and AOIDE_MUTE) play into a silent, real-time-paced sink.
    #[cfg(debug_assertions)]
    if std::env::var_os("AOIDE_SMOKE_SECONDS").is_some() || std::env::var_os("AOIDE_MUTE").is_some()
    {
        environment.audio = crate::core::Environment::silent_audio();
    }
    let core = tauri::async_runtime::block_on(async move { Core::start(environment, paths) })?;
    #[cfg(debug_assertions)]
    if let Ok(seconds) = std::env::var("AOIDE_SMOKE_SECONDS") {
        smoke::spawn(
            app.handle().clone(),
            Arc::clone(&core),
            seconds.parse().unwrap_or(20),
        );
    }
    Ok(core)
}

/// Where the user's library, data, cache and settings live.
fn user_paths(app: &tauri::App) -> tauri::Result<Paths> {
    let path = app.path();
    let data = path.app_data_dir()?;
    Ok(Paths {
        home_dir: path.home_dir()?,
        library_dir: path.document_dir()?.join("Aoide").join("Books"),
        covers_dir: data.join("covers"),
        audio_dir: path.app_cache_dir()?.join("audio"),
        db_path: data.join("library.sqlite3"),
        models_dir: data.join("models"),
        articles_dir: data.join("articles"),
        browser_dir: data.join("browser"),
        config_dir: aoide_config::config_dir(
            std::env::var_os("XDG_CONFIG_HOME").as_deref(),
            &path.home_dir()?,
        ),
    })
}

/// A debug build's throwaway library (`AOIDE_SMOKE_HOME`), used instead of the real one.
fn smoke_home() -> Option<Paths> {
    #[cfg(debug_assertions)]
    if let Some(home) = std::env::var_os("AOIDE_SMOKE_HOME") {
        return Some(smoke::paths(Path::new(&home)));
    }
    None
}

/// First start after the rename from Folio: brings its folders along.
fn migrate_from_folio(app: &tauri::App, paths: &Paths) -> tauri::Result<()> {
    let data = app.path().app_data_dir()?;
    let cache = app.path().app_cache_dir()?;
    let mut moves: Vec<(PathBuf, PathBuf)> = vec![
        (data.with_file_name(rename::OLD_IDENTIFIER), data.clone()),
        (cache.with_file_name(rename::OLD_IDENTIFIER), cache),
        (
            paths.config_dir.with_file_name(rename::OLD_CONFIG),
            paths.config_dir.clone(),
        ),
    ];
    if let Some(documents) = paths.library_dir.parent() {
        moves.push((
            documents.with_file_name(rename::OLD_DOCUMENTS),
            documents.to_path_buf(),
        ));
    }
    for line in rename::move_folders(&moves, &paths.db_path) {
        tracing::info!(event = "app.migrate", "{line}");
    }
    Ok(())
}
