//! "Find books on this computer": scan folders for EPUBs, show the ones not in the library,
//! fetch their details on request, and import the chosen ones into the library folder.
//!
//! Scans are incremental: a file whose size, modification time and inode are unchanged since
//! the last scan is not read again. Source files are only ever read.

use super::*;

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

use aoide_library::DiscoveredFile;
use aoide_metadata::{Match, Query};
use rayon::prelude::*;

/// Threads that read and hash candidate files (disk-bound, so few).
const READ_THREADS: usize = 4;
/// Pause between Open Library requests (about 3 per second).
const LOOKUP_GAP: Duration = Duration::from_millis(350);

/// Which folders a scan covers. Empty `roots` means the home folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ScanSettings {
    pub roots: Vec<String>,
    pub exclude: Vec<String>,
    /// Hidden folders to search anyway.
    pub include_hidden: Vec<String>,
    /// Look for new books when Aoide starts.
    pub on_startup: bool,
}

/// Where a "Find books" scan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ScanStage {
    Walking,
    Reading,
    Done,
    Cancelled,
    Error,
}

/// How a found EPUB relates to the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum CandidateStatus {
    New,
    InLibrary,
    Removed,
    Unreadable,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ScanProgress {
    pub stage: ScanStage,
    pub dirs: u64,
    pub files: u64,
    pub found: u64,
    /// Files read so far / to read in this scan (new or changed files only).
    pub read: u64,
    pub to_read: u64,
    pub error: Option<String>,
}

/// One book found on disk (all copies of the same file grouped together).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Candidate {
    /// The content hash (the book id it would get), or `path:<path>` when unreadable.
    pub id: String,
    pub status: CandidateStatus,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub cover_path: Option<String>,
    pub paths: Vec<String>,
    pub size: u64,
    pub error: Option<String>,
    /// Best cached Open Library match, if details were fetched.
    pub suggestion: Option<Match>,
    /// The audiobook kept beside it (audio files, or one folder), unless the book already
    /// has a recording.
    pub audiobook: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ImportChoice {
    pub id: String,
    /// Apply the suggested Open Library match after importing.
    pub apply_suggestion: bool,
    /// Pair the audiobook found beside it after importing.
    #[serde(default)]
    pub pair_audiobook: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "DiscoveryImportProgress"))]
pub struct ImportProgress {
    pub done: usize,
    pub total: usize,
    pub current: Option<String>,
}

/// Groups remembered files by content and classifies them against the library.
pub fn group_candidates(
    files: &[DiscoveredFile],
    in_library: &HashSet<String>,
    removed: &HashSet<String>,
) -> Vec<Candidate> {
    let mut by_hash: BTreeMap<String, Vec<&DiscoveredFile>> = BTreeMap::new();
    let mut result = Vec::new();
    for file in files {
        match &file.hash {
            Some(hash) if file.error.is_none() => {
                by_hash.entry(hash.clone()).or_default().push(file)
            }
            _ => result.push(Candidate {
                id: format!("path:{}", file.path.display()),
                status: CandidateStatus::Unreadable,
                title: file
                    .path
                    .file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
                authors: Vec::new(),
                language: None,
                isbn: None,
                cover_path: None,
                paths: vec![file.path.to_string_lossy().into_owned()],
                size: file.size,
                error: file
                    .error
                    .clone()
                    .or_else(|| Some("could not be read".into())),
                suggestion: None,
                audiobook: Vec::new(),
            }),
        }
    }
    for (hash, copies) in by_hash {
        let first = copies[0];
        let status = if in_library.contains(&hash) {
            CandidateStatus::InLibrary
        } else if removed.contains(&hash) {
            CandidateStatus::Removed
        } else {
            CandidateStatus::New
        };
        result.push(Candidate {
            title: first.title.clone().unwrap_or_else(|| {
                first
                    .path
                    .file_stem()
                    .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
            }),
            authors: first.authors.clone(),
            language: first.language.clone(),
            isbn: first.isbn.clone(),
            cover_path: copies
                .iter()
                .find_map(|c| c.cover_path.as_ref())
                .map(|p| p.to_string_lossy().into_owned()),
            paths: copies
                .iter()
                .map(|c| c.path.to_string_lossy().into_owned())
                .collect(),
            size: first.size,
            error: None,
            suggestion: None,
            audiobook: Vec::new(),
            id: hash,
            status,
        });
    }
    result.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    result
}

