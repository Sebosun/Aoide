//! The books on the shelf: the library folder's EPUBs, the ones the reader removed (kept out
//! of every scan), and the parsed books kept for reuse.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_library::{
    content_hash, scan_folder, BookId, BookKey, BookRecord, BookStatus, GlobalKey, Library, NewBook,
};

use super::error::Reporter;
use super::facets::{self, AudioFacts};
use super::syncing::Stage as SyncStage;
use super::{
    lock, now_ms, BookProgressView, BookSummary, CoreEvent, Events, ImportResult, PrepareView,
};
use crate::book::BookModel;

/// What the shelf asks of the rest of the app.
pub(super) trait ShelfHost {
    /// Whether book `id` is read with its notes.
    fn reads_notes(&self, id: &str) -> bool;
    /// The book being listened to; it stays parsed.
    fn playing(&self) -> Option<String>;
    /// Book `id`'s preparation, as its summary shows it.
    fn prepare_of(&self, id: &str) -> Option<PrepareView>;
    /// Book `id`'s recording, as its summary shows it.
    fn recording_of(&self, id: &str) -> Option<SyncStage>;
    /// Whether book `id` is narrated by its recording, and the recording can be listened to.
    fn narrates_recording(&self, id: &str) -> bool;
}

pub(super) struct Shelf {
    library: Arc<Mutex<Library>>,
    library_dir: PathBuf,
    covers_dir: PathBuf,
    reporter: Reporter,
    events: Arc<dyn Events>,
    /// Parsed books: the last few opened, and the one playing.
    books: Mutex<HashMap<String, Arc<BookModel>>>,
}

impl Shelf {
    pub fn new(
        library: Arc<Mutex<Library>>,
        library_dir: PathBuf,
        covers_dir: PathBuf,
        reporter: Reporter,
        events: Arc<dyn Events>,
    ) -> Self {
        Self {
            library,
            library_dir,
            covers_dir,
            reporter,
            events,
            books: Mutex::new(HashMap::new()),
        }
    }

    /// The books the reader removed; scans leave them out.
    pub fn ignored(&self) -> HashSet<String> {
        let stored = lock(&self.library).value(GlobalKey::IgnoredBooks);
        stored
            .unwrap_or_else(|error| {
                self.reporter
                    .report("library.read", "Could not read the books left out", error);
                None
            })
            .unwrap_or_default()
    }

    fn set_ignored(&self, ignored: &HashSet<String>) {
        if let Err(error) = lock(&self.library).set_value(GlobalKey::IgnoredBooks, ignored) {
            self.reporter
                .report("library.save", "Could not save the books left out", error);
        }
    }

