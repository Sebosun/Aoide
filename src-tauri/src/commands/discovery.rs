use super::prelude::*;

/// Scans the configured folders; resolves with everything found.
#[tauri::command]
pub async fn scan_for_books(core: CoreState<'_>) -> Result<Vec<crate::core::Candidate>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.scan_for_books().await?)
}

#[tauri::command]
pub fn cancel_scan(core: CoreState<'_>) {
    core.cancel_scan();
}

/// Results of the last scan, compared with the library.
#[tauri::command]
pub async fn discovered_books(
    core: CoreState<'_>,
) -> Result<Vec<crate::core::Candidate>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || core.discovered_books()).await
}

/// Searches the audiobook folders for every book without a recording; with verifying on, Aoide
/// starts pairing them itself. "audiobooks://progress" reports the search.
#[tauri::command]
pub async fn scan_for_audiobooks(
    core: CoreState<'_>,
) -> Result<Vec<crate::core::AudiobookMatch>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.scan_for_audiobooks().await?)
}

/// The matches the last searches found, and where each stands.
#[tauri::command]
pub fn audiobook_matches(core: CoreState<'_>) -> Vec<crate::core::AudiobookMatch> {
    core.audiobook_matches()
}

/// Gives an audiobook on its own the book it reads (already imported, with no recording): the book
/// takes its recording, which syncs, its bookmarks, collections and listening time, and its place
/// once the sync reaches it. The audiobook item is removed.
#[tauri::command]
pub fn merge_audiobook(
    core: CoreState<'_>,
    audiobook_id: String,
    book_id: String,
) -> Result<(), CoreError> {
    Ok(core.merge_audiobook(&audiobook_id, &book_id)?)
}

/// What the last whole search found that comes close to no book, to add on its own.
#[tauri::command]
pub fn unmatched_audiobooks(core: CoreState<'_>) -> Vec<crate::core::AudiobookSuggestion> {
    core.unmatched_audiobooks()
}

#[tauri::command]
pub fn cancel_audiobook_scan(core: CoreState<'_>) {
    core.cancel_audiobook_scan();
}

/// After books were added: how many have audiobooks waiting for the user (0 when looking on adding
/// is off or Aoide verifies them).
#[tauri::command]
pub async fn look_for_audiobooks(
    core: CoreState<'_>,
    ids: Vec<String>,
) -> Result<usize, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.look_for_audiobooks(ids).await?)
}

/// Pairs each chosen candidate; each succeeds or fails on its own.
#[tauri::command]
pub async fn pair_audiobooks(
    core: CoreState<'_>,
    choices: Vec<crate::core::AudiobookChoice>,
) -> Result<Vec<crate::core::PairOutcome>, CoreError> {
    let core = Arc::clone(&core);
    blocking(move || Ok::<_, CoreError>(core.pair_audiobooks(choices))).await
}

/// Open Library lookups for these candidates; "discovery://details" fires per id.
#[tauri::command]
pub async fn fetch_discovered_details(
    core: CoreState<'_>,
    ids: Vec<String>,
) -> Result<(), CoreError> {
    let core = Arc::clone(&core);
    Ok(core.fetch_discovered_details(ids).await?)
}

/// `pairAudiobook` pairs the audiobook found beside a book once it is imported.
#[tauri::command]
pub async fn import_discovered(
    core: CoreState<'_>,
    choices: Vec<crate::core::ImportChoice>,
) -> Result<Vec<ImportResult>, CoreError> {
    let core = Arc::clone(&core);
    Ok(core.import_discovered(choices).await?)
}

/// The system's file chooser for books (EPUBs and PDFs) and saved emails to add; null when
/// cancelled.
#[tauri::command]
pub async fn choose_books(app: tauri::AppHandle) -> Option<Vec<String>> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Choose books to add")
            .add_filter(
                "Books and saved emails (EPUB, PDF, DOCX, FB2, TXT, Markdown, HTML, EML)",
                &[aoide_library::BOOK_EXTENSIONS.as_slice(), &["eml"]].concat(),
            )
            .pick_files(move |files| done(files.map(|f| f.iter().map(|p| p.to_string()).collect())))
    })
    .await
}

/// The system's file chooser for a picture to use as a cover; null when cancelled.
#[tauri::command]
pub async fn choose_image(app: tauri::AppHandle) -> Option<String> {
    pick(&app, |dialog, done| {
        dialog
            .set_title("Choose a cover picture")
            .add_filter("Pictures", &["jpg", "jpeg", "png", "webp", "gif", "bmp"])
            .pick_file(move |file| done(file.map(|p| p.to_string())))
    })
    .await
}

/// The system's folder chooser, titled `title` (what the folder is for); null when cancelled.
#[tauri::command]
pub async fn choose_folder(app: tauri::AppHandle, title: String) -> Option<String> {
    pick(&app, move |dialog, done| {
        dialog
            .set_title(title)
            .pick_folder(move |p| done(p.map(|p| p.to_string())));
    })
    .await
}

/// The system's file chooser for audiobook files (or a folder); null when cancelled.
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