/// A file name in `dir` that does not exist yet: `name.epub`, `name (2).epub`, …
pub fn unique_target(dir: &Path, file_name: &str) -> PathBuf {
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .map_or_else(|| "book".into(), |s| s.to_string_lossy().into_owned());
    let candidate = dir.join(format!("{stem}.epub"));
    if !candidate.exists() {
        return candidate;
    }
    (2..)
        .map(|n| dir.join(format!("{stem} ({n}).epub")))
        .find(|p| !p.exists())
        .expect("unbounded")
}

fn query_for(candidate: &Candidate) -> Query {
    Query {
        isbn: candidate.isbn.clone(),
        title: candidate.title.clone(),
        authors: candidate.authors.clone(),
    }
}

/// What finding books asks of the rest of the app.
pub(super) trait DiscoveryHost: Sync {
    /// Books the reader removed from the library.
    fn removed(&self) -> HashSet<String>;
    /// The best Open Library match already looked up for `query`.
    fn cached_match(&self, query: &Query) -> Option<Match>;
    /// Looks `query` up on Open Library.
    fn look_up<'a>(
        &'a self,
        query: &'a Query,
    ) -> aoide_tts::BoxFuture<'a, Result<Vec<Match>, String>>;
    /// Books `ids` were copied into the library folder: they join the library.
    fn imported(&self, ids: Vec<String>) -> aoide_tts::BoxFuture<'_, Result<(), String>>;
    /// Pairs book `id` with its audiobook's files.
    fn pair(&self, id: &str, audiobook: Vec<String>) -> Result<(), String>;
    /// Stores `picked` as book `id`'s details.
    fn apply<'a>(
        &'a self,
        id: &'a str,
        picked: Match,
    ) -> aoide_tts::BoxFuture<'a, Result<(), String>>;
}

/// "Find books": scans folders for EPUBs, compares them with the library and copies the
/// chosen ones in.
pub(super) struct Discovery {
    library: Arc<Mutex<Library>>,
    events: Arc<dyn Events>,
    reporter: error::Reporter,
    /// Where a scan starts when no folders are set.
    home_dir: PathBuf,
    library_dir: PathBuf,
    covers_dir: PathBuf,
    /// Aoide's own folders, never scanned.
    private_dirs: Vec<PathBuf>,
    /// Set while a scan runs; storing `true` cancels it.
    scanning: Mutex<Option<Arc<AtomicBool>>>,
}

impl Discovery {
    pub fn new(
        library: Arc<Mutex<Library>>,
        paths: &Paths,
        events: Arc<dyn Events>,
        reporter: error::Reporter,
    ) -> Self {
        let mut private_dirs = vec![paths.library_dir.clone(), paths.audio_dir.clone()];
        private_dirs.extend(paths.db_path.parent().map(Path::to_path_buf));
        Self {
            library,
            events,
            reporter,
            home_dir: paths.home_dir.clone(),
            library_dir: paths.library_dir.clone(),
            covers_dir: paths.covers_dir.clone(),
            private_dirs,
            scanning: Mutex::new(None),
        }
    }

    fn scan_options(&self, settings: ScanSettings) -> aoide_discovery::ScanOptions {
        let roots: Vec<PathBuf> = if settings.roots.is_empty() {
            vec![self.home_dir.clone()]
        } else {
            settings.roots.iter().map(PathBuf::from).collect()
        };
        let mut exclude: Vec<PathBuf> = settings.exclude.iter().map(PathBuf::from).collect();
        exclude.extend(self.private_dirs.iter().cloned());
        aoide_discovery::ScanOptions {
            roots,
            exclude,
            include_hidden: settings.include_hidden.iter().map(PathBuf::from).collect(),
            ..Default::default()
        }
    }

