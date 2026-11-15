//! Disk use of generated audio, covers, books and the database, and cleanup of files that
//! nothing refers to any more. Source EPUBs outside Aoide's folders are never touched.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use aoide_library::{BookId, CachedSegment, Library};
use serde::Serialize;

use super::{lock, Core, CoreEvent, Paths};

/// Audio files younger than this may still be on their way into the index.
const SETTLE: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookStorage {
    pub book_id: String,
    pub bytes: u64,
    pub segments: u64,
    /// Unix milliseconds when the newest segment was generated.
    pub newest_at: i64,
    /// Playing, being prepared or being exported: bulk cleanup leaves it alone.
    pub in_use: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct StorageReport {
    /// Generated audio in the index.
    pub audio_bytes: u64,
    pub covers_bytes: u64,
    /// EPUB copies in the library folder.
    pub books_bytes: u64,
    /// The library database, without the recordings' sync data.
    pub database_bytes: u64,
    /// Recordings' transcripts, loudness and sync maps (kept in the database).
    pub sync_bytes: u64,
    /// Speech-recognition models.
    pub models_bytes: u64,
    /// Files nothing refers to: unindexed audio and covers of removed books.
    pub unused_files: u64,
    pub unused_bytes: u64,
    /// Books with generated audio, largest first.
    pub books: Vec<BookStorage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct CleanResult {
    pub files: u64,
    pub bytes: u64,
}

#[derive(Debug, Default, PartialEq)]
pub(super) struct Unused {
    pub files: Vec<PathBuf>,
    pub dirs: Vec<PathBuf>,
    pub bytes: u64,
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|list| list.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default()
}

fn older_than(path: &Path, before: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .is_ok_and(|modified| modified < before)
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

/// Total size of the files under `dir`, recursively.
fn tree_bytes(dir: &Path) -> u64 {
    entries(dir)
        .iter()
        .map(|path| {
            if path.is_dir() {
                tree_bytes(path)
            } else {
                file_len(path)
            }
        })
        .sum()
}

/// Opus files in the audio cache's book folders that the index does not list, book folders
/// left empty, and covers (directly in `covers_dir`) of books no longer in the library. Only
/// files modified before `before` count, so audio being written right now is left alone.
pub(super) fn find_unused(
    audio_dir: &Path,
    covers_dir: &Path,
    indexed: &HashSet<PathBuf>,
    books: &HashSet<String>,
    before: SystemTime,
) -> Unused {
    let mut unused = Unused::default();
    for dir in entries(audio_dir).into_iter().filter(|p| p.is_dir()) {
        let mut kept = 0;
        for file in entries(&dir) {
            let relative = file.strip_prefix(audio_dir).unwrap_or(&file);
            let stray = file.is_file()
                && file.extension().is_some_and(|e| e == "opus")
                && !indexed.contains(relative)
                && older_than(&file, before);
            if stray {
                unused.bytes += file_len(&file);
                unused.files.push(file);
            } else {
                kept += 1;
            }
        }
        if kept == 0 {
            unused.dirs.push(dir);
        }
    }
    for cover in entries(covers_dir).into_iter().filter(|p| p.is_file()) {
        let stem = cover
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if !books.contains(stem) && older_than(&cover, before) {
            unused.bytes += file_len(&cover);
            unused.files.push(cover);
        }
    }
    unused
}

/// What storage asks of the rest of the app.
pub(super) trait StorageHost {
    /// The book playing; its audio is evicted last.
    fn playing(&self) -> Option<String>;
    /// Books whose audio is never evicted: being prepared or exported.
    fn protected(&self) -> HashSet<String>;
}

/// Audio evicted to keep the cache within its limit.
pub(super) struct Evicted {
    pub books: HashSet<String>,
    pub keys: HashSet<String>,
}

/// Aoide's files on disk: how much they take, cleaning up the unused ones and keeping the
/// audio cache within its limit.
pub(super) struct Storage {
    library: Arc<Mutex<Library>>,
    audio_dir: PathBuf,
    covers_dir: PathBuf,
    library_dir: PathBuf,
    models_dir: PathBuf,
    db_path: PathBuf,
    /// When the cache was last trimmed; trimming is throttled.
    last_eviction: Mutex<Option<Instant>>,
}

impl Storage {
    pub fn new(library: Arc<Mutex<Library>>, paths: &Paths) -> Self {
        Self {
            library,
            audio_dir: paths.audio_dir.clone(),
            covers_dir: paths.covers_dir.clone(),
            library_dir: paths.library_dir.clone(),
            models_dir: paths.models_dir.clone(),
            db_path: paths.db_path.clone(),
            last_eviction: Mutex::new(None),
        }
    }

    fn unused(&self) -> Result<Unused, String> {
        let (indexed, books) = {
            let library = lock(&self.library);
            let indexed = library.segment_files().map_err(|e| e.to_string())?;
            let books = library.books().map_err(|e| e.to_string())?;
            (indexed, books.into_iter().map(|b| b.id.0).collect())
        };
        let before = SystemTime::now()
            .checked_sub(SETTLE)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        Ok(find_unused(
            &self.audio_dir,
            &self.covers_dir,
            &indexed,
            &books,
            before,
        ))
    }

    /// `in_use`: whether a book is playing, being prepared or being exported.
    pub fn report(&self, in_use: &dyn Fn(&str) -> bool) -> Result<StorageReport, String> {
        let (audio_bytes, usage, sync_bytes) = {
            let library = lock(&self.library);
            (
                library.total_audio_bytes().map_err(|e| e.to_string())?,
                library.audio_usage().map_err(|e| e.to_string())?,
                library.sync_data_bytes().map_err(|e| e.to_string())?,
            )
        };
        let unused = self.unused()?;
        let database_bytes: u64 = ["", "-wal", "-shm"]
            .iter()
            .map(|suffix| {
                let mut name = self.db_path.as_os_str().to_owned();
                name.push(suffix);
                file_len(Path::new(&name))
            })
            .sum();
        Ok(StorageReport {
            audio_bytes,
            covers_bytes: tree_bytes(&self.covers_dir),
            books_bytes: tree_bytes(&self.library_dir),
            database_bytes: database_bytes.saturating_sub(sync_bytes),
            sync_bytes,
            models_bytes: tree_bytes(&self.models_dir),
            unused_files: unused.files.len() as u64,
            unused_bytes: unused.bytes,
            books: usage
                .into_iter()
                .map(|u| BookStorage {
                    in_use: in_use(&u.book_id.0),
                    book_id: u.book_id.0,
                    bytes: u.bytes,
                    segments: u.segments,
                    newest_at: u.newest_at,
                })
                .collect(),
        })
    }

    /// Deletes the files `report` counts as unused and removes empty book folders.
    pub fn clean_unused(&self) -> Result<CleanResult, String> {
        let unused = self.unused()?;
        let mut result = CleanResult { files: 0, bytes: 0 };
        for file in &unused.files {
            let bytes = file_len(file);
            if std::fs::remove_file(file).is_ok() {
                result.files += 1;
                result.bytes += bytes;
            }
        }
        for dir in &unused.dirs {
            let _ = std::fs::remove_dir(dir); // Fails, harmlessly, if anything is left.
        }
        Ok(result)
    }

    /// Lets the next [`Storage::evict`] run at once (the limit changed).
    pub fn limit_changed(&self) {
        *lock(&self.last_eviction) = None;
    }

    /// Deletes the oldest generated audio until the cache fits `limit`. Audio of the playing
    /// book goes last and that of protected books stays. Throttled; returns what was deleted.
    pub fn evict(&self, limit: Option<u64>, host: &dyn StorageHost) -> Option<Evicted> {
        {
            let mut last = lock(&self.last_eviction);
            if last.is_some_and(|at| at.elapsed() < EVICTION_INTERVAL) {
                return None;
            }
            *last = Some(Instant::now());
        }
        let limit = limit?;
        let playing = host.playing().map(BookId);
        let protected = host.protected();
        let mut evicted = Evicted {
            books: HashSet::new(),
            keys: HashSet::new(),
        };
        loop {
            let library = lock(&self.library);
            let total = library.total_audio_bytes().ok()?;
            if total <= limit {
                break;
            }
            // Read well past the batch: protected books' old audio may come first.
            let oldest = library
                .oldest_segments(playing.as_ref(), EVICTION_BATCH * 25)
                .ok()?;
            let mut excess = total - limit;
            let victims: Vec<CachedSegment> = oldest
                .into_iter()
                .filter(|s| !protected.contains(&s.book_id.0))
                .take(EVICTION_BATCH)
                .take_while(|s| {
                    let take = excess > 0;
                    excess = excess.saturating_sub(s.bytes);
                    take
                })
                .collect();
            if victims.is_empty() {
                break; // Everything left belongs to protected books.
            }
            let keys: Vec<String> = victims.iter().map(|s| s.key.clone()).collect();
            library.delete_segments(&keys).ok()?;
            drop(library);
            for victim in victims {
                let _ = std::fs::remove_file(self.audio_dir.join(&victim.file));
                evicted.books.insert(victim.book_id.0);
                evicted.keys.insert(victim.key);
            }
        }
        if evicted.keys.is_empty() {
            return None;
        }
        tracing::info!(
            event = "cache.evict",
            segments = evicted.keys.len(),
            books = evicted.books.len(),
            "Deleted the oldest made audio to keep the cache within its limit"
        );
        Some(evicted)
    }
}

impl Core {
    pub fn storage_report(&self) -> Result<StorageReport, String> {
        self.storage.report(&|id| self.audio_in_use(id))
    }

    pub fn clean_unused_files(&self) -> Result<CleanResult, String> {
        self.storage.clean_unused()
    }

    /// Whether the book is playing, being prepared or being exported.
    fn audio_in_use(&self, id: &str) -> bool {
        self.playback.is_book(id) || self.protected().contains(id)
    }

    /// Trims the audio cache to its limit, and tells playback and generation what went.
    pub(super) fn enforce_cache_limit(&self) {
        let limit = self.app_settings().cache_limit_bytes;
        let Some(evicted) = self.storage.evict(limit, self) else {
            return;
        };
        self.playback.audio_lost(&evicted.keys);
        self.plan(super::PlanEvent::AudioLost {
            books: evicted.books,
            keys: evicted.keys,
        });
        self.emit(CoreEvent::LibraryChanged);
    }
}

impl StorageHost for Core {
    fn playing(&self) -> Option<String> {
        self.playback.book().map(|book| book.id.clone())
    }

    fn protected(&self) -> HashSet<String> {
        let mut protected: HashSet<String> = self.generation.queued().into_iter().collect();
        protected.extend(self.exports.exporting());
        protected
    }
}

/// Cache-size checks run at most this often.
const EVICTION_INTERVAL: Duration = Duration::from_secs(20);
const EVICTION_BATCH: usize = 400;

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0_u8; bytes]).unwrap();
    }

    #[test]
    fn finds_unindexed_audio_empty_book_folders_and_covers_of_removed_books() {
        let root = tempfile::tempdir().unwrap();
        let (audio, covers) = (root.path().join("audio"), root.path().join("covers"));
        write(&audio.join("a/kept.opus"), 10);
        write(&audio.join("a/stray.opus"), 7);
        write(&audio.join("a/notes.txt"), 3);
        write(&audio.join("gone/old.opus"), 5);
        std::fs::create_dir_all(audio.join("empty")).unwrap();
        write(&covers.join("a.jpg"), 4);
        write(&covers.join("gone.png"), 6);
        write(&covers.join("discovered/x.jpg"), 2);
        let indexed: HashSet<PathBuf> = [PathBuf::from("a/kept.opus")].into();
        let books: HashSet<String> = ["a".to_string()].into();
        let later = SystemTime::now() + Duration::from_secs(5);

        let mut unused = find_unused(&audio, &covers, &indexed, &books, later);
        unused.files.sort();
        unused.dirs.sort();
        assert_eq!(
            unused.files,
            vec![
                audio.join("a/stray.opus"),
                audio.join("gone/old.opus"),
                covers.join("gone.png")
            ]
        );
        assert_eq!(unused.dirs, vec![audio.join("empty"), audio.join("gone")]);
        assert_eq!(unused.bytes, 7 + 5 + 6);

        let fresh = find_unused(&audio, &covers, &indexed, &books, SystemTime::UNIX_EPOCH);
        assert!(
            fresh.files.is_empty(),
            "files written just now are left alone"
        );
        assert_eq!(tree_bytes(&covers), 4 + 6 + 2);
    }

    struct Host {
        playing: Option<String>,
        protected: HashSet<String>,
    }

    impl StorageHost for Host {
        fn playing(&self) -> Option<String> {
            self.playing.clone()
        }
        fn protected(&self) -> HashSet<String> {
            self.protected.clone()
        }
    }

    fn storage_with_audio(root: &Path, books: &[(&str, usize)]) -> Storage {
        let ids: Vec<&str> = books.iter().map(|(id, _)| *id).collect();
        let library = crate::core::tests::library_with(&ids);
        let mut at = 0;
        for (id, segments) in books {
            for n in 0..*segments {
                at += 1;
                let file = PathBuf::from(format!("{id}/{n}.opus"));
                write(&root.join("audio").join(&file), 100);
                lock(&library)
                    .record_segment(&CachedSegment {
                        key: format!("{id}-{n}"),
                        book_id: BookId((*id).into()),
                        file,
                        duration_ms: 1_000,
                        bytes: 100,
                        word_timings: None,
                        created_at: at,
                    })
                    .unwrap();
            }
        }
        let paths = Paths {
            audio_dir: root.join("audio"),
            ..crate::core::tests::paths(root)
        };
        Storage::new(library, &paths)
    }

    #[test]
    fn eviction_deletes_the_oldest_audio_of_books_not_protected_until_it_fits() {
        let root = tempfile::tempdir().unwrap();
        let storage = storage_with_audio(root.path(), &[("kept", 3), ("old", 3)]);
        let host = Host {
            playing: None,
            protected: ["kept".to_string()].into(),
        };

        let evicted = storage.evict(Some(400), &host).expect("something evicted");

        assert_eq!(evicted.books, ["old".to_string()].into());
        assert_eq!(evicted.keys.len(), 2);
        assert!(!root.path().join("audio/old/0.opus").exists());
        assert!(root.path().join("audio/old/2.opus").exists());
        assert!(storage.evict(Some(0), &host).is_none(), "throttled");
        storage.limit_changed();
        assert!(storage.evict(None, &host).is_none(), "no limit");
    }
}