    /// Takes books back that were removed (they are being added again).
    pub fn restore<'a>(&self, ids: impl IntoIterator<Item = &'a str>) {
        let mut ignored = self.ignored();
        for id in ids {
            ignored.remove(id);
        }
        self.set_ignored(&ignored);
    }

    /// Scans the library folder, importing new books and marking vanished ones missing.
    pub fn rescan(&self) -> Result<(), String> {
        let started = Instant::now();
        let files = scan_folder(&self.library_dir).map_err(|e| e.to_string())?;
        let (mut new, mut read) = (0usize, 0usize);
        let ignored = self.ignored();
        let mut present = Vec::new();
        for file in files {
            if ignored.contains(&file.id.0) {
                continue;
            }
            present.push(file.id.clone());
            let existing = lock(&self.library)
                .book(&file.id)
                .map_err(|e| e.to_string())?;
            let book = match existing {
                Some(record) if !matches!(record.status, BookStatus::Error(_)) => NewBook {
                    id: record.id,
                    path: file.path.clone(),
                    title: record.title,
                    authors: record.authors,
                    language: record.language,
                    cover_path: record.cover_path,
                    error: None,
                },
                existing => {
                    if existing.is_none() {
                        new += 1;
                    }
                    read += 1;
                    self.describe(&file.path, &file.id)
                }
            };
            lock(&self.library)
                .upsert_book(&book, now_ms())
                .map_err(|e| e.to_string())?;
        }
        lock(&self.library)
            .mark_missing_except(&present)
            .map_err(|e| e.to_string())?;
        tracing::info!(
            event = "library.scan",
            outcome = "ok",
            books = present.len(),
            new,
            read,
            took_ms = crate::logging::took(started),
            "Scanned the library folder"
        );
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// Parses a new book's metadata and extracts its cover.
    fn describe(&self, path: &Path, id: &BookId) -> NewBook {
        let fallback_title = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let parsed = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| {
                aoide_epub::parse_epub(&bytes, &aoide_epub::ParseOptions::default())
                    .map_err(|e| e.to_string())
            })
            .and_then(|book: aoide_epub::ParsedBook| {
                // Parsing can succeed with nothing to read (every section empty or left out).
                if book.chapters.iter().any(|c| !c.text.trim().is_empty()) {
                    Ok(book)
                } else {
                    Err("no readable text was found in this EPUB".into())
                }
            });
        match parsed {
            Ok(book) => {
                let cover_path = book.cover.as_ref().and_then(|cover| {
                    let ext = match cover.media_type.as_str() {
                        "image/png" => "png",
                        "image/gif" => "gif",
                        "image/webp" => "webp",
                        "image/svg+xml" => "svg",
                        _ => "jpg",
                    };
                    let target = self.covers_dir.join(format!("{}.{ext}", id.0));
                    std::fs::write(&target, &cover.bytes).ok().map(|_| target)
                });
                NewBook {
                    id: id.clone(),
                    path: path.to_path_buf(),
                    title: book.title.or(fallback_title),
                    authors: book.authors,
                    language: book.language,
                    cover_path,
                    error: None,
                }
            }
            Err(error) => NewBook {
                id: id.clone(),
                path: path.to_path_buf(),
                title: fallback_title,
                authors: vec![],
                language: None,
                cover_path: None,
                error: Some(error),
            },
        }
    }

    /// Scans again whenever the library folder changes; returns only if it cannot watch.
    pub fn watch(&self) {
        use notify::{RecursiveMode, Watcher};
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(error) => {
                tracing::warn!(event = "library.watch", %error, "Cannot watch the library folder");
                return;
            }
        };
        if let Err(error) = watcher.watch(&self.library_dir, RecursiveMode::Recursive) {
            tracing::warn!(event = "library.watch", %error, "Cannot watch the library folder");
            return;
        }
        while rx.recv().is_ok() {
            // Debounce bursts (copies emit many events).
            while rx.recv_timeout(Duration::from_millis(1200)).is_ok() {}
            if let Err(error) = self.rescan() {
                tracing::warn!(event = "library.scan", outcome = "failed", %error, "Library rescan failed");
            }
        }
        drop(watcher);
    }

    pub fn import_files(&self, paths: Vec<String>) -> Vec<ImportResult> {
        let mut ignored = self.ignored();
        let results: Vec<ImportResult> = paths
            .into_iter()
            .map(|path| {
                let source = PathBuf::from(&path);
                let outcome = (|| -> Result<String, String> {
                    if !source
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
                    {
                        return Err("not an .epub file".into());
                    }
                    let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
                    let id = content_hash(&bytes);
                    ignored.remove(&id.0);
                    if !source.starts_with(&self.library_dir) {
                        let name = source.file_name().ok_or("file has no name")?;
                        let mut target = self.library_dir.join(name);
                        let mut n = 1;
                        while target.exists() {
                            if std::fs::read(&target).is_ok_and(|b| content_hash(&b) == id) {
                                break;
                            }
                            let stem = source.file_stem().unwrap_or_default().to_string_lossy();
                            target = self.library_dir.join(format!("{stem} ({n}).epub"));
                            n += 1;
                        }
                        if !target.exists() {
                            std::fs::write(&target, &bytes).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(id.0)
                })();
                match outcome {
                    Ok(id) => ImportResult {
                        path,
                        book_id: Some(id),
                        error: None,
                    },
                    Err(error) => ImportResult {
                        path,
                        book_id: None,
                        error: Some(error),
                    },
                }
            })
            .collect();
        self.set_ignored(&ignored);
        if let Err(error) = self.rescan() {
            tracing::warn!(event = "library.scan", outcome = "failed", %error, "Library rescan after an import failed");
        }
        // A copied file the parser could not read is a failed import, not a book.
        let library = lock(&self.library);
        let results: Vec<ImportResult> = results
            .into_iter()
            .map(|result| {
                let failed = result.book_id.as_ref().and_then(|id| {
                    match library.book(&BookId(id.clone())).ok().flatten()?.status {
                        BookStatus::Error(message) => Some(message),
                        _ => None,
                    }
                });
                match failed {
                    Some(message) => ImportResult {
                        path: result.path,
                        book_id: None,
                        error: Some(format!("could not import: {message}")),
                    },
                    None => result,
                }
            })
            .collect();
        drop(library);
        let failed = results.iter().filter(|r| r.error.is_some()).count();
        tracing::info!(
            event = "library.import",
            ok = results.len() - failed,
            failed,
            "Added EPUB files"
        );
        for result in results.iter().filter(|r| r.error.is_some()) {
            tracing::warn!(
                event = "library.import",
                file = %crate::logging::file(Path::new(&result.path)),
                outcome = "failed",
                error = result.error.as_deref().unwrap_or_default(),
                "Could not add an EPUB file"
            );
        }
        results
    }

    /// Takes book `id` off the shelf and keeps it out of later scans; returns its audio
    /// files, relative to the audio folder.
    pub fn remove(&self, id: &str) -> Result<Vec<PathBuf>, String> {
        let files = lock(&self.library)
            .remove_book(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        let mut ignored = self.ignored();
        ignored.insert(id.into());
        self.set_ignored(&ignored);
        self.unload(id);
        Ok(files)
    }

    pub fn progress(&self, id: &BookId) -> Option<BookProgressView> {
        let library = lock(&self.library);
        let progress = library.progress(id).ok().flatten()?;
        let fraction = library
            .book_value::<f64>(id, BookKey::ProgressFraction)
            .ok()
            .flatten()
            .unwrap_or(0.0);
        Some(BookProgressView {
            chapter: progress.chapter,
            segment_id: progress.segment_id,
            fraction,
        })
    }

    pub fn list(&self, host: &dyn ShelfHost) -> Result<Vec<BookSummary>, String> {
        let (records, mut shelves, listened) = {
            let library = lock(&self.library);
            (
                library.books().map_err(|e| e.to_string())?,
                library.book_collections().map_err(|e| e.to_string())?,
                library
                    .listening_stats()
                    .map_err(|e| e.to_string())?
                    .by_book
                    .into_iter()
                    .filter(|b| b.ms > 0)
                    .map(|b| b.book_id)
                    .collect::<HashSet<BookId>>(),
            )
        };
        let ignored = self.ignored();
        Ok(records
            .into_iter()
            .filter(|r| !ignored.contains(&r.id.0))
            .map(|record| {
                let collections = shelves.remove(&record.id).unwrap_or_default();
                let listened = listened.contains(&record.id);
                self.summary(record, collections, listened, host)
            })
            .collect())
    }

    fn summary(
        &self,
        record: BookRecord,
        collections: Vec<i64>,
        listened: bool,
        host: &dyn ShelfHost,
    ) -> BookSummary {
        let audio_bytes = lock(&self.library)
            .book_audio_bytes(&record.id)
            .unwrap_or(0);
        let prepare = host.prepare_of(&record.id.0);
        let audio = facets::audio_source(AudioFacts {
            narrates_recording: host.narrates_recording(&record.id.0),
            cached_speech: audio_bytes > 0,
            prepared: prepare.is_some(),
            listened,
        });
        BookSummary {
            prepare,
            audio,
            recording: host.recording_of(&record.id.0),
            progress: self.progress(&record.id),
            id: record.id.0,
            title: record.title.unwrap_or_else(|| "Untitled".into()),
            authors: record.authors,
            language: record.language,
            status: record.status,
            cover_path: record.cover_path.map(|p| p.to_string_lossy().into_owned()),
            audio_bytes,
            added_at: record.added_at,
            last_opened_at: record.last_opened_at,
            collections,
        }
    }

    /// Book `id` parsed, from the ones kept or read again.
    pub fn load(&self, id: &str, host: &dyn ShelfHost) -> Result<Arc<BookModel>, String> {
        if let Some(book) = lock(&self.books).get(id) {
            return Ok(Arc::clone(book));
        }
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("book not found")?;
        match record.status {
            BookStatus::Present => {}
            BookStatus::Missing => return Err("the book file is missing".into()),
            BookStatus::Error(message) => return Err(message),
        }
        let bytes = std::fs::read(&record.path).map_err(|e| e.to_string())?;
        let book = Arc::new(BookModel::parse(id, &bytes, host.reads_notes(id))?);
        let mut books = lock(&self.books);
        if books.len() >= 3 {
            let playing = host.playing();
            books.retain(|k, _| Some(k) == playing.as_ref());
        }
        books.insert(id.into(), Arc::clone(&book));
        Ok(book)
    }

    /// Book `id`'s EPUB file, if it is there.
    pub fn file(&self, id: &str) -> Result<PathBuf, String> {
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("no such book")?;
        if !record.path.is_file() {
            return Err(format!(
                "the book's file is missing: {}",
                record.path.display()
            ));
        }
        Ok(record.path)
    }

    /// Book `id`'s cover image, if it has one.
    pub fn cover(&self, id: &str) -> Option<String> {
        lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .and_then(|r| r.cover_path)
            .map(|p| p.to_string_lossy().into_owned())
    }

    /// Book `id`'s voice settings.
    pub fn settings(&self, id: &str) -> super::BookSettings {
        let stored = lock(&self.library).book_value(&BookId(id.into()), BookKey::Settings);
        stored
            .unwrap_or_else(|error| {
                self.reporter.report(
                    "book.read",
                    "Could not read the book's voice settings",
                    error,
                );
                None
            })
            .unwrap_or_default()
    }

    /// Saves book `id`'s voice settings; it is read again when next loaded (footnote
    /// narration changes how it is split).
    pub fn save_settings(&self, id: &str, settings: &super::BookSettings) -> Result<(), String> {
        lock(&self.library)
            .set_book_value(&BookId(id.into()), BookKey::Settings, settings)
            .map_err(|e| e.to_string())?;
        self.unload(id);
        Ok(())
    }

    /// Makes book `id` play its recording unless its narration was chosen already. Returns
    /// whether that changed it; it is read again when next loaded.
    pub fn prefer_recording(&self, id: &str) -> Result<bool, String> {
        let book = BookId(id.into());
        let changed = {
            let library = lock(&self.library);
            library
                .book_value::<super::BookSettings>(&book, BookKey::Settings)
                .and_then(|stored| {
                    let mut settings = stored.unwrap_or_default();
                    if settings.narration.is_some() {
                        return Ok(false);
                    }
                    settings.narration = Some(super::Narration::Recording);
                    library
                        .set_book_value(&book, BookKey::Settings, &settings)
                        .map(|()| true)
                })
        };
        if !matches!(changed, Ok(false)) {
            self.unload(id);
        }
        changed.map_err(|e| e.to_string())
    }

    /// Notes that book `id` was opened now.
    pub fn opened(&self, id: &str) {
        if let Err(error) = lock(&self.library).set_last_opened(&BookId(id.into()), now_ms()) {
            self.reporter.report(
                "library.save",
                "Could not save when the book was opened",
                error,
            );
        }
    }

    pub fn collections(&self) -> Result<Vec<aoide_library::Collection>, String> {
        lock(&self.library).collections().map_err(|e| e.to_string())
    }

    /// Runs a change to the collections.
    pub fn shelve<T>(
        &self,
        change: impl FnOnce(&Library) -> aoide_library::Result<T>,
    ) -> Result<T, String> {
        change(&lock(&self.library)).map_err(|e| e.to_string())
    }

    /// Forgets the parsed book `id`, so it is read again (how it is read changed).
    pub fn unload(&self, id: &str) {
        lock(&self.books).remove(id);
    }

    fn emit(&self, event: CoreEvent) {
        let name = event.name();
        match event.payload() {
            Ok(value) => self.events.emit(name, value),
            Err(error) => {
                tracing::error!(event = "app.emit", name, %error, "Cannot send an event to the window")
            }
        }
    }
}

impl ShelfHost for super::Core {
    fn reads_notes(&self, id: &str) -> bool {
        let settings = self.book_settings(id);
        // A recording is laid onto the book's own text, without footnotes read in between.
        settings.read_notes && !self.plays_recording(id, &settings)
    }

    fn playing(&self) -> Option<String> {
        self.playback.book().map(|book| book.id.clone())
    }

    fn prepare_of(&self, id: &str) -> Option<PrepareView> {
        self.prepare_view(id)
    }

    fn recording_of(&self, id: &str) -> Option<SyncStage> {
        self.recording_stage(id)
    }

    fn narrates_recording(&self, id: &str) -> bool {
        self.plays_recording(id, &self.book_settings(id))
    }
}

impl super::Core {
    /// Scans the library folder, importing new books and marking vanished ones missing.
    pub fn rescan(&self) -> Result<(), String> {
        self.shelf.rescan()
    }

    pub fn import_files(&self, paths: Vec<String>) -> Vec<ImportResult> {
        self.shelf.import_files(paths)
    }

    pub fn list_books(&self) -> Result<Vec<BookSummary>, String> {
        self.shelf.list(self)
    }

    pub fn load_book(&self, id: &str) -> Result<Arc<BookModel>, String> {
        self.shelf.load(id, self)
    }

    /// Opens the file manager on the book's folder with its EPUB selected.
    pub fn show_in_folder(&self, id: &str) -> Result<(), String> {
        (self.show_in_folder)(&self.shelf.file(id)?)
    }

    pub fn collections(&self) -> Result<Vec<aoide_library::Collection>, String> {
        self.shelf.collections()
    }

    pub fn create_collection(&self, name: &str) -> Result<i64, String> {
        let id = self
            .shelf
            .shelve(|library| library.create_collection(name, now_ms()))?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(id)
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> Result<(), String> {
        self.shelf
            .shelve(|library| library.rename_collection(id, name))?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub fn delete_collection(&self, id: i64) -> Result<(), String> {
        self.shelf.shelve(|library| library.delete_collection(id))?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub fn set_book_collections(&self, book_id: &str, ids: &[i64]) -> Result<(), String> {
        self.shelf
            .shelve(|library| library.set_book_collections(&BookId(book_id.into()), ids))?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub fn open_book(&self, id: &str) -> Result<super::BookView, String> {
        let view = self.book_overview(id)?;
        self.shelf.opened(id);
        Ok(view)
    }

    /// The same view as [`Core::open_book`], without marking the book as opened.
    pub fn book_overview(&self, id: &str) -> Result<super::BookView, String> {
        let book = self.load_book(id)?;
        Ok(super::BookView {
            id: book.id.clone(),
            title: book.title.clone(),
            authors: book.authors.clone(),
            language: book.language.clone(),
            chapters: book.chapter_summaries(),
            warnings: book.warnings.clone(),
            excluded: book.excluded.clone(),
            language_warning: self.language_warning(book.language.as_deref()),
            voice_warning: self.voice_warning(&book),
            progress: self.shelf.progress(&BookId(id.into())),
            word_count: book.word_count(),
        })
    }

    pub fn chapter(&self, id: &str, chapter: usize) -> Result<crate::book::ChapterView, String> {
        let book = self.load_book(id)?;
        let mut view = book
            .chapter_view(chapter)
            .ok_or_else(|| "no such chapter".to_owned())?;
        view.not_narrated = self.not_narrated(book, chapter);
        Ok(view)
    }
}

impl super::Core {
    pub fn book_settings(&self, id: &str) -> super::BookSettings {
        self.shelf.settings(id)
    }

    pub async fn save_book_settings(
        &self,
        id: &str,
        settings: super::BookSettings,
    ) -> Result<(), String> {
        self.engine.check_params(&settings.params)?;
        self.shelf.save_settings(id, &settings)?;
        // Whether its recording narrates it may have changed.
        self.emit_recording(id);
        if self.playback.is_book(id) {
            self.restart_session().await?;
        }
        self.plan(super::PlanEvent::BookSettingsChanged(id.into()));
        Ok(())
    }

    pub fn remove_book(&self, id: &str) -> Result<(), String> {
        if self.playback.is_book(id) {
            self.stop();
        }
        self.plan(super::PlanEvent::BookForgotten(id.into()));
        self.forget_recording(id);
        let files = self.shelf.remove(id)?;
        self.storage.delete_audio(id, files);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use aoide_library::BookStatus;

    use super::super::tests::{epub, Recorder};
    use super::*;

    struct Host;

    impl ShelfHost for Host {
        fn reads_notes(&self, _: &str) -> bool {
            false
        }
        fn playing(&self) -> Option<String> {
            None
        }
        fn prepare_of(&self, _: &str) -> Option<PrepareView> {
            None
        }
        fn recording_of(&self, _: &str) -> Option<SyncStage> {
            None
        }
        fn narrates_recording(&self, _: &str) -> bool {
            false
        }
    }

    fn shelf(root: &Path) -> Shelf {
        let (books, covers) = (root.join("books"), root.join("covers"));
        std::fs::create_dir_all(&books).unwrap();
        std::fs::create_dir_all(&covers).unwrap();
        let events: Arc<dyn Events> = Arc::new(Recorder::default());
        Shelf::new(
            Arc::new(Mutex::new(Library::open_in_memory().unwrap())),
            books,
            covers,
            Reporter::default(),
            events,
        )
    }

    fn listed(shelf: &Shelf) -> Vec<(String, BookStatus)> {
        shelf
            .list(&Host)
            .unwrap()
            .into_iter()
            .map(|book| (book.id, book.status))
            .collect()
    }

    #[test]
    fn importing_an_epub_copies_it_into_the_library_and_lists_it() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let outside = dir.path().join("Elsewhere.epub");
        std::fs::write(&outside, epub()).unwrap();

        let results = shelf.import_files(vec![outside.to_string_lossy().into_owned()]);

        let id = results[0].book_id.clone().expect("imported");
        assert!(dir.path().join("books/Elsewhere.epub").is_file());
        assert_eq!(listed(&shelf), [(id.clone(), BookStatus::Present)]);
        assert_eq!(shelf.load(&id, &Host).unwrap().id, id);
    }

    #[test]
    fn a_rescan_marks_a_book_whose_file_is_gone_missing() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let file = dir.path().join("books/Gone.epub");
        std::fs::write(&file, epub()).unwrap();
        shelf.rescan().unwrap();
        let id = listed(&shelf)[0].0.clone();

        std::fs::remove_file(&file).unwrap();
        shelf.rescan().unwrap();

        assert_eq!(listed(&shelf), [(id, BookStatus::Missing)]);
    }

    #[test]
    fn a_removed_book_stays_out_of_later_rescans() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        std::fs::write(dir.path().join("books/Kept.epub"), epub()).unwrap();
        shelf.rescan().unwrap();
        let id = listed(&shelf)[0].0.clone();

        shelf.remove(&id).unwrap();
        shelf.rescan().unwrap();

        assert!(listed(&shelf).is_empty());
        assert!(shelf.ignored().contains(&id));
    }
}