    fn emit_scan(&self, progress: ScanProgress) {
        self.emit(CoreEvent::ScanProgress(progress));
    }

    fn emit(&self, event: CoreEvent) {
        match event.payload() {
            Ok(value) => self.events.emit(event.name(), value),
            Err(error) => {
                tracing::error!(event = "app.emit", name = event.name(), %error, "Cannot send an event to the window")
            }
        }
    }

    /// Scans the configured folders. Returns the candidates afterwards.
    pub async fn scan(
        self: &Arc<Self>,
        settings: ScanSettings,
        host: &dyn DiscoveryHost,
    ) -> Result<Vec<Candidate>, String> {
        let cancel = {
            let mut scanning = lock(&self.scanning);
            if scanning.is_some() {
                return Err("a scan is already running".into());
            }
            Arc::clone(scanning.insert(Arc::new(AtomicBool::new(false))))
        };
        let this = Arc::clone(self);
        let result = tokio::task::spawn_blocking(move || this.scan_blocking(&cancel, settings))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        *lock(&self.scanning) = None;
        if let Err(error) = &result {
            self.emit_scan(ScanProgress {
                stage: if error == super::error::CANCELLED {
                    ScanStage::Cancelled
                } else {
                    ScanStage::Error
                },
                dirs: 0,
                files: 0,
                found: 0,
                read: 0,
                to_read: 0,
                error: Some(error.clone()),
            });
        }
        result?;
        self.candidates(host)
    }

