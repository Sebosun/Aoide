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
use super::reading;
use super::syncing::Stage as SyncStage;
use super::{
    lock, now_ms, BookProgressView, BookSummary, CoreEvent, Events, ImportResult, PrepareView,
};
use crate::book::BookModel;

/// What the shelf asks of the rest of the app.
pub(super) trait ShelfHost {
    /// How book `id`'s text is laid out: with its notes read or not, and the pauses.
    fn layout(&self, id: &str) -> crate::book::Layout;
    /// The book being listened to; it stays parsed.
    fn playing(&self) -> Option<String>;
    /// Book `id`'s preparation, as its summary shows it.
    fn prepare_of(&self, id: &str) -> Option<PrepareView>;
    /// Book `id`'s recording, as its summary shows it.
    fn recording_of(&self, id: &str) -> Option<SyncStage>;
    /// Whether book `id` is narrated by its recording, and the recording can be listened to.
    fn narrates_recording(&self, id: &str, settings: &super::BookSettings) -> bool;
    /// Article `id`'s stored page, read.
    fn article_document(&self, id: &str) -> Result<aoide_document::ParsedBook, String>;
    /// Article `id`'s site and fetch, as its summary shows them.
    fn article_of(&self, id: &str) -> Option<super::ArticleSummary>;
}

/// What listing the library shows of every book, read at once rather than book by book.
struct Stored {
    collections: HashMap<BookId, Vec<i64>>,
    tags: HashMap<BookId, Vec<String>>,
    /// Books listened to at all.
    listened: HashSet<BookId>,
    audio_bytes: HashMap<BookId, u64>,
    progress: HashMap<BookId, aoide_library::Progress>,
    fractions: HashMap<BookId, aoide_library::Result<f64>>,
    settings: HashMap<BookId, aoide_library::Result<super::BookSettings>>,
    /// Each recording's length.
    recorded_ms: HashMap<BookId, u64>,
}

