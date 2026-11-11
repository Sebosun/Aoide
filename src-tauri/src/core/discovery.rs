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
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportChoice {
    pub id: String,
    /// Apply the suggested Open Library match after importing.
    pub apply_suggestion: bool,
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

impl Core {
    fn scan_options(&self) -> aoide_discovery::ScanOptions {
        let settings = self.app_settings().scan;
        let roots: Vec<PathBuf> = if settings.roots.is_empty() {
            vec![self.paths.home_dir.clone()]
        } else {
            settings.roots.iter().map(PathBuf::from).collect()
        };
        let mut exclude: Vec<PathBuf> = settings.exclude.iter().map(PathBuf::from).collect();
        exclude.push(self.paths.library_dir.clone());
        exclude.push(self.paths.audio_dir.clone());
        if let Some(data) = self.paths.db_path.parent() {
            exclude.push(data.to_path_buf());
        }
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

    /// Scans the configured folders. Returns the candidates afterwards.
    pub async fn scan_for_books(self: &Arc<Self>) -> Result<Vec<Candidate>, String> {
        {
            let mut extras = lock(&self.extras);
            if extras.scan_cancel.is_some() {
                return Err("a scan is already running".into());
            }
            extras.scan_cancel = Some(Arc::new(AtomicBool::new(false)));
        }
        let cancel = lock(&self.extras).scan_cancel.clone().expect("set above");
        let core = Arc::clone(self);
        let result = tokio::task::spawn_blocking(move || core.scan_blocking(&cancel))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        lock(&self.extras).scan_cancel = None;
        if let Err(error) = &result {
            self.emit_scan(ScanProgress {
                stage: if error == "cancelled" {
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
        self.discovered_books()
    }

    pub fn cancel_scan(&self) {
        if let Some(cancel) = lock(&self.extras).scan_cancel.as_ref() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    fn scan_blocking(&self, cancel: &AtomicBool) -> Result<(), String> {
        let options = self.scan_options();
        let last_emit = Mutex::new(Instant::now());
        let found = aoide_discovery::find_epubs(&options, cancel, &|p| {
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
        });
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let scan_started = now_ms();
        // Unchanged files keep what we learned last time.
        let (unchanged, changed): (Vec<_>, Vec<_>) = {
            let library = lock(&self.library);
            found.into_iter().partition(|f| {
                library.discovered(&f.path).ok().flatten().is_some_and(|d| {
                    d.size == f.size && d.modified_ns == f.modified_ns && d.inode == f.inode
                })
            })
        };
        lock(&self.library)
            .mark_discovered_seen(
                &unchanged.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
                scan_started,
            )
            .map_err(|e| e.to_string())?;
        let total = changed.len() as u64;
        let read = std::sync::atomic::AtomicU64::new(0);
        let covers = self.paths.covers_dir.join("discovered");
        let _ = std::fs::create_dir_all(&covers);
        let process = |f: &aoide_discovery::Found| {
            let record = describe_file(f, &covers, scan_started);
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
        let records: Vec<DiscoveredFile> = match rayon::ThreadPoolBuilder::new()
            .num_threads(READ_THREADS)
            .build()
        {
            Ok(pool) => pool.install(|| {
                changed
                    .par_iter()
                    .filter(|_| !cancel.load(std::sync::atomic::Ordering::Relaxed))
                    .map(process)
                    .collect()
            }),
            Err(_) => changed.iter().map(process).collect(),
        };
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let library = lock(&self.library);
        for record in &records {
            library
                .upsert_discovered(record)
                .map_err(|e| e.to_string())?;
        }
        let _ = library.forget_discovered_before(scan_started);
        drop(library);
        let found = (unchanged.len() + records.len()) as u64;
        self.emit_scan(ScanProgress {
            stage: ScanStage::Done,
            dirs: 0,
            files: 0,
            found,
            read: total,
            to_read: total,
            error: None,
        });
        Ok(())
    }

    /// What the last scan found, grouped and compared with the library.
    pub fn discovered_books(&self) -> Result<Vec<Candidate>, String> {
        let files = lock(&self.library)
            .discovered_files()
            .map_err(|e| e.to_string())?;
        let removed = self.ignored();
        let in_library: HashSet<String> = lock(&self.library)
            .books()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|b| b.id.0)
            .filter(|id| !removed.contains(id))
            .collect();
        let mut candidates = group_candidates(&files, &in_library, &removed);
        for candidate in candidates
            .iter_mut()
            .filter(|c| c.status == CandidateStatus::New)
        {
            candidate.suggestion = self.cached_match(&query_for(candidate));
        }
        Ok(candidates)
    }

    /// Looks the given candidates up on Open Library, one at a time (cached for 30 days).
    /// Emits `discovery://details` with each candidate id as its lookup finishes.
    pub async fn fetch_discovered_details(&self, ids: Vec<String>) -> Result<(), String> {
        if !self.app_settings().metadata_lookup {
            return Err("Online lookups are turned off in Settings.".into());
        }
        let candidates = self.discovered_books()?;
        for id in ids {
            let Some(candidate) = candidates.iter().find(|c| c.id == id) else {
                continue;
            };
            if candidate.suggestion.is_some() || candidate.title.trim().is_empty() {
                self.emit(CoreEvent::DiscoveryDetails(id.clone()));
                continue;
            }
            match self.lookup_query(&query_for(candidate), false).await {
                Ok(_) => {}
                Err(error) => eprintln!("aoide: lookup for {id} failed: {error}"),
            }
            self.emit(CoreEvent::DiscoveryDetails(id.clone()));
            tokio::time::sleep(LOOKUP_GAP).await;
        }
        Ok(())
    }

    /// Copies the chosen books into the library folder (reflink when the filesystem supports
    /// it) and optionally applies their suggested Open Library match. Returns per-book results.
    pub async fn import_discovered(
        self: &Arc<Self>,
        choices: Vec<ImportChoice>,
    ) -> Result<Vec<ImportResult>, String> {
        let candidates = self.discovered_books()?;
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
                        imported.push((c.clone(), choice.apply_suggestion));
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
        // Re-imported books are no longer "removed".
        let mut ignored = self.ignored();
        for (candidate, _) in &imported {
            ignored.remove(&candidate.id);
        }
        self.set_ignored(&ignored);
        let core = Arc::clone(self);
        tokio::task::spawn_blocking(move || core.rescan())
            .await
            .map_err(|e| e.to_string())??;
        for (candidate, apply) in imported {
            if let (true, Some(suggestion)) = (apply, candidate.suggestion) {
                if let Err(error) = self.apply_metadata(&candidate.id, suggestion).await {
                    eprintln!(
                        "aoide: could not apply details to {}: {error}",
                        candidate.id
                    );
                }
            }
        }
        self.emit(CoreEvent::DiscoveryImport(ImportProgress {
            done: total,
            total,
            current: None,
        }));
        Ok(results)
    }

    async fn copy_into_library(&self, candidate: &Candidate) -> Result<(), String> {
        let source = candidate
            .paths
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .ok_or("the file is gone; scan again")?;
        let library_dir = self.paths.library_dir.clone();
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
                let ext = match cover.media_type.as_str() {
                    "image/png" => "png",
                    "image/gif" => "gif",
                    "image/webp" => "webp",
                    _ => "jpg",
                };
                let path = covers.join(format!("{hash}.{ext}"));
                if path.exists() || std::fs::write(&path, &cover.bytes).is_ok() {
                    record.cover_path = Some(path);
                }
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
}
