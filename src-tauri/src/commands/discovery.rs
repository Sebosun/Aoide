use super::prelude::*;

#[tauri::command]
pub async fn scan_for_books(core: CoreState<'_>) -> Result<Vec<crate::core::Candidate>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.scan_for_books().await?)
}

#[tauri::command]
pub fn cancel_scan(core: CoreState<'_>) {
    core.cancel_scan();
}

#[tauri::command]
pub async fn discovered_books(
    core: CoreState<'_>,
) -> Result<Vec<crate::core::Candidate>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.discovered_books()).await
}

#[tauri::command]
pub async fn scan_for_audiobooks(
    core: CoreState<'_>,
) -> Result<Vec<crate::core::AudiobookMatch>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.scan_for_audiobooks().await?)
}

#[tauri::command]
pub fn audiobook_matches(core: CoreState<'_>) -> Vec<crate::core::AudiobookMatch> {
    core.audiobook_matches()
}

#[tauri::command]
pub fn merge_audiobook(
    core: CoreState<'_>,
    audiobook_id: String,
    book_id: String,
) -> Result<(), CoreError> {
    Ok(core.merge_audiobook(&audiobook_id, &book_id)?)
}

#[tauri::command]
pub fn unmatched_audiobooks(core: CoreState<'_>) -> Vec<crate::core::AudiobookSuggestion> {
    core.unmatched_audiobooks()
}

#[tauri::command]
pub fn cancel_audiobook_scan(core: CoreState<'_>) {
    core.cancel_audiobook_scan();
}

/// After books were added: how many of them have audiobooks waiting for the user.
#[tauri::command]
pub async fn look_for_audiobooks(
    core: CoreState<'_>,
    ids: Vec<String>,
) -> Result<usize, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.look_for_audiobooks(ids).await?)
}

#[tauri::command]
pub async fn pair_audiobooks(
    core: CoreState<'_>,
    choices: Vec<crate::core::AudiobookChoice>,
) -> Result<Vec<crate::core::PairOutcome>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.pair_audiobooks(choices))).await
}

#[tauri::command]
pub async fn fetch_discovered_details(
    core: CoreState<'_>,
    ids: Vec<String>,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.fetch_discovered_details(ids).await?)
}

#[tauri::command]
pub async fn import_discovered(
    core: CoreState<'_>,
    choices: Vec<crate::core::ImportChoice>,
) -> Result<Vec<ImportResult>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.import_discovered(choices).await?)
}

/// The system's file chooser for EPUB files to add. `None` when cancelled.
#[tauri::command]
pub async fn choose_books(app: tauri::AppHandle) -> Option<Vec<String>> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Choose EPUB files to add")
            .add_filter("EPUB books", &["epub"])
            .pick_files(move |files| done(files.map(|f| f.iter().map(|p| p.to_string()).collect())))
    })
    .await
}

/// The system's file chooser for audiobook files, or a folder of them. `None` when cancelled.
#[tauri::command]
pub async fn choose_audio(app: tauri::AppHandle, folder: bool) -> Option<Vec<String>> {
    const AUDIO: [&str; 10] = [
        "m4b", "m4a", "mp4", "aac", "mp3", "flac", "ogg", "oga", "opus", "wav",
    ];
    pick(&app, |dialog, done| {
        if folder {
            dialog
                .set_title("Choose the audiobook's folder")
                .pick_folder(move |p| done(p.map(|p| vec![p.to_string()])));
        } else {
            dialog
                .set_title("Choose the audiobook's files")
                .add_filter("Audiobooks", &AUDIO)
                .pick_files(move |files| {
                    done(files.map(|f| f.iter().map(|p| p.to_string()).collect()))
                });
        }
    })
    .await
}