impl Stored {
    fn read(library: &Library) -> aoide_library::Result<Self> {
        Ok(Self {
            collections: library.book_collections()?,
            tags: library.book_tags()?,
            listened: library
                .listening_stats()?
                .by_book
                .into_iter()
                .filter(|b| b.ms > 0)
                .map(|b| b.book_id)
                .collect(),
            audio_bytes: library
                .audio_usage()?
                .into_iter()
                .map(|usage| (usage.book_id, usage.bytes))
                .collect(),
            progress: library.all_progress()?,
            fractions: library.book_values(BookKey::ProgressFraction)?,
            settings: library.book_values(BookKey::Settings)?,
            recorded_ms: library
                .recordings()?
                .into_iter()
                .map(|row| (row.book_id, row.total_ms))
                .collect(),
        })
    }
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
        let known = lock(&self.library).scanned_files().unwrap_or_else(|error| {
            tracing::warn!(event = "library.scan", %error, "Could not read the last scan; every file is read again");
            Vec::new()
        });
        let files = scan_folder(&self.library_dir, &known).map_err(|e| e.to_string())?;
        if let Err(error) = lock(&self.library).store_scanned_files(&files) {
            tracing::warn!(event = "library.scan", %error, "Could not remember this scan; the next reads every file again");
        }
        let (mut new, mut read) = (0usize, 0usize);
        let ignored = self.ignored();
        let mut present = Vec::new();
        // Books listed before tags came have their subjects read once.
        let subjects_read = matches!(
            lock(&self.library).value::<bool>(GlobalKey::SubjectsRead),
            Ok(Some(true))
        );
        for file in files {
            if ignored.contains(&file.id.0) {
                continue;
            }
            let existing = lock(&self.library)
                .book(&file.id)
                .map_err(|e| e.to_string())?;
            let (book, subjects) = match existing {
                Some(record) if !matches!(record.status, BookStatus::Error(_)) => {
                    let subjects = if subjects_read {
                        Vec::new()
                    } else {
                        // Its EPUB's, and those of a record looked up before tags came.
                        let looked_up = lock(&self.library)
                            .book_details(&record.id)
                            .ok()
                            .flatten()
                            .map(|d| d.subjects)
                            .unwrap_or_default();
                        [subjects_of(&file.path), looked_up].concat()
                    };
                    let record = NewBook {
                        id: record.id,
                        path: file.path.clone(),
                        title: record.title,
                        authors: record.authors,
                        language: record.language,
                        cover_path: record.cover_path,
                        error: None,
                        kind: aoide_library::ItemKind::Book,
                    };
                    (record, subjects)
                }
                existing => {
                    let described = self.describe(&file.path, &file.id);
                    // A PDF that cannot be read (a scan has no text) is passed over, not listed.
                    if described.0.error.is_some() && is_pdf_file(&file.path) {
                        continue;
                    }
                    if existing.is_none() {
                        new += 1;
                    }
                    read += 1;
                    described
                }
            };
            present.push(file.id.clone());
            let library = lock(&self.library);
            library
                .upsert_book(&book, now_ms())
                .map_err(|e| e.to_string())?;
            let tags = aoide_library::tidy_tags(&subjects);
            if !tags.is_empty() {
                library
                    .add_tags(&book.id, &tags, aoide_library::TagSource::Metadata)
                    .map_err(|e| e.to_string())?;
            }
        }
        if !subjects_read {
            if let Err(error) = lock(&self.library).set_value(GlobalKey::SubjectsRead, &true) {
                self.reporter.report(
                    "library.save",
                    "Could not note that the books' subjects were read",
                    error,
                );
            }
        }
        {
            let library = lock(&self.library);
            library
                .mark_missing_except(&present)
                .map_err(|e| e.to_string())?;
            // An audiobook is where it was added: missing while its file or folder is gone.
            for record in library.books().map_err(|e| e.to_string())? {
                if record.kind == aoide_library::ItemKind::Audiobook {
                    library
                        .set_missing(&record.id, !record.path.exists())
                        .map_err(|e| e.to_string())?;
                }
            }
        }
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
    /// The book's record from its file, and its subjects.
    fn describe(&self, path: &Path, id: &BookId) -> (NewBook, Vec<String>) {
        let fallback_title = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let parsed = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| crate::book::parse_document(&bytes))
            .and_then(|book: aoide_epub::ParsedBook| {
                // Parsing can succeed with nothing to read (every section empty or left out).
                if book.chapters.iter().any(|c| !c.text.trim().is_empty()) {
                    Ok(book)
                } else {
                    Err("no readable text was found in this book".into())
                }
            });
        match parsed {
            Ok(book) => {
                let cover_path = book.cover.as_ref().and_then(|cover| {
                    super::item::store_cover(
                        &self.covers_dir,
                        &id.0,
                        &cover.media_type,
                        &cover.bytes,
                        super::item::Untyped::Jpeg,
                    )
                });
                let record = NewBook {
                    id: id.clone(),
                    path: path.to_path_buf(),
                    title: book.title.or(fallback_title),
                    authors: book.authors,
                    language: book.language,
                    cover_path,
                    error: None,
                    kind: aoide_library::ItemKind::Book,
                };
                (record, book.subjects)
            }
            Err(error) => {
                let record = NewBook {
                    id: id.clone(),
                    path: path.to_path_buf(),
                    title: fallback_title,
                    authors: vec![],
                    language: None,
                    cover_path: None,
                    error: Some(error),
                    kind: aoide_library::ItemKind::Book,
                };
                (record, Vec::new())
            }
        }
    }

    /// Scans again whenever what the library folder holds changes (reading a book is not a
    /// change); returns only if it cannot watch.
    pub fn watch(&self) {
        // A copy comes as a burst of events: scan once it has settled.
        let watched = super::watching::watch_folder(
            &self.library_dir,
            Duration::from_millis(1200),
            || {
                if let Err(error) = self.rescan() {
                    tracing::warn!(event = "library.scan", outcome = "failed", %error, "Library rescan failed");
                }
                std::ops::ControlFlow::Continue(())
            },
        );
        if let Err(error) = watched {
            tracing::warn!(event = "library.watch", %error, "Cannot watch the library folder");
        }
    }

    pub fn import_files(&self, paths: Vec<String>) -> Vec<ImportResult> {
        let mut ignored = self.ignored();
        let results: Vec<ImportResult> = paths
            .into_iter()
            .map(|path| {
                let source = PathBuf::from(&path);
                let outcome = (|| -> Result<String, String> {
                    if !aoide_library::is_book_file(&source) {
                        return Err("not an .epub or .pdf file".into());
                    }
                    let bytes = std::fs::read(&source).map_err(|e| e.to_string())?;
                    // A PDF with no text to read (a scan) is not taken in at all.
                    if aoide_pdf::is_pdf(&bytes) {
                        aoide_pdf::check_text(&bytes).map_err(|e| e.to_string())?;
                    }
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
                            let extension =
                                source.extension().unwrap_or_default().to_string_lossy();
                            target = self.library_dir.join(format!("{stem} ({n}).{extension}"));
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
        let kind = lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .map(|r| r.kind);
        let files = lock(&self.library)
            .remove_book(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        // Only EPUBs in the library folder come back by themselves; keep them out of scans.
        if kind == Some(aoide_library::ItemKind::Book) {
            let mut ignored = self.ignored();
            ignored.insert(id.into());
            self.set_ignored(&ignored);
        }
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
            position_ms: progress.position_ms,
        })
    }

    pub fn list(&self, host: &dyn ShelfHost) -> Result<Vec<BookSummary>, String> {
        let (records, mut stored) = {
            let library = lock(&self.library);
            let records = library.books().map_err(|e| e.to_string())?;
            (records, Stored::read(&library).map_err(|e| e.to_string())?)
        };
        let ignored = self.ignored();
        Ok(records
            .into_iter()
            .filter(|r| !ignored.contains(&r.id.0))
            .map(|record| self.summary(record, &mut stored, host))
            .collect())
    }

    fn summary(
        &self,
        record: BookRecord,
        stored: &mut Stored,
        host: &dyn ShelfHost,
    ) -> BookSummary {
        let id = &record.id;
        let audio_bytes = stored.audio_bytes.get(id).copied().unwrap_or(0);
        let settings = match stored.settings.remove(id) {
            Some(Ok(settings)) => settings,
            Some(Err(error)) => {
                self.reporter.report(
                    "book.read",
                    "Could not read the book's voice settings",
                    error,
                );
                super::BookSettings::default()
            }
            None => super::BookSettings::default(),
        };
        let prepare = host.prepare_of(&id.0);
        let facts = AudioFacts {
            narrates_recording: host.narrates_recording(&id.0, &settings),
            cached_speech: audio_bytes > 0,
            prepared: prepare.is_some(),
            listened: stored.listened.contains(id),
        };
        let audio = facets::audio_source(facts);
        let mode = reading::mode_for(settings.mode, &facts);
        let progress = stored.progress.remove(id).map(|progress| BookProgressView {
            chapter: progress.chapter,
            segment_id: progress.segment_id,
            fraction: stored
                .fractions
                .get(id)
                .and_then(|f| f.as_ref().ok())
                .copied()
                .unwrap_or(0.0),
            position_ms: progress.position_ms,
        });
        let duration_ms = (record.kind == aoide_library::ItemKind::Audiobook)
            .then(|| stored.recorded_ms.get(id).copied())
            .flatten();
        BookSummary {
            duration_ms,
            prepare,
            kind: record.kind,
            audio,
            mode,
            shape: reading::shape(record.kind != aoide_library::ItemKind::Audiobook, mode),
            read_state: reading::ReadState::stored(
                record.finished_at.is_some(),
                progress.is_some(),
            ),
            word_count: record.word_count,
            recording: host.recording_of(&id.0),
            article: (record.kind == aoide_library::ItemKind::Article)
                .then(|| host.article_of(&id.0))
                .flatten(),
            progress,
            collections: stored.collections.remove(id).unwrap_or_default(),
            tags: stored.tags.remove(id).unwrap_or_default(),
            id: record.id.0,
            title: record.title.unwrap_or_else(|| "Untitled".into()),
            authors: record.authors,
            language: record.language,
            status: record.status,
            // The window shows a large cover from its thumbnail (made once, see `item`).
            cover_path: record
                .cover_path
                .map(|p| super::item::shown_cover(&p).to_string_lossy().into_owned()),
            audio_bytes,
            added_at: record.added_at,
            last_opened_at: record.last_opened_at,
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
        if record.kind == aoide_library::ItemKind::Audiobook {
            return Err(super::item::NO_TEXT.into());
        }
        match record.status {
            BookStatus::Present => {}
            BookStatus::Missing => return Err("the book file is missing".into()),
            BookStatus::Error(message) => return Err(message),
        }
        let book = if record.kind == aoide_library::ItemKind::Article {
            let document = host.article_document(id)?;
            Arc::new(BookModel::from_parsed(id, document, &host.layout(id)))
        } else {
            let bytes = std::fs::read(&record.path).map_err(|e| e.to_string())?;
            Arc::new(BookModel::parse(id, &bytes, &host.layout(id))?)
        };
        // Its length, for reading-time estimates in lists that do not parse it.
        let words = u32::try_from(book.word_count()).unwrap_or(u32::MAX);
        if record.word_count != Some(words) {
            if let Err(error) = lock(&self.library).set_word_count(&record.id, words) {
                tracing::warn!(event = "library.word_count", book = crate::logging::book(id), %error, "Could not store the word count");
            }
        }
        let mut books = lock(&self.books);
        if books.len() >= 3 {
            let playing = host.playing();
            books.retain(|k, _| Some(k) == playing.as_ref());
        }
        books.insert(id.into(), Arc::clone(&book));
        Ok(book)
    }

    /// Book `id`'s file (EPUB or PDF), if it is there.
    pub fn file(&self, id: &str) -> Result<PathBuf, String> {
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("no such book")?;
        // An audiobook's path may be its folder.
        if !record.path.exists() {
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
    /// Stores the mode chosen for book `id`, keeping its other settings and its parsed text.
    pub fn save_mode(&self, id: &str, mode: super::ReadingMode) -> Result<(), String> {
        let mut settings = self.settings(id);
        settings.mode = Some(mode);
        lock(&self.library)
            .set_book_value(&BookId(id.into()), BookKey::Settings, &settings)
            .map_err(|e| e.to_string())
    }

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
    /// Every tag on some item, with how many have it, by name.
    pub fn tags(&self) -> Result<Vec<aoide_library::TagCount>, String> {
        self.shelve(|library| library.tags())
    }

    /// The user's tags for an item: exactly these are shown.
    pub fn set_tags(&self, id: &str, tags: &[String]) -> Result<(), String> {
        self.shelve(|library| library.set_tags(&BookId(id.into()), tags))
    }

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

    /// Forgets every parsed book, so each is read again (how all of them are laid out changed).
    pub fn unload_all(&self) {
        lock(&self.books).clear();
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
    fn layout(&self, id: &str) -> crate::book::Layout {
        let settings = self.book_settings(id);
        crate::book::Layout {
            // A recording is laid onto the book's own text, without footnotes read in between.
            read_notes: settings.read_notes && !self.plays_recording(id, &settings),
            segments: self.app_settings().pauses.segment_options(),
        }
    }

    fn playing(&self) -> Option<String> {
        self.playback.playing_id()
    }

    fn prepare_of(&self, id: &str) -> Option<PrepareView> {
        self.prepare_view(id)
    }

    fn recording_of(&self, id: &str) -> Option<SyncStage> {
        self.recording_stage(id)
    }

    fn narrates_recording(&self, id: &str, settings: &super::BookSettings) -> bool {
        self.plays_recording(id, settings)
    }

    fn article_document(&self, id: &str) -> Result<aoide_document::ParsedBook, String> {
        self.articles.document(id)
    }

    fn article_of(&self, id: &str) -> Option<super::ArticleSummary> {
        self.articles.summary(id)
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

    /// Every tag on some item, with how many items have it.
    pub fn tags(&self) -> Result<Vec<aoide_library::TagCount>, String> {
        self.shelf.tags()
    }

    /// The user's tags for an item: exactly these are shown, the others stay removed.
    pub fn set_book_tags(&self, book_id: &str, tags: &[String]) -> Result<(), String> {
        self.shelf.set_tags(book_id, tags)?;
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
        let kind = self.item_kind(id).unwrap_or(aoide_library::ItemKind::Book);
        if kind == aoide_library::ItemKind::Audiobook {
            return self.audiobook_overview(id);
        }
        let book = self.load_book(id)?;
        let mode = self.mode_of(id);
        Ok(super::BookView {
            id: book.id.clone(),
            kind,
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
            mode,
            shape: reading::shape(true, mode),
        })
    }

    pub fn chapter(&self, id: &str, chapter: usize) -> Result<crate::book::ChapterView, String> {
        let book = self.load_book(id)?;
        let mut view = book
            .chapter_view(chapter)
            .ok_or_else(|| "no such chapter".to_owned())?;
        if !self.says_again(&book.id) {
            for segment in view.blocks.iter_mut().flat_map(|b| &mut b.segments) {
                segment.can_say_again = false;
            }
        }
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
        // An item a feed still lists goes back to the feed shelf, keeping its place.
        if self.unsave(id)? {
            return Ok(());
        }
        if self.playback.is_book(id) {
            self.stop();
        }
        self.plan(super::PlanEvent::BookForgotten(id.into()));
        self.forget_recording(id);
        let files = self.shelf.remove(id)?;
        self.articles.forget(id);
        self.storage.delete_audio(id, files);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }
}

fn is_pdf_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// A book's subjects, from an EPUB's package or a PDF's keywords alone; none when it cannot be
/// read.
fn subjects_of(path: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    if aoide_pdf::is_pdf(&bytes) {
        return aoide_pdf::read_pdf_metadata(&bytes)
            .map(|meta| meta.subjects)
            .unwrap_or_default();
    }
    aoide_epub::read_metadata(&bytes, &aoide_epub::ParseOptions::default())
        .map(|meta| meta.subjects)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use aoide_library::BookStatus;

    use super::super::tests::{epub, Recorder};
    use super::*;

    struct Host;

    impl ShelfHost for Host {
        fn layout(&self, _: &str) -> crate::book::Layout {
            crate::book::Layout::default()
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
        fn narrates_recording(&self, _: &str, _: &super::super::BookSettings) -> bool {
            false
        }
        fn article_document(&self, _: &str) -> Result<aoide_document::ParsedBook, String> {
            Err("no articles in this test".into())
        }
        fn article_of(&self, _: &str) -> Option<super::super::ArticleSummary> {
            None
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

    /// A PDF of the given pages of lines, titled "The Quay Lights", with the keyword "Sea".
    fn quay_pdf(pages: &[&[&str]]) -> Vec<u8> {
        use aoide_pdf::testing::{lines_from, pdf};
        let pages: Vec<_> = pages.iter().map(|lines| lines_from(700.0, lines)).collect();
        pdf(
            &pages,
            &[],
            &[
                ("Title", "The Quay Lights"),
                ("Author", "Mira Vale"),
                ("Keywords", "Sea"),
            ],
        )
    }

    #[test]
    fn a_pdf_is_imported_and_scanned_like_an_epub_and_read_as_reflowed_text() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let outside = dir.path().join("Quay.PDF");
        std::fs::write(&outside, quay_pdf(&[&["The ferry left at dawn."]])).unwrap();
        std::fs::write(
            dir.path().join("books/Scanned.pdf"),
            aoide_pdf::testing::pdf(&[Vec::new()], &[], &[]),
        )
        .unwrap();

        let results = shelf.import_files(vec![outside.to_string_lossy().into_owned()]);
        let id = results[0].book_id.clone().expect("imported");
        assert!(dir.path().join("books/Quay.PDF").is_file());
        shelf.rescan().unwrap();

        let books = shelf.list(&Host).unwrap();
        let quay = books.iter().find(|b| b.id == id).unwrap();
        assert_eq!(quay.title, "The Quay Lights");
        assert_eq!(quay.authors, ["Mira Vale"]);
        assert_eq!(quay.tags, ["Sea"]);
        let book = shelf.load(&id, &Host).unwrap();
        assert!(book.chapters[0].text.contains("The ferry left at dawn."));
        // A scan has no text to read: not listed.
        assert!(!books.iter().any(|b| b.title == "Scanned"));
    }

    #[test]
    fn a_pdf_with_no_text_to_read_is_not_imported() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let outside = dir.path().join("Scan.pdf");
        std::fs::write(&outside, aoide_pdf::testing::pdf(&[Vec::new()], &[], &[])).unwrap();

        let results = shelf.import_files(vec![outside.to_string_lossy().into_owned()]);

        assert_eq!(results[0].book_id, None);
        assert!(results[0].error.as_deref().unwrap().contains("no text"));
        assert!(!dir.path().join("books/Scan.pdf").exists());
        assert!(listed(&shelf).is_empty());
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
    fn a_books_subjects_become_its_tags_and_the_users_edit_holds_over_rescans() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let one = "<html><body><p>The quay lights came on one by one.</p></body></html>";
        std::fs::write(
            dir.path().join("books/Tagged.epub"),
            super::super::tests::epub_with_subjects(
                one.into(),
                one.into(),
                &["Fiction / Sea Stories / General", "Lighthouses"],
            ),
        )
        .unwrap();
        shelf.rescan().unwrap();
        let tags = || shelf.list(&Host).unwrap()[0].tags.clone();
        assert_eq!(tags(), ["Fiction", "Lighthouses", "Sea Stories"]);
        assert_eq!(
            shelf
                .tags()
                .unwrap()
                .iter()
                .map(|t| &t.name)
                .collect::<Vec<_>>(),
            ["Fiction", "Lighthouses", "Sea Stories"]
        );

        let id = shelf.list(&Host).unwrap()[0].id.clone();
        shelf
            .set_tags(&id, &["Lighthouses".into(), "To read".into()])
            .unwrap();
        shelf.rescan().unwrap();
        assert_eq!(tags(), ["Lighthouses", "To read"]);
    }

    #[test]
    fn books_listed_before_tags_get_their_subjects_once() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let one = "<html><body><p>The quay lights came on one by one.</p></body></html>";
        std::fs::write(
            dir.path().join("books/Older.epub"),
            super::super::tests::epub_with_subjects(one.into(), one.into(), &["Lighthouses"]),
        )
        .unwrap();
        shelf.rescan().unwrap();
        let id = shelf.list(&Host).unwrap()[0].id.clone();
        // As a library from before tags: the book without tags, its subjects never read.
        shelf
            .shelve(|library| {
                let id = BookId(id.clone());
                let record = library.book(&id)?.unwrap();
                library.remove_book(&id)?;
                library.upsert_book(
                    &NewBook {
                        id,
                        path: record.path,
                        title: record.title,
                        authors: record.authors,
                        language: record.language,
                        cover_path: record.cover_path,
                        error: None,
                        kind: record.kind,
                    },
                    1,
                )?;
                library.remove_value(GlobalKey::SubjectsRead)
            })
            .unwrap();
        assert!(shelf.list(&Host).unwrap()[0].tags.is_empty());
        shelf.rescan().unwrap();
        assert_eq!(shelf.list(&Host).unwrap()[0].tags, ["Lighthouses"]);
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