    pub fn cancel(&self) {
        if let Some(cancel) = lock(&self.scanning).as_ref() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn scan_blocking(&self, cancel: &AtomicBool, settings: ScanSettings) -> Result<(), String> {
        let cancelled = || cancel.load(std::sync::atomic::Ordering::Relaxed);
        let options = self.scan_options(settings);
        let started = Instant::now();
        let found = self.walk(&options, cancel);
        if cancelled() {
            return Err(super::error::CANCELLED.into());
        }
        let scan_started = now_ms();
        let (unchanged, changed) = self.set_aside_unchanged(found, scan_started)?;
        let records = self.read_files(&changed, scan_started, cancel);
        if cancelled() {
            return Err(super::error::CANCELLED.into());
        }
        self.store(&records, scan_started)?;
        let found = (unchanged + records.len()) as u64;
        let read = changed.len() as u64;
        tracing::info!(
            event = "discovery.scan",
            outcome = "ok",
            roots = options.roots.len(),
            epubs = found,
            read,
            took_ms = crate::logging::took(started),
            "Searched for EPUBs"
        );
        self.emit_scan(ScanProgress {
            stage: ScanStage::Done,
            dirs: 0,
            files: 0,
            found,
            read,
            to_read: read,
            error: None,
        });
        Ok(())
    }

    /// Walks the folders for EPUBs, telling the window how far it got every 150 ms.
    fn walk(
        &self,
        options: &aoide_discovery::ScanOptions,
        cancel: &AtomicBool,
    ) -> Vec<aoide_discovery::Found> {
        let last_emit = Mutex::new(Instant::now());
        aoide_discovery::find_epubs(options, cancel, &|p| {
            let mut last = lock(&last_emit);
            if last.elapsed() >= Duration::from_millis(150) {
                *last = Instant::now();
                self.emit_scan(ScanProgress {
                    stage: ScanStage::Walking,
                    dirs: p.dirs,
                    files: p.files,
                    found: p.found,
                    read: 0,
                    to_read: 0,
                    error: None,
                });
            }
        })
    }

    /// Files unchanged since the last scan keep what was learned then (they are only marked
    /// seen); returns how many there were, and the files to read again.
    fn set_aside_unchanged(
        &self,
        found: Vec<aoide_discovery::Found>,
        seen_at: i64,
    ) -> Result<(usize, Vec<aoide_discovery::Found>), String> {
        let library = lock(&self.library);
        let (unchanged, changed): (Vec<_>, Vec<_>) = found.into_iter().partition(|f| {
            library.discovered(&f.path).ok().flatten().is_some_and(|d| {
                d.size == f.size && d.modified_ns == f.modified_ns && d.inode == f.inode
            })
        });
        library
            .mark_discovered_seen(
                &unchanged.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
                seen_at,
            )
            .map_err(|e| e.to_string())?;
        Ok((unchanged.len(), changed))
    }

    /// Reads each file's details and cover, a few at a time, telling the window every eighth.
    fn read_files(
        &self,
        files: &[aoide_discovery::Found],
        seen_at: i64,
        cancel: &AtomicBool,
    ) -> Vec<DiscoveredFile> {
        let total = files.len() as u64;
        let read = std::sync::atomic::AtomicU64::new(0);
        let covers = self.covers_dir.join("discovered");
        let _ = std::fs::create_dir_all(&covers);
        let process = |f: &aoide_discovery::Found| {
            let record = describe_file(f, &covers, seen_at);
            let n = read.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n & 7 == 0 || n == total {
                self.emit_scan(ScanProgress {
                    stage: ScanStage::Reading,
                    dirs: 0,
                    files: 0,
                    found: 0,
                    read: n,
                    to_read: total,
                    error: None,
                });
            }
            record
        };
        match rayon::ThreadPoolBuilder::new()
            .num_threads(READ_THREADS)
            .build()
        {
            Ok(pool) => pool.install(|| {
                files
                    .par_iter()
                    .filter(|_| !cancel.load(std::sync::atomic::Ordering::Relaxed))
                    .map(process)
                    .collect()
            }),
            Err(_) => files.iter().map(process).collect(),
        }
    }

    /// Stores what was read, and forgets files not seen in this scan.
    fn store(&self, records: &[DiscoveredFile], seen_at: i64) -> Result<(), String> {
        let library = lock(&self.library);
        library
            .upsert_discovered(records)
            .map_err(|e| e.to_string())?;
        let forgot = library.forget_discovered_before(seen_at);
        drop(library);
        if let Err(error) = forgot {
            self.reporter.report(
                "discovery.save",
                "Could not forget books no longer found",
                error,
            );
        }
        Ok(())
    }

    /// What the last scan found, grouped and compared with the library.
    pub fn candidates(&self, host: &dyn DiscoveryHost) -> Result<Vec<Candidate>, String> {
        let files = lock(&self.library)
            .discovered_files()
            .map_err(|e| e.to_string())?;
        let removed = host.removed();
        let in_library: HashSet<String> = lock(&self.library)
            .books()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|b| b.id.0)
            .filter(|id| !removed.contains(id))
            .collect();
        let paired: HashSet<String> = lock(&self.library)
            .recordings()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| r.book_id.0)
            .collect();
        let mut candidates = group_candidates(&files, &in_library, &removed);
        let mut beside = aoide_discovery::Neighbourhood::default();
        for candidate in &mut candidates {
            if candidate.status == CandidateStatus::New {
                candidate.suggestion = host.cached_match(&query_for(candidate));
            }
            if candidate.status != CandidateStatus::Unreadable && !paired.contains(&candidate.id) {
                candidate.audiobook = candidate
                    .paths
                    .iter()
                    .map(|p| {
                        beside.audiobook(Path::new(p), Some(&candidate.title), &candidate.authors)
                    })
                    .find(|found| !found.is_empty())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
            }
        }
        Ok(candidates)
    }

    /// Looks the given candidates up on Open Library, one at a time (cached for 30 days).
    /// Emits `discovery://details` with each candidate id as its lookup finishes.
    pub async fn fetch_details(
        &self,
        ids: Vec<String>,
        host: &dyn DiscoveryHost,
    ) -> Result<(), String> {
        let candidates = self.candidates(host)?;
        for id in ids {
            let Some(candidate) = candidates.iter().find(|c| c.id == id) else {
                continue;
            };
            if candidate.suggestion.is_some() || candidate.title.trim().is_empty() {
                self.emit(CoreEvent::DiscoveryDetails(id.clone()));
                continue;
            }
            match host.look_up(&query_for(candidate)).await {
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(event = "metadata.lookup", book = crate::logging::book(&id), outcome = "failed", %error, "Online details lookup failed")
                }
            }
            self.emit(CoreEvent::DiscoveryDetails(id.clone()));
            tokio::time::sleep(LOOKUP_GAP).await;
        }
        Ok(())
    }

    /// Copies the chosen books into the library folder (reflink when the filesystem supports
    /// it) and optionally applies their suggested Open Library match. Returns per-book results.
    pub async fn import(
        &self,
        choices: Vec<ImportChoice>,
        host: &dyn DiscoveryHost,
    ) -> Result<Vec<ImportResult>, String> {
        let candidates = self.candidates(host)?;
        let total = choices.len();
        let mut results = Vec::with_capacity(total);
        let mut imported = Vec::new();
        for (done, choice) in choices.iter().enumerate() {
            let candidate = candidates.iter().find(|c| c.id == choice.id);
            self.emit(CoreEvent::DiscoveryImport(ImportProgress {
                done,
                total,
                current: candidate.map(|c| c.title.clone()),
            }));
            let outcome = match candidate {
                None => Err("no longer found; scan again".to_owned()),
                Some(c) if c.status == CandidateStatus::Unreadable => {
                    Err("not a readable EPUB".to_owned())
                }
                Some(c) if c.status == CandidateStatus::InLibrary => Ok(c.paths[0].clone()),
                Some(c) => self.copy_into_library(c).await.map(|_| c.paths[0].clone()),
            };
            match outcome {
                Ok(path) => {
                    if let Some(c) = candidate {
                        imported.push((c.clone(), choice.clone()));
                    }
                    results.push(ImportResult {
                        path,
                        book_id: Some(choice.id.clone()),
                        error: None,
                    });
                }
                Err(error) => results.push(ImportResult {
                    path: candidate.map_or_else(String::new, |c| c.paths[0].clone()),
                    book_id: None,
                    error: Some(error),
                }),
            }
        }
        host.imported(imported.iter().map(|(c, _)| c.id.clone()).collect())
            .await?;
        for (candidate, choice) in &imported {
            if !choice.pair_audiobook || candidate.audiobook.is_empty() {
                continue;
            }
            if let Err(error) = host.pair(&candidate.id, candidate.audiobook.clone()) {
                results.push(ImportResult {
                    path: candidate.audiobook[0].clone(),
                    book_id: Some(candidate.id.clone()),
                    error: Some(format!(
                        "imported, but its audiobook was not paired: {error}"
                    )),
                });
            }
        }
        for (candidate, choice) in imported {
            if let (true, Some(suggestion)) = (choice.apply_suggestion, candidate.suggestion) {
                if let Err(error) = host.apply(&candidate.id, suggestion).await {
                    tracing::warn!(
                        event = "metadata.apply",
                        book = crate::logging::book(&candidate.id),
                        %error,
                        "Could not apply online details"
                    );
                }
            }
        }
        self.emit(CoreEvent::DiscoveryImport(ImportProgress {
            done: total,
            total,
            current: None,
        }));
        tracing::info!(
            event = "discovery.import",
            chosen = total,
            failed = results.iter().filter(|r| r.error.is_some()).count(),
            "Imported found books"
        );
        Ok(results)
    }

    async fn copy_into_library(&self, candidate: &Candidate) -> Result<(), String> {
        let source = candidate
            .paths
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or("the file is gone; scan again")?;
        let library_dir = self.library_dir.clone();
        let id = candidate.id.clone();
        tokio::task::spawn_blocking(move || {
            let name = source
                .file_name()
                .map_or_else(|| "book.epub".into(), |n| n.to_string_lossy().into_owned());
            let target = unique_target(&library_dir, &name);
            // Copy under a hidden name first so the folder watcher never sees a partial file.
            let partial = library_dir.join(format!(".{id}.partial"));
            reflink_copy::reflink_or_copy(&source, &partial)
                .map_err(|e| format!("could not copy: {e}"))?;
            let copied = aoide_discovery::hash_file(&partial).map_err(|e| e.to_string())?;
            if copied != id {
                let _ = std::fs::remove_file(&partial);
                return Err("the file changed while copying; scan again".into());
            }
            std::fs::rename(&partial, &target).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

impl Core {
    /// Scans the configured folders. Returns the candidates afterwards.
    pub async fn scan_for_books(&self) -> Result<Vec<Candidate>, String> {
        self.discovery.scan(self.app_settings().scan, self).await
    }

    pub fn cancel_scan(&self) {
        self.discovery.cancel()
    }

    /// What the last scan found, grouped and compared with the library.
    pub fn discovered_books(&self) -> Result<Vec<Candidate>, String> {
        self.discovery.candidates(self)
    }

    /// Looks the given candidates up on Open Library, one at a time (cached for 30 days).
    pub async fn fetch_discovered_details(&self, ids: Vec<String>) -> Result<(), String> {
        if !self.app_settings().metadata_lookup {
            return Err("Online lookups are turned off in Settings.".into());
        }
        self.discovery.fetch_details(ids, self).await
    }

    /// Copies the chosen books into the library folder and returns per-book results.
    pub async fn import_discovered(
        &self,
        choices: Vec<ImportChoice>,
    ) -> Result<Vec<ImportResult>, String> {
        self.discovery.import(choices, self).await
    }
}

impl DiscoveryHost for Core {
    fn removed(&self) -> HashSet<String> {
        self.shelf.ignored()
    }

    fn cached_match(&self, query: &Query) -> Option<Match> {
        self.metadata.cached_match(query)
    }

    fn look_up<'a>(
        &'a self,
        query: &'a Query,
    ) -> aoide_tts::BoxFuture<'a, Result<Vec<Match>, String>> {
        Box::pin(self.metadata.search(query, false))
    }

    fn imported(&self, ids: Vec<String>) -> aoide_tts::BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            // Re-imported books are no longer "removed".
            self.shelf.restore(ids.iter().map(String::as_str));
            let shelf = Arc::clone(&self.shelf);
            tokio::task::spawn_blocking(move || shelf.rescan())
                .await
                .map_err(|e| e.to_string())?
        })
    }

    fn pair(&self, id: &str, audiobook: Vec<String>) -> Result<(), String> {
        self.attach_recording(id, audiobook).map(drop)
    }

    fn apply<'a>(
        &'a self,
        id: &'a str,
        picked: Match,
    ) -> aoide_tts::BoxFuture<'a, Result<(), String>> {
        Box::pin(async move { self.apply_metadata(id, picked).await.map(drop) })
    }
}

