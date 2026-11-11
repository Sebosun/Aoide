//! Disk use of generated audio, covers, books and the database, and cleanup of files that
//! nothing refers to any more. Source EPUBs outside Aoide's folders are never touched.
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use aoide_library::BookId;
use serde::Serialize;

use super::{lock, Core};

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
    pub database_bytes: u64,
    /// Files nothing refers to: unindexed audio and covers of removed books.
    pub unused_files: u64,
    pub unused_bytes: u64,
    /// Books with generated audio, largest first.
    pub books: Vec<BookStorage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
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

impl Core {
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
            &self.paths.audio_dir,
            &self.paths.covers_dir,
            &indexed,
            &books,
            before,
        ))
    }

    pub fn storage_report(&self) -> Result<StorageReport, String> {
        let (audio_bytes, usage) = {
            let library = lock(&self.library);
            (
                library.total_audio_bytes().map_err(|e| e.to_string())?,
                library.audio_usage().map_err(|e| e.to_string())?,
            )
        };
        let unused = self.unused()?;
        let db = &self.paths.db_path;
        let database_bytes = ["", "-wal", "-shm"]
            .iter()
            .map(|suffix| {
                let mut name = db.as_os_str().to_owned();
                name.push(suffix);
                file_len(Path::new(&name))
            })
            .sum();
        Ok(StorageReport {
            audio_bytes,
            covers_bytes: tree_bytes(&self.paths.covers_dir),
            books_bytes: tree_bytes(&self.paths.library_dir),
            database_bytes,
            unused_files: unused.files.len() as u64,
            unused_bytes: unused.bytes,
            books: usage
                .into_iter()
                .map(|u| BookStorage {
                    in_use: self.audio_in_use(&u.book_id),
                    book_id: u.book_id.0,
                    bytes: u.bytes,
                    segments: u.segments,
                    newest_at: u.newest_at,
                })
                .collect(),
        })
    }

    /// Deletes the files `storage_report` counts as unused and removes empty book folders.
    pub fn clean_unused_files(&self) -> Result<CleanResult, String> {
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

    /// Whether the book is playing, being prepared or being exported.
    fn audio_in_use(&self, id: &BookId) -> bool {
        lock(&self.session)
            .as_ref()
            .is_some_and(|s| s.target.book.id == id.0)
            || lock(&self.preparation).contains(&id.0)
            || lock(&self.extras).exporting.as_deref() == Some(id.0.as_str())
    }
}

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
}
