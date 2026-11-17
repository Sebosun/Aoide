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
use super::{leaving_out, reading};
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
    /// Book `id` was parsed to be opened (its text may be indexed from it).
    fn parsed(&self, _id: &str, _book: &Arc<BookModel>) {}
}

/// Who archived an item, or took it out of the archive, for the log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchivedBy {
    /// The user, from a menu or a toast.
    User,
    /// The setting that archives an article once it is finished.
    Finished,
}

impl ArchivedBy {
    fn name(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Finished => "finished",
        }
    }
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
    queued: HashSet<BookId>,
    /// The details corrected by hand.
    details: HashMap<BookId, aoide_library::Result<super::details::DetailsEdit>>,
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
            queued: library.queue()?.into_iter().collect(),
            details: library.book_values(BookKey::Details)?,
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

    /// Keeps item `id` out once removed, as a removed book is: a saved email in the inbox is
    /// not added again.
    pub fn keep_out(&self, id: &str) {
        let mut ignored = self.ignored();
        if ignored.insert(id.into()) {
            self.set_ignored(&ignored);
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
        // Books listed before tags came have their subjects read once; before series, their
        // series (each from one read of the package).
        let flag = |key| matches!(lock(&self.library).value::<bool>(key), Ok(Some(true)));
        let (subjects_read, series_read) =
            (flag(GlobalKey::SubjectsRead), flag(GlobalKey::SeriesRead));
        let mut with_series = 0usize;
        for file in files {
            if ignored.contains(&file.id.0) {
                continue;
            }
            let existing = lock(&self.library)
                .book(&file.id)
                .map_err(|e| e.to_string())?;
            let (book, subjects, series) = match existing {
                Some(record) if !matches!(record.status, BookStatus::Error(_)) => {
                    let package = (!subjects_read || !series_read).then(|| package_of(&file.path));
                    let (file_subjects, file_series) = package.unwrap_or_default();
                    let series = (!series_read).then_some(file_series);
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
                        [file_subjects, looked_up].concat()
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
                    (record, subjects, series)
                }
                existing => {
                    let described = self.describe(&file.path, &file.id);
                    // A file that may hold no text (a scanned PDF) is passed over, not listed,
                    // when it cannot be read.
                    if described.0.error.is_some() && may_hold_no_text(&file.path) {
                        continue;
                    }
                    if existing.is_none() {
                        new += 1;
                    }
                    read += 1;
                    let (record, subjects, series) = described;
                    (record, subjects, Some(series))
                }
            };
            present.push(file.id.clone());
            let library = lock(&self.library);
            library
                .upsert_book(&book, now_ms())
                .map_err(|e| e.to_string())?;
            if let Some(series) = series {
                with_series += usize::from(series.is_some());
                library
                    .set_series(
                        &book.id,
                        series.as_ref().map(|s| (s.name.as_str(), s.position)),
                    )
                    .map_err(|e| e.to_string())?;
            }
            let tags = aoide_library::tidy_tags(&subjects);
            if !tags.is_empty() {
                library
                    .add_tags(&book.id, &tags, aoide_library::TagSource::Metadata)
                    .map_err(|e| e.to_string())?;
            }
        }
        for (read, key, what) in [
            (subjects_read, GlobalKey::SubjectsRead, "subjects"),
            (series_read, GlobalKey::SeriesRead, "series"),
        ] {
            if read {
                continue;
            }
            if let Err(error) = lock(&self.library).set_value(key, &true) {
                self.reporter.report(
                    "library.save",
                    &format!("Could not note that the books' {what} were read"),
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
            with_series,
            took_ms = crate::logging::took(started),
            "Scanned the library folder"
        );
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// Parses a new book's metadata and extracts its cover.
    /// The book's record from its file, its subjects and its series.
    fn describe(
        &self,
        path: &Path,
        id: &BookId,
    ) -> (NewBook, Vec<String>, Option<aoide_epub::Series>) {
        let fallback_title = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        let parsed = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| crate::book::parse_document_named(&bytes, file_name(path).as_deref()))
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
                (record, book.subjects, book.series)
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
                (record, Vec::new(), None)
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
                    let name = file_name(&source);
                    if name.as_deref().and_then(crate::book::Format::of_name)
                        == Some(crate::book::Format::Kindle)
                    {
                        return Err(crate::book::KINDLE.into());
                    }
                    if !aoide_library::is_book_file(&source) {
                        return Err(format!(
                            "not a book file Aoide reads (.{})",
                            aoide_library::BOOK_EXTENSIONS.join(", .")
                        ));
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
            "Added book files"
        );
        for result in results.iter().filter(|r| r.error.is_some()) {
            tracing::warn!(
                event = "library.import",
                file = %crate::logging::file(Path::new(&result.path)),
                format = ?file_name(Path::new(&result.path)).as_deref().and_then(crate::book::Format::of_name),
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
        let edit = match stored.details.remove(&record.id) {
            Some(Ok(edit)) => edit,
            Some(Err(error)) => {
                self.reporter.report(
                    "book.read",
                    "Could not read the book's corrected details",
                    error,
                );
                super::details::DetailsEdit::default()
            }
            None => super::details::DetailsEdit::default(),
        };
        let edited = edit.edited();
        let record = super::details::apply(record, &edit, &self.covers_dir);
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
        let queued = stored.queued.contains(id);
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
            archived: record.archived_at.is_some(),
            queued,
            edited,
            series: record.series.map(|name| super::SeriesView {
                name,
                position: record.series_position,
            }),
            finished_at: record.finished_at,
        }
    }

    /// Book `id` parsed, from the ones kept or read again.
    pub fn load(&self, id: &str, host: &dyn ShelfHost) -> Result<Arc<BookModel>, String> {
        if let Some(book) = lock(&self.books).get(id) {
            return Ok(Arc::clone(book));
        }
        let book = Arc::new(self.parse(id, host)?);
        host.parsed(id, &book);
        let mut books = lock(&self.books);
        if books.len() >= 3 {
            let playing = host.playing();
            books.retain(|k, _| Some(k) == playing.as_ref());
        }
        books.insert(id.into(), Arc::clone(&book));
        Ok(book)
    }

    /// Item `id`'s text, parsed (outside the library lock) as the reader shows it, and not
    /// kept: [`Shelf::load`] keeps it, indexing does not.
    pub fn parse(&self, id: &str, host: &dyn ShelfHost) -> Result<BookModel, String> {
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
        let mut book = if record.kind == aoide_library::ItemKind::Article {
            let document = host.article_document(id)?;
            BookModel::from_parsed(id, document, &host.layout(id))
        } else {
            let bytes = std::fs::read(&record.path).map_err(|e| e.to_string())?;
            let name = file_name(&record.path).unwrap_or_default();
            BookModel::parse_named(id, &bytes, &name, &host.layout(id))?
        };
        // Its title and authors as the library shows them: corrected by hand, else the file's
        // (a PDF with none goes by its file name everywhere).
        let shown = {
            let library = lock(&self.library);
            let edit = super::details::edit_of(&library, &record.id);
            super::details::apply(record.clone(), &edit, &self.covers_dir)
        };
        if let Some(title) = shown.title {
            book.title = title;
        }
        book.authors = shown.authors;
        // Its length, for reading-time estimates in lists that do not parse it.
        let words = u32::try_from(book.word_count()).unwrap_or(u32::MAX);
        if record.word_count != Some(words) {
            if let Err(error) = lock(&self.library).set_word_count(&record.id, words) {
                tracing::warn!(event = "library.word_count", book = crate::logging::book(id), %error, "Could not store the word count");
            }
        }
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

    /// Book `id`'s cover image, if it has one: its own when the user chose one.
    pub fn cover(&self, id: &str) -> Option<String> {
        super::details::shown(&lock(&self.library), &BookId(id.into()), &self.covers_dir)
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

    /// Sets book `id`'s own playback speed (`None`: the usual one), keeping its other
    /// settings; how it is read does not depend on it.
    pub fn set_playback_speed(&self, id: &str, speed: Option<f32>) -> Result<(), String> {
        let book = BookId(id.into());
        let library = lock(&self.library);
        if library.book(&book).map_err(|e| e.to_string())?.is_none() {
            return Err("book not found".into());
        }
        library
            .book_value::<super::BookSettings>(&book, BookKey::Settings)
            .and_then(|stored| {
                let settings = super::BookSettings {
                    playback_speed: speed,
                    ..stored.unwrap_or_default()
                };
                library.set_book_value(&book, BookKey::Settings, &settings)
            })
            .map_err(|e| e.to_string())
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

    /// Archives item `id` (`on`), or takes it out of the archive. Nothing of it is deleted.
    /// Returns whether that changed it.
    pub fn archive(&self, id: &str, on: bool, by: ArchivedBy) -> Result<bool, String> {
        let book = BookId(id.into());
        let changed = {
            let library = lock(&self.library);
            if library.book(&book).map_err(|e| e.to_string())?.is_none() {
                return Err("no such item".into());
            }
            library
                .set_archived(&book, on.then(now_ms))
                .map_err(|e| e.to_string())?
        };
        if changed {
            tracing::info!(
                event = "item.archived",
                book = crate::logging::book(id),
                on,
                from = by.name(),
                "Archive"
            );
        }
        Ok(changed)
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
    fn parsed(&self, id: &str, book: &Arc<BookModel>) {
        self.indexing.offer(id, Arc::clone(book));
    }

    fn layout(&self, id: &str) -> crate::book::Layout {
        let settings = self.book_settings(id);
        // A recording is laid onto the book's own text, without footnotes read in between,
        // and in its narrator's one voice.
        let recorded = self.plays_recording(id, &settings);
        crate::book::Layout {
            read_notes: settings.read_notes && !recorded,
            segments: aoide_text::SegmentOptions {
                split_dialogue: settings.dialogue_voice_id.is_some() && !recorded,
                ..self.app_settings().pauses.segment_options()
            },
            describe: self.app_settings().describe,
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

    /// Its pictures not kept (not asked for, or not got) are said to be left out, as before.
    fn article_document(&self, id: &str) -> Result<aoide_document::ParsedBook, String> {
        use aoide_document::{InsertKind, LeftOut, Warning};
        let mut document = self.articles.document(id)?;
        let mut missing = 0;
        for chapter in &mut document.chapters {
            chapter.inserts.retain(|insert| match &insert.kind {
                InsertKind::Picture { resource, .. } => {
                    let kept = self.pictures.stored(id, resource).is_some();
                    missing += usize::from(!kept);
                    kept
                }
                InsertKind::Table { .. } => true,
            });
        }
        if missing > 0 {
            let counted = document.warnings.iter_mut().find_map(|w| match w {
                Warning::LeftOut {
                    what: LeftOut::Image,
                    count,
                } => Some(count),
                _ => None,
            });
            match counted {
                Some(count) => *count += missing,
                None => document.warnings.push(Warning::LeftOut {
                    what: LeftOut::Image,
                    count: missing,
                }),
            }
        }
        Ok(document)
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

    /// Adds files to the library: books are copied into its folder, saved emails (`.eml`)
    /// become articles. Results are in the order of `paths`.
    pub fn import_files(&self, paths: Vec<String>) -> Vec<ImportResult> {
        let is_email = |path: &String| super::articles::is_email_file(std::path::Path::new(path));
        let books: Vec<String> = paths.iter().filter(|p| !is_email(p)).cloned().collect();
        // An email alone does not rescan the library folder.
        let mut books = match books.is_empty() {
            true => Vec::new().into_iter(),
            false => self.shelf.import_files(books).into_iter(),
        };
        let mut emailed = false;
        let results = paths
            .into_iter()
            .map(|path| {
                if !is_email(&path) {
                    return books.next().expect("one result per book");
                }
                let made = self
                    .articles
                    .import_email(std::path::Path::new(&path), super::now_ms());
                if let Ok(id) = &made {
                    // Added by hand, a removed email comes back.
                    self.shelf.restore([id.as_str()]);
                    emailed = true;
                }
                match made {
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
        if emailed {
            self.emit(super::CoreEvent::LibraryChanged);
        }
        results
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

    /// Deletes collection `id`; a feed whose entries were saved into it is left with none.
    pub fn delete_collection(&self, id: i64) -> Result<(), String> {
        let feeds_use_it = self
            .feeds()
            .is_ok_and(|feeds| feeds.iter().any(|f| f.collection_id == Some(id)));
        self.shelf.shelve(|library| library.delete_collection(id))?;
        self.emit(CoreEvent::LibraryChanged);
        if feeds_use_it {
            self.emit(CoreEvent::FeedsChanged);
        }
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

    /// Archives item `id`, or takes it out of the archive: it leaves Home and the library's
    /// default views, and nothing of it is deleted.
    pub fn archive(&self, id: &str, on: bool) -> Result<(), String> {
        if self.shelf.archive(id, on, ArchivedBy::User)? {
            self.emit(CoreEvent::LibraryChanged);
        }
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
        let left_out = self.book_settings(id).left_out;
        // The sentence the place is in now: its id moves when the book is segmented anew.
        let progress = self.shelf.progress(&BookId(id.into())).map(|mut view| {
            if let Some(index) = self.saved_index(&book) {
                view.segment_id = book.segment(index).map(|s| s.id.clone());
            }
            view
        });
        Ok(super::BookView {
            id: book.id.clone(),
            kind,
            title: book.title.clone(),
            authors: book.authors.clone(),
            language: book.language.clone(),
            chapters: leaving_out::chapters(&book, &left_out),
            narrated_word_count: leaving_out::narrated_words(&book, &left_out),
            chapters_left_out: leaving_out::within(&book, &left_out).len(),
            warnings: book.warnings.clone(),
            excluded: book.excluded.clone(),
            language_warning: self.language_warning(book.language.as_deref()),
            voice_warning: self.voice_warning(&book),
            progress,
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
        view.inserts = book.insert_views(chapter, &|resources| self.picture_files(id, resources));
        view.not_narrated = self.not_narrated(book, chapter);
        Ok(view)
    }
}

impl super::Core {
    pub fn book_settings(&self, id: &str) -> super::BookSettings {
        self.shelf.settings(id)
    }

    /// The files of book `id`'s pictures at `resources`, copied out of its EPUB when first
    /// asked for (read once, only if one is needed).
    fn picture_files(&self, id: &str, resources: &[&str]) -> Vec<Option<std::path::PathBuf>> {
        if resources.is_empty() {
            return Vec::new();
        }
        let record = lock(&self.library).book(&BookId(id.into())).ok().flatten();
        // An article's pictures are fetched when it is saved, not now.
        if record
            .as_ref()
            .is_some_and(|r| r.kind == aoide_library::ItemKind::Article)
        {
            return resources
                .iter()
                .map(|resource| self.pictures.stored(id, resource))
                .collect();
        }
        let source = record.filter(|r| r.kind == aoide_library::ItemKind::Book);
        let bytes = std::sync::OnceLock::new();
        let read = |resource: &str| -> Result<Vec<u8>, String> {
            let path = &source.as_ref().ok_or("no book file")?.path;
            let bytes = bytes.get_or_init(|| std::fs::read(path).map_err(|e| e.to_string()));
            let bytes = bytes.as_ref().map_err(Clone::clone)?;
            aoide_epub::read_resource(bytes, resource, &aoide_epub::ParseOptions::default())
                .map_err(|e| e.to_string())
        };
        self.pictures.files(id, resources, &read)
    }

    /// Saves book `id`'s settings, keeping the chapters it leaves out and its own playback
    /// speed: those change only through [`super::Core::set_left_out`] and the speed's own
    /// commands, so a form holding older settings cannot undo them.
    pub async fn save_book_settings(
        &self,
        id: &str,
        settings: super::BookSettings,
    ) -> Result<(), String> {
        let stored = self.book_settings(id);
        let settings = super::BookSettings {
            left_out: stored.left_out,
            playback_speed: stored.playback_speed,
            ..settings
        };
        self.store_book_settings(id, settings).await
    }

    /// Saves book `id`'s settings as they are, and plays and plans by them.
    pub(super) async fn store_book_settings(
        &self,
        id: &str,
        settings: super::BookSettings,
    ) -> Result<(), String> {
        self.engine.check_params(&settings.params)?;
        let split_before = self.book_settings(id).dialogue_voice_id.is_some();
        self.shelf.save_settings(id, &settings)?;
        if settings.dialogue_voice_id.is_some() != split_before {
            self.dialogue_changed(id, settings.dialogue_voice_id.is_some());
        }
        // Whether its recording narrates it may have changed.
        self.emit_recording(id);
        if self.playback.is_book(id) {
            self.restart_session().await?;
        }
        self.plan(super::PlanEvent::BookSettingsChanged(id.into()));
        Ok(())
    }

    /// Book `id` has a second voice for dialogue now, or no longer: it is segmented anew, so
    /// its sentence ids moved (places keep their character offsets) and its text is indexed
    /// again.
    fn dialogue_changed(&self, id: &str, on: bool) {
        self.indexing.changed(id);
        let counted = self.load_book(id).map(|book| {
            let segments = book.chapters.iter().flat_map(|c| &c.segments);
            let dialogue = segments
                .clone()
                .filter(|s| s.role == aoide_text::Role::Dialogue)
                .count();
            (segments.count(), dialogue)
        });
        match counted {
            Ok((segments, dialogue)) => tracing::info!(
                event = "voicing.dialogue",
                book = crate::logging::book(id),
                outcome = if on { "on" } else { "off" },
                segments,
                dialogue_segments = dialogue,
                "A second voice for dialogue"
            ),
            Err(error) => {
                tracing::warn!(event = "voicing.dialogue", book = crate::logging::book(id), outcome = "failed", %error, "Could not segment the book anew")
            }
        }
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
        let emailed = self
            .articles
            .view(id)
            .is_ok_and(|view| view.via == Some(super::ArticleSource::Email));
        let files = self.shelf.remove(id)?;
        if emailed {
            self.shelf.keep_out(id);
        }
        self.articles.forget(id);
        self.pictures.forget(id);
        self.storage.delete_audio(id, files);
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }
}

fn file_name(path: &Path) -> Option<String> {
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Whether the file's format may hold no text at all (see [`crate::book::Format`]).
fn may_hold_no_text(path: &Path) -> bool {
    file_name(path)
        .as_deref()
        .and_then(crate::book::Format::of_name)
        .is_some_and(crate::book::Format::may_hold_no_text)
}

/// A book's subjects and series, from one read of an EPUB's package (or a PDF's keywords,
/// which name no series); none when it cannot be read.
fn package_of(path: &Path) -> (Vec<String>, Option<aoide_epub::Series>) {
    let Ok(bytes) = std::fs::read(path) else {
        return Default::default();
    };
    use crate::book::Format;
    match crate::book::format_of(&bytes, file_name(path).as_deref()) {
        Format::Pdf => {
            let subjects = aoide_pdf::read_pdf_metadata(&bytes)
                .map(|meta| meta.subjects)
                .unwrap_or_default();
            (subjects, None)
        }
        Format::Epub => aoide_epub::read_metadata(&bytes, &aoide_epub::ParseOptions::default())
            .map(|meta| (meta.subjects, meta.series))
            .unwrap_or_default(),
        // Read whole: only small formats say these.
        Format::Fb2 | Format::Docx => {
            crate::book::parse_document_named(&bytes, file_name(path).as_deref())
                .map(|book| (book.subjects, book.series))
                .unwrap_or_default()
        }
        _ => Default::default(),
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
    fn text_markdown_and_html_files_are_imported_and_read_like_books() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let files = [
            (
                "Lamp.txt",
                "CHAPTER I\n\nThe keeper climbed the stair.\n\nCHAPTER II\n\nMorning came.\n",
            ),
            (
                "Essay.md",
                "---\ntitle: On Lamps\nauthor: Mara Quill\n---\n\n# On Lamps\n\nTrim the *wick* often.\n",
            ),
            (
                "Page.html",
                "<html><head><title>The Quay</title></head><body><article><h1>The Quay</h1><p>The ferry left at dawn and the gulls followed it out past the breakwater, calling.</p><p>By noon the quay was quiet again, the nets drying on the wall in the sun.</p></article></body></html>",
            ),
        ];
        let paths: Vec<String> = files
            .iter()
            .map(|(name, text)| {
                let path = dir.path().join(name);
                std::fs::write(&path, text).unwrap();
                path.to_string_lossy().into_owned()
            })
            .collect();
        let results = shelf.import_files(paths);
        assert!(results.iter().all(|r| r.error.is_none()), "{results:?}");
        shelf.rescan().unwrap();
        let books = shelf.list(&Host).unwrap();
        let titles: std::collections::BTreeSet<&str> =
            books.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, ["Lamp", "On Lamps", "The Quay"].into());
        let read = |title: &str| {
            let id = &books.iter().find(|b| b.title == title).unwrap().id;
            shelf.load(id, &Host).unwrap()
        };
        let lamp = read("Lamp");
        assert_eq!(lamp.chapters.len(), 2);
        assert!(lamp.chapters[0]
            .text
            .contains("The keeper climbed the stair."));
        assert!(read("On Lamps").chapters[0]
            .text
            .contains("Trim the wick often."));
        assert!(read("The Quay").chapters[0]
            .text
            .contains("The ferry left at dawn"));
        let essay = books.iter().find(|b| b.title == "On Lamps").unwrap();
        assert_eq!(essay.authors, ["Mara Quill"]);
    }

    #[test]
    fn a_kindle_book_is_refused_with_a_way_to_read_it() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let outside = dir.path().join("Lamp.azw3");
        let mut bytes = vec![0u8; 80];
        bytes[60..68].copy_from_slice(b"BOOKMOBI");
        std::fs::write(&outside, bytes).unwrap();
        let results = shelf.import_files(vec![outside.to_string_lossy().into_owned()]);
        assert_eq!(results[0].book_id, None);
        assert!(
            results[0].error.as_deref().unwrap().contains("Calibre"),
            "{results:?}"
        );
        let other = dir.path().join("Notes.odt");
        std::fs::write(&other, b"x").unwrap();
        let results = shelf.import_files(vec![other.to_string_lossy().into_owned()]);
        assert!(
            results[0].error.as_deref().unwrap().contains(".txt"),
            "{results:?}"
        );
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
    fn a_books_series_is_read_when_it_is_added_and_once_for_books_listed_before() {
        let dir = tempfile::tempdir().unwrap();
        let shelf = shelf(dir.path());
        let in_series = super::super::tests::epub_in_series;
        std::fs::write(
            dir.path().join("books/one.epub"),
            in_series("The First Light", "Harbour Lights", "1"),
        )
        .unwrap();
        shelf.rescan().unwrap();
        let series = |shelf: &Shelf| {
            let mut found: Vec<(String, Option<String>, Option<f32>)> = shelf
                .shelve(|library| library.books())
                .unwrap()
                .into_iter()
                .map(|r| (r.title.unwrap(), r.series, r.series_position))
                .collect();
            found.sort_by(|a, b| a.0.cmp(&b.0));
            found
        };
        assert_eq!(
            series(&shelf),
            [(
                "The First Light".into(),
                Some("Harbour Lights".into()),
                Some(1.0)
            )]
        );
        // As a library from before series: the book stored without one, never read for it.
        shelf
            .shelve(|library| {
                let id = library.books()?[0].id.clone();
                library.set_series(&id, None)?;
                library.remove_value(GlobalKey::SeriesRead)
            })
            .unwrap();
        std::fs::write(
            dir.path().join("books/two.epub"),
            in_series("The Second Light", "Harbour Lights", "2.5"),
        )
        .unwrap();
        shelf.rescan().unwrap();
        assert_eq!(
            series(&shelf),
            [
                (
                    "The First Light".into(),
                    Some("Harbour Lights".into()),
                    Some(1.0)
                ),
                (
                    "The Second Light".into(),
                    Some("Harbour Lights".into()),
                    Some(2.5)
                ),
            ]
        );
        // Read once: a series taken off afterwards is not put back by the next scan.
        shelf
            .shelve(|library| {
                let id = library.books()?[0].id.clone();
                library.set_series(&id, None)
            })
            .unwrap();
        shelf.rescan().unwrap();
        assert_eq!(series(&shelf).iter().filter(|s| s.1.is_none()).count(), 1);
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