/// Reads, hashes and describes one found file; never fails (errors are recorded).
fn describe_file(found: &aoide_discovery::Found, covers: &Path, seen_at: i64) -> DiscoveredFile {
    let mut record = DiscoveredFile {
        path: found.path.clone(),
        size: found.size,
        modified_ns: found.modified_ns,
        inode: found.inode,
        hash: None,
        title: None,
        authors: Vec::new(),
        language: None,
        isbn: None,
        cover_path: None,
        error: None,
        seen_at,
    };
    let bytes = match std::fs::read(&found.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            record.error = Some(format!("could not read: {error}"));
            return record;
        }
    };
    let hash = content_hash(&bytes).0;
    match aoide_epub::read_metadata(&bytes, &aoide_epub::ParseOptions::default()) {
        Ok(meta) => {
            if let Some(cover) = &meta.cover {
                record.cover_path = super::item::store_cover(
                    covers,
                    &hash,
                    &cover.media_type,
                    &cover.bytes,
                    super::item::Untyped::Jpeg,
                );
            }
            record.title = meta.title;
            record.authors = meta.authors;
            record.language = meta.language;
            record.isbn = meta.isbn;
        }
        Err(error) => record.error = Some(format!("not a readable EPUB: {error}")),
    }
    record.hash = Some(hash);
    record
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, hash: Option<&str>, title: Option<&str>) -> DiscoveredFile {
        DiscoveredFile {
            path: PathBuf::from(path),
            size: 10,
            modified_ns: 1,
            inode: 1,
            hash: hash.map(Into::into),
            title: title.map(Into::into),
            authors: vec![],
            language: None,
            isbn: None,
            cover_path: None,
            error: None,
            seen_at: 0,
        }
    }

    #[test]
    fn candidates_are_grouped_by_content_and_classified() {
        let mut broken = file("/h/broken.epub", Some("x"), None);
        broken.error = Some("not a readable EPUB".into());
        let files = vec![
            file("/h/a.epub", Some("aaa"), Some("Zebra Tales")),
            file("/h/copy/a.epub", Some("aaa"), Some("Zebra Tales")),
            file("/h/b.epub", Some("bbb"), Some("Already Here")),
            file("/h/c.epub", Some("ccc"), None),
            file("/h/d.epub", Some("ddd"), Some("Deleted Once")),
            broken,
        ];
        let in_library = HashSet::from(["bbb".to_owned()]);
        let removed = HashSet::from(["ddd".to_owned()]);
        let candidates = group_candidates(&files, &in_library, &removed);
        let summary: Vec<(&str, CandidateStatus, usize)> = candidates
            .iter()
            .map(|c| (c.title.as_str(), c.status, c.paths.len()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Already Here", CandidateStatus::InLibrary, 1),
                ("broken", CandidateStatus::Unreadable, 1),
                ("c", CandidateStatus::New, 1),
                ("Deleted Once", CandidateStatus::Removed, 1),
                ("Zebra Tales", CandidateStatus::New, 2),
            ]
        );
        assert_eq!(candidates[1].id, "path:/h/broken.epub");
    }

    #[test]
    fn unique_targets_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            unique_target(dir.path(), "Book.epub"),
            dir.path().join("Book.epub")
        );
        std::fs::write(dir.path().join("Book.epub"), b"x").unwrap();
        std::fs::write(dir.path().join("Book (2).epub"), b"x").unwrap();
        assert_eq!(
            unique_target(dir.path(), "Book.EPUB"),
            dir.path().join("Book (3).epub")
        );
    }

    /// A host with nothing removed, no matches, and imports that need nothing more.
    #[derive(Default)]
    struct Host {
        imported: Mutex<Vec<String>>,
    }

    impl DiscoveryHost for Host {
        fn removed(&self) -> HashSet<String> {
            HashSet::new()
        }
        fn cached_match(&self, _: &Query) -> Option<Match> {
            None
        }
        fn look_up<'a>(
            &'a self,
            _: &'a Query,
        ) -> aoide_tts::BoxFuture<'a, Result<Vec<Match>, String>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn imported(&self, ids: Vec<String>) -> aoide_tts::BoxFuture<'_, Result<(), String>> {
            lock(&self.imported).extend(ids);
            Box::pin(async { Ok(()) })
        }
        fn pair(&self, _: &str, _: Vec<String>) -> Result<(), String> {
            Ok(())
        }
        fn apply<'a>(
            &'a self,
            _: &'a str,
            _: Match,
        ) -> aoide_tts::BoxFuture<'a, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn a_scan_finds_a_new_epub_and_importing_it_copies_it_into_the_library() {
        let dir = tempfile::tempdir().unwrap();
        let paths = super::super::tests::paths(dir.path());
        for folder in [&paths.home_dir, &paths.library_dir, &paths.covers_dir] {
            std::fs::create_dir_all(folder).unwrap();
        }
        std::fs::write(
            paths.home_dir.join("Found.epub"),
            super::super::tests::epub(),
        )
        .unwrap();
        let discovery = Arc::new(Discovery::new(
            Arc::new(Mutex::new(Library::open_in_memory().unwrap())),
            &paths,
            Arc::new(super::super::tests::Recorder::default()),
            Default::default(),
        ));
        let host = Host::default();

        let found = discovery
            .scan(ScanSettings::default(), &host)
            .await
            .unwrap();

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].status, CandidateStatus::New);
        let choice = ImportChoice {
            id: found[0].id.clone(),
            apply_suggestion: false,
            pair_audiobook: false,
        };
        let results = discovery.import(vec![choice], &host).await.unwrap();
        assert_eq!(results[0].error, None);
        assert!(paths.library_dir.join("Found.epub").is_file());
        assert_eq!(*lock(&host.imported), [found[0].id.clone()]);
    }
}
