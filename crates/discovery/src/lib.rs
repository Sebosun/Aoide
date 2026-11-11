//! Fast, parallel search for EPUB files on the local disk, and streaming content hashes.
//!
//! Nothing here opens files for writing or follows symbolic links.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

/// Folder names that never contain a reader's books.
const SKIP_NAMES: &[&str] = &[
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    "site-packages",
    "build",
    "dist",
    "vendor",
    "bower_components",
    "Steam",
    "steamapps",
];
/// Report progress every this many files.
const PROGRESS_EVERY: u64 = 4096;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    /// Folders never entered (e.g. Aoide's own library folder).
    pub exclude: Vec<PathBuf>,
    /// Hidden folders that are searched anyway.
    pub include_hidden: Vec<PathBuf>,
    /// Folder names skipped anywhere (build output, dependencies, caches).
    pub skip_names: Vec<String>,
    /// Stay on the filesystem of each root (no other drives or mounts).
    pub same_filesystem: bool,
    /// Walker threads; 0 picks one per CPU.
    pub threads: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            exclude: Vec::new(),
            include_hidden: Vec::new(),
            skip_names: SKIP_NAMES.iter().map(|s| s.to_string()).collect(),
            same_filesystem: true,
            threads: 0,
        }
    }
}

impl ScanOptions {
    /// Whether the walker may enter `dir` (never called for the roots themselves).
    fn enter(&self, dir: &Path, name: &str) -> bool {
        if self.exclude.iter().any(|e| dir.starts_with(e)) {
            return false;
        }
        if name.starts_with('.') {
            // Hidden: only on the way to, or inside, an allowed hidden folder.
            return self
                .include_hidden
                .iter()
                .any(|allowed| allowed.starts_with(dir) || dir.starts_with(allowed));
        }
        !self.skip_names.iter().any(|s| s == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,
    pub modified_ns: i64,
    pub inode: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    pub dirs: u64,
    pub files: u64,
    pub found: u64,
}

/// EPUB files (case-insensitive `.epub`) under the roots, sorted by path. Progress is reported
/// now and then and once more with the final totals. A set `cancel` stops early.
pub fn find_epubs(
    options: &ScanOptions,
    cancel: &AtomicBool,
    progress: &(dyn Fn(Progress) + Sync),
) -> Vec<Found> {
    if cancel.load(Ordering::Relaxed) || options.roots.is_empty() {
        return Vec::new();
    }
    let mut builder = WalkBuilder::new(&options.roots[0]);
    for root in &options.roots[1..] {
        builder.add(root);
    }
    let threads = if options.threads == 0 {
        std::thread::available_parallelism().map_or(4, |n| n.get())
    } else {
        options.threads
    };
    builder
        .standard_filters(false)
        .follow_links(false)
        .same_file_system(options.same_filesystem)
        .threads(threads);
    let filter_options = options.clone();
    builder.filter_entry(move |entry| {
        if entry.depth() == 0 || !entry.file_type().is_some_and(|t| t.is_dir()) {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        filter_options.enter(entry.path(), &name)
    });
    let dirs = AtomicU64::new(0);
    let files = AtomicU64::new(0);
    let found = Mutex::new(Vec::new());
    let report = |found_count: u64| Progress {
        dirs: dirs.load(Ordering::Relaxed),
        files: files.load(Ordering::Relaxed),
        found: found_count,
    };
    builder.build_parallel().run(|| {
        Box::new(|entry| {
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            let Some(kind) = entry.file_type() else {
                return WalkState::Continue;
            };
            if kind.is_dir() {
                dirs.fetch_add(1, Ordering::Relaxed);
                return WalkState::Continue;
            }
            if !kind.is_file() {
                return WalkState::Continue; // Symlinks and special files.
            }
            let seen = files.fetch_add(1, Ordering::Relaxed) + 1;
            let is_epub = entry
                .path()
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("epub"));
            if is_epub {
                if let Ok(meta) = entry.metadata() {
                    let modified_ns = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos() as i64);
                    found.lock().unwrap_or_else(|p| p.into_inner()).push(Found {
                        path: entry.path().to_path_buf(),
                        size: meta.len(),
                        modified_ns,
                        inode: inode(&meta),
                    });
                }
            }
            if seen.is_multiple_of(PROGRESS_EVERY) {
                let count = found.lock().unwrap_or_else(|p| p.into_inner()).len() as u64;
                progress(report(count));
            }
            WalkState::Continue
        })
    });
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let mut found = found.into_inner().unwrap_or_else(|p| p.into_inner());
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found.dedup_by(|a, b| a.path == b.path);
    progress(report(found.len() as u64));
    found
}

#[cfg(unix)]
fn inode(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.ino()
}

#[cfg(not(unix))]
fn inode(_meta: &std::fs::Metadata) -> u64 {
    0
}

/// Lower-case hex SHA-256 of a file, read in chunks.
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Hashes files on `threads` threads, keeping input order. `done` is called after each file.
pub fn hash_all(
    paths: Vec<PathBuf>,
    threads: usize,
    cancel: &AtomicBool,
    done: &(dyn Fn(usize) + Sync),
) -> Vec<(PathBuf, std::io::Result<String>)> {
    let hash = |path: PathBuf| {
        let result = if cancel.load(Ordering::Relaxed) {
            Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            ))
        } else {
            hash_file(&path)
        };
        done(1);
        (path, result)
    };
    match rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
    {
        Ok(pool) => pool.install(|| paths.into_par_iter().map(hash).collect()),
        Err(_) => paths.into_iter().map(hash).collect(),
    }
}
