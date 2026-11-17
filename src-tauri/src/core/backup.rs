//! Backups: one zip with the library (a consistent copy of its database), covers, saved
//! articles and settings, and the books if chosen; and restoring one, here or on another
//! computer.
//!
//! A restore is staged (unpacked and checked beside the database) and applied at the next
//! start, before anything opens the library, as `rename.rs` moves folders. The current files are
//! moved aside first and put back if any step fails, so a failed restore leaves the previous
//! library in use.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use aoide_library::Library;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{lock, CoreEvent, Paths};

/// The backup format this build writes and reads.
pub const FORMAT: u32 = 1;
/// A backup file's extension.
pub const EXTENSION: &str = "aoide-backup";

/// What goes in besides the library, covers, articles and settings.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupOptions {
    /// The books in the library folder (they can be large).
    pub books: bool,
}

/// Where things were on the computer the backup was made on, so paths can be moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Places {
    pub home_dir: PathBuf,
    pub library_dir: PathBuf,
    pub covers_dir: PathBuf,
    pub articles_dir: PathBuf,
}

/// One file in the backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

/// `manifest.json`: what the backup is and holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub aoide_version: String,
    pub schema_version: i64,
    /// Unix milliseconds.
    pub created_at: i64,
    pub options: BackupOptions,
    pub from: Places,
    pub files: Vec<Entry>,
}

const MANIFEST: &str = "manifest.json";
const DATABASE: &str = "library.sqlite3";
/// The settings files and folder taken from the config folder.
const CONFIG: [&str; 3] = ["settings.toml", "site-rules.toml", "themes"];

/// Where a restore is unpacked, and the marker that it is complete.
pub fn staging_dir(paths: &Paths) -> PathBuf {
    paths.db_path.with_file_name("restore-staging")
}
const READY: &str = "READY";
/// Written once a restore is in place, before the files set aside are removed.
const APPLIED: &str = "APPLIED";
/// In the aside folder: the files a restore has put in place so far, one name a line.
const PLACED: &str = "placed";
/// Where the current files wait while a restore is applied.
fn aside_dir(paths: &Paths) -> PathBuf {
    paths.db_path.with_file_name("restore-previous")
}

/// Writes a backup of the library at `paths` to `to`, calling `progress(done, of)` per file.
/// Returns its manifest and size. The file appears only once complete.
pub fn write(
    paths: &Paths,
    to: &Path,
    options: BackupOptions,
    now: i64,
    mut progress: impl FnMut(usize, usize),
) -> Result<(Manifest, u64), String> {
    if to.exists() {
        return Err(format!("{} already exists.", to.display()));
    }
    let part = with_suffix(to, ".part");
    let copy = paths
        .db_path
        .with_file_name(format!("backup-{}-{now}.sqlite3", std::process::id()));
    let _ = std::fs::remove_file(&copy);
    let _cleanup = Cleanup(vec![copy.clone(), part.clone()]);
    Library::copy_consistent(&paths.db_path, &copy).map_err(|e| e.to_string())?;
    let schema_version = Library::schema_version_of(&copy).map_err(|e| e.to_string())?;

    let mut sources = vec![(DATABASE.to_owned(), copy.clone())];
    walk(&paths.covers_dir, "covers", &mut sources);
    walk(&paths.articles_dir, "articles", &mut sources);
    for name in CONFIG {
        walk(
            &paths.config_dir.join(name),
            &format!("config/{name}"),
            &mut sources,
        );
    }
    if options.books {
        walk(&paths.library_dir, "books", &mut sources);
    }

    let mut zip = zip::ZipWriter::new(File::create(&part).map_err(|e| e.to_string())?);
    let mut files = Vec::with_capacity(sources.len());
    progress(0, sources.len());
    for (done, (name, source)) in sources.iter().enumerate() {
        let mut input = File::open(source).map_err(|e| format!("{}: {e}", source.display()))?;
        let size = input.metadata().map_err(|e| e.to_string())?.len();
        let packed = if already_packed(name) {
            zip::CompressionMethod::Stored
        } else {
            zip::CompressionMethod::Deflated
        };
        let entry = zip::write::SimpleFileOptions::default()
            .compression_method(packed)
            .large_file(size >= u32::MAX as u64);
        zip.start_file(name.as_str(), entry)
            .map_err(|e| e.to_string())?;
        let (bytes, sha256) = copy_hashed(&mut input, &mut zip)?;
        files.push(Entry {
            path: name.clone(),
            bytes,
            sha256,
        });
        progress(done + 1, sources.len());
    }
    let manifest = Manifest {
        format: FORMAT,
        aoide_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version,
        created_at: now,
        options,
        from: Places {
            home_dir: paths.home_dir.clone(),
            library_dir: paths.library_dir.clone(),
            covers_dir: paths.covers_dir.clone(),
            articles_dir: paths.articles_dir.clone(),
        },
        files,
    };
    zip.start_file(MANIFEST, zip::write::SimpleFileOptions::default())
        .map_err(|e| e.to_string())?;
    let json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;
    let file = zip.finish().map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&part, to).map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(to).map_err(|e| e.to_string())?.len();
    Ok((manifest, bytes))
}

/// The manifest of the backup at `path`, and the schema version of its database as read from
/// the database itself (without changing it).
pub fn inspect(path: &Path) -> Result<(Manifest, i64), String> {
    let mut zip = open_zip(path)?;
    let manifest = read_manifest(&mut zip)?;
    static INSPECTED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let copy = std::env::temp_dir().join(format!(
        "aoide-inspect-{}-{}.sqlite3",
        std::process::id(),
        INSPECTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    let _cleanup = Cleanup(vec![copy.clone()]);
    {
        let mut entry = zip
            .by_name(DATABASE)
            .map_err(|_| damaged("it has no library"))?;
        let mut out = File::create(&copy).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    let schema =
        Library::schema_version_of(&copy).map_err(|_| damaged("its library cannot be read"))?;
    Ok((manifest, schema))
}

/// Unpacks the backup at `path` beside the library, checking every file; it is applied at the
/// next start ([`apply_staged`]). Refuses a backup from a newer Aoide.
pub fn stage(paths: &Paths, path: &Path) -> Result<Manifest, String> {
    let (manifest, schema) = inspect(path)?;
    if schema > Library::schema_version() {
        return Err(NEWER.into());
    }
    let staging = staging_dir(paths);
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    let unpacked = unpack(path, &manifest, &staging);
    if unpacked.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    unpacked.map(|()| manifest)
}

fn unpack(path: &Path, manifest: &Manifest, staging: &Path) -> Result<(), String> {
    let mut zip = open_zip(path)?;
    std::fs::create_dir_all(staging).map_err(|e| e.to_string())?;
    let mut seen = 0;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().to_owned();
        if name == MANIFEST || entry.is_dir() {
            continue;
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| damaged("a file in it points outside it"))?;
        let expected = manifest
            .files
            .iter()
            .find(|f| f.path == name)
            .ok_or_else(|| damaged("it holds a file its list does not name"))?;
        let target = staging.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = File::create(&target).map_err(|e| e.to_string())?;
        let (bytes, sha256) = copy_hashed(&mut entry, &mut out)?;
        if bytes != expected.bytes || sha256 != expected.sha256 {
            return Err(damaged(&format!("{name} has changed since it was written")));
        }
        seen += 1;
    }
    if seen != manifest.files.len() {
        return Err(damaged("files are missing from it"));
    }
    let json = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::write(staging.join(MANIFEST), json).map_err(|e| e.to_string())?;
    std::fs::write(staging.join(READY), b"").map_err(|e| e.to_string())
}

/// What a restore replaces: a name in the aside folder, the live path, and the staged one.
fn swapped(paths: &Paths) -> Vec<(String, PathBuf, Option<PathBuf>)> {
    let staging = staging_dir(paths);
    let db = paths.db_path.to_string_lossy().into_owned();
    let mut list = vec![
        (
            DATABASE.to_owned(),
            paths.db_path.clone(),
            Some(staging.join(DATABASE)),
        ),
        // The current database's log goes with it; the restored one has none.
        ("wal".into(), PathBuf::from(format!("{db}-wal")), None),
        ("shm".into(), PathBuf::from(format!("{db}-shm")), None),
        (
            "covers".into(),
            paths.covers_dir.clone(),
            Some(staging.join("covers")),
        ),
        (
            "articles".into(),
            paths.articles_dir.clone(),
            Some(staging.join("articles")),
        ),
    ];
    for name in CONFIG {
        list.push((
            format!("config-{name}"),
            paths.config_dir.join(name),
            Some(staging.join("config").join(name)),
        ));
    }
    list
}

/// At start, before the library is opened: puts a staged restore in place of the current
/// library. `Ok(None)` when none is staged. On failure the previous files are back in place.
pub fn apply_staged(paths: &Paths) -> Result<Option<Manifest>, String> {
    let staging = staging_dir(paths);
    if staging.join(APPLIED).exists() {
        // It was in place when the last start stopped: only the tidying was left.
        let _ = std::fs::remove_dir_all(aside_dir(paths));
        let _ = std::fs::remove_dir_all(&staging);
        return Ok(None);
    }
    if aside_dir(paths).exists() {
        // The last start stopped part way through: the previous library goes back, and the
        // restore (some of its files already moved) is not tried again.
        roll_back(paths);
        let _ = std::fs::remove_dir_all(&staging);
        return Err("The last restore was interrupted; the previous library is back.".into());
    }
    if !staging.join(READY).exists() {
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        return Ok(None);
    }
    let applied = apply(paths, &staging);
    if applied.is_err() {
        roll_back(paths);
    } else {
        let _ = std::fs::write(staging.join(APPLIED), b"");
        let _ = std::fs::remove_dir_all(aside_dir(paths));
    }
    let _ = std::fs::remove_dir_all(&staging);
    applied.map(Some)
}

fn apply(paths: &Paths, staging: &Path) -> Result<Manifest, String> {
    let manifest: Manifest = std::fs::read(staging.join(MANIFEST))
        .map_err(|e| e.to_string())
        .and_then(|json| serde_json::from_slice(&json).map_err(|e| e.to_string()))?;
    let aside = aside_dir(paths);
    std::fs::create_dir_all(&aside).map_err(|e| e.to_string())?;
    let list = swapped(paths);
    for (key, live, _) in &list {
        if live.exists() {
            move_path(live, &aside.join(key))?;
        }
    }
    // Each file put in place is noted, so a roll back removes exactly those.
    let mut placed = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(aside.join(PLACED))
        .map_err(|e| e.to_string())?;
    for (key, live, staged) in &list {
        match staged {
            Some(staged) if staged.exists() => {
                if let Some(parent) = live.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                writeln!(placed, "{key}").map_err(|e| e.to_string())?;
                move_path(staged, live)?;
            }
            _ => {}
        }
    }
    for dir in [&paths.covers_dir, &paths.articles_dir] {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    {
        let library = Library::open(&paths.db_path).map_err(|e| e.to_string())?;
        let from = &manifest.from;
        for (old, new) in [
            (&from.library_dir, &paths.library_dir),
            (&from.covers_dir, &paths.covers_dir),
            (&from.articles_dir, &paths.articles_dir),
            (&from.home_dir, &paths.home_dir),
        ] {
            if old != new {
                library.move_paths(old, new).map_err(|e| e.to_string())?;
            }
        }
        // The files' times and places are another computer's: the next scan hashes each once.
        library.clear_scanned_files().map_err(|e| e.to_string())?;
    }
    // The books, beside any already in the library folder (none is replaced).
    let books = staging.join("books");
    let mut found = Vec::new();
    walk(&books, "", &mut found);
    for (name, staged) in found {
        let live = paths.library_dir.join(name.trim_start_matches('/'));
        if live.exists() {
            continue;
        }
        if let Some(parent) = live.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        move_path(&staged, &live)?;
    }
    Ok(manifest)
}

/// Puts back what [`apply`] moved aside, removing what it put in place (and only that: a start
/// that stopped part way left the rest untouched).
fn roll_back(paths: &Paths) {
    let aside = aside_dir(paths);
    let placed = std::fs::read_to_string(aside.join(PLACED)).unwrap_or_default();
    let placed: Vec<&str> = placed.lines().collect();
    for (key, live, _) in swapped(paths).into_iter().rev() {
        if placed.contains(&key.as_str()) {
            let _ = remove_path(&live);
        }
        let saved = aside.join(&key);
        if !saved.exists() {
            continue;
        }
        let _ = remove_path(&live);
        if let Err(error) = move_path(&saved, &live) {
            tracing::error!(event = "backup.restored", outcome = "rollback_failed", %error, item = key, "Could not put a file back after a failed restore");
        }
    }
    let _ = std::fs::remove_dir_all(&aside);
}

const NEWER: &str = "This backup was made by a newer version of Aoide. Update Aoide to restore it.";

fn damaged(why: &str) -> String {
    format!("This backup is damaged: {why}.")
}

fn open_zip(path: &Path) -> Result<zip::ZipArchive<File>, String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    zip::ZipArchive::new(file).map_err(|_| "This is not an Aoide backup.".to_owned())
}

fn read_manifest(zip: &mut zip::ZipArchive<File>) -> Result<Manifest, String> {
    let mut json = Vec::new();
    zip.by_name(MANIFEST)
        .map_err(|_| "This is not an Aoide backup.".to_owned())?
        .read_to_end(&mut json)
        .map_err(|e| e.to_string())?;
    let manifest: Manifest =
        serde_json::from_slice(&json).map_err(|_| damaged("its list of files cannot be read"))?;
    if manifest.format > FORMAT {
        return Err(NEWER.into());
    }
    Ok(manifest)
}

/// Copies `input` to `out`, returning the bytes copied and their SHA-256 in hex.
fn copy_hashed(input: &mut impl Read, out: &mut impl Write) -> Result<(u64, String), String> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut bytes = 0u64;
    loop {
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        bytes += n as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((bytes, hex))
}

/// Every regular file under `root` (or `root` itself if it is one), as (`prefix/relative`, path),
/// sorted. Links are not followed.
fn walk(root: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(meta) = std::fs::symlink_metadata(root) else {
        return;
    };
    if meta.is_file() {
        out.push((prefix.to_owned(), root.to_path_buf()));
        return;
    }
    if !meta.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let prefix = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        walk(&entry.path(), &prefix, out);
    }
}

/// Formats that do not shrink when deflated.
fn already_packed(name: &str) -> bool {
    let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "epub"
            | "pdf"
            | "jpg"
            | "jpeg"
            | "png"
            | "webp"
            | "gif"
            | "m4b"
            | "m4a"
            | "mp3"
            | "ogg"
            | "opus"
            | "flac"
            | "zip"
    )
}

/// Moves a file or folder, copying across file systems.
fn move_path(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_tree(from, to).map_err(|e| format!("{}: {e}", from.display()))?;
    remove_path(from).map_err(|e| e.to_string())
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::symlink_metadata(from)?.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_tree(&entry.path(), &to.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(drop)
    }
}

fn remove_path(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

/// `path` with `suffix` added to its file name.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Removes these files when dropped (a temporary copy, a part-written file).
struct Cleanup(Vec<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Automatic backups (Settings › Storage).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupSettings {
    /// Where they are written; `None`: no automatic backups.
    pub folder: Option<PathBuf>,
    /// How many days apart (1–365).
    pub every_days: u32,
    /// How many are kept; older ones are removed (1–100).
    pub keep: u32,
    /// Whether the books go in too.
    pub books: bool,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            folder: None,
            every_days: 7,
            keep: 5,
            books: false,
        }
    }
}

/// A backup being written, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupProgressView {
    /// Files packed so far, of `of`.
    pub done: usize,
    pub of: usize,
    /// The file being written.
    pub path: String,
    /// Written by the schedule, not asked for.
    pub automatic: bool,
    /// Set once written.
    pub finished: bool,
}

/// What a backup file holds, before restoring it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupInfoView {
    /// Unix milliseconds.
    pub created_at: i64,
    pub aoide_version: String,
    pub books_included: bool,
    pub bytes: u64,
    /// Why it cannot be restored here (a newer Aoide made it); `None` when it can.
    pub refused: Option<String>,
}

/// A backup written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BackupWritten {
    pub path: String,
    pub bytes: u64,
}

/// What [`Backups`] needs from the rest of the app.
pub(super) trait BackupHost {
    fn paths(&self) -> &Paths;
    fn settings(&self) -> BackupSettings;
    /// Unix milliseconds.
    fn now_ms(&self) -> i64;
    fn emit(&self, event: CoreEvent);
    fn report(&self, doing: &str, error: String);
}

/// Writing backups, one at a time, and the automatic ones.
#[derive(Default)]
pub(super) struct Backups {
    writing: Mutex<bool>,
    /// The automatic backup's folder failed, and the user was told (once, until one works).
    folder_failed: std::sync::atomic::AtomicBool,
}

const AUTOMATIC: &str = "Aoide backup ";
const BEFORE_RESTORE: &str = "Aoide before restore ";

impl Backups {
    /// Writes a backup to `to`, telling the window how far it is.
    pub fn back_up(
        &self,
        host: &dyn BackupHost,
        to: &Path,
        options: BackupOptions,
        automatic: bool,
    ) -> Result<BackupWritten, String> {
        {
            let mut writing = lock(&self.writing);
            if *writing {
                return Err("A backup is already being written.".into());
            }
            *writing = true;
        }
        let started = std::time::Instant::now();
        let path = to.to_string_lossy().into_owned();
        let view = |done, of, finished| BackupProgressView {
            done,
            of,
            path: path.clone(),
            automatic,
            finished,
        };
        let written = write(host.paths(), to, options, host.now_ms(), |done, of| {
            host.emit(CoreEvent::BackupProgress(view(done, of, false)))
        });
        *lock(&self.writing) = false;
        let took_ms = crate::logging::took(started);
        match &written {
            Ok((manifest, bytes)) => {
                tracing::info!(
                    event = "backup.written",
                    outcome = "ok",
                    bytes,
                    books_included = options.books,
                    automatic,
                    took_ms,
                    "Wrote a backup"
                );
                let files = manifest.files.len();
                host.emit(CoreEvent::BackupProgress(view(files, files, true)));
            }
            Err(error) => {
                tracing::warn!(event = "backup.written", outcome = "failed", %error, books_included = options.books, automatic, took_ms, "Could not write a backup");
            }
        }
        written.map(|(_, bytes)| BackupWritten { path, bytes })
    }

    /// Writes an automatic backup if one is due, and removes the oldest beyond the number kept.
    /// A failing folder is told to the user once, until a backup works again.
    pub fn run_automatic(&self, host: &dyn BackupHost) {
        let settings = host.settings();
        let Some(folder) = settings.folder.clone() else {
            return;
        };
        let now = host.now_ms();
        let newest = automatic_backups(&folder)
            .into_iter()
            .map(|(_, at)| at)
            .max();
        let every = i64::from(settings.every_days.clamp(1, 365)) * 24 * 3_600_000;
        if newest.is_some_and(|at| now - at < every) {
            return;
        }
        let written = std::fs::create_dir_all(&folder)
            .map_err(|e| e.to_string())
            .and_then(|()| {
                let to = folder.join(dated(AUTOMATIC, now));
                let options = BackupOptions {
                    books: settings.books,
                };
                self.back_up(host, &to, options, true)
            });
        match written {
            Ok(_) => {
                self.folder_failed
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                prune(&folder, settings.keep.clamp(1, 100));
            }
            Err(error) => {
                if !self
                    .folder_failed
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    host.report("Could not write the automatic backup", error);
                }
            }
        }
    }

    /// Before a restore: the current library, saved where automatic backups go (else beside
    /// the library), so a restore can be undone.
    pub fn save_before_restore(&self, host: &dyn BackupHost) -> Result<BackupWritten, String> {
        let folder = host
            .settings()
            .folder
            .unwrap_or_else(|| host.paths().db_path.with_file_name("backups"));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let to = folder.join(dated(BEFORE_RESTORE, host.now_ms()));
        self.back_up(host, &to, BackupOptions::default(), true)
    }
}

/// A backup's file name: `prefix` and the local date and time.
fn dated(prefix: &str, at_ms: i64) -> String {
    let when = chrono::DateTime::from_timestamp_millis(at_ms)
        .unwrap_or_default()
        .with_timezone(&chrono::Local);
    format!("{prefix}{}.{EXTENSION}", when.format("%Y-%m-%d %H%M%S"))
}

/// The automatic backups in `folder`, with when each was written (Unix milliseconds).
fn automatic_backups(folder: &Path) -> Vec<(PathBuf, i64)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with(AUTOMATIC) && name.ends_with(&format!(".{EXTENSION}"))
        })
        .filter_map(|e| {
            let at = e.metadata().ok()?.modified().ok()?;
            let at = at.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64;
            Some((e.path(), at))
        })
        .collect()
}

/// Removes the oldest automatic backups beyond `keep` (never another file).
fn prune(folder: &Path, keep: u32) {
    let mut found = automatic_backups(folder);
    found.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    let mut removed = 0;
    for (path, _) in found.into_iter().skip(keep as usize) {
        if std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        tracing::info!(
            event = "backup.pruned",
            removed,
            "Removed the oldest automatic backups"
        );
    }
}

/// The host trait, for the app.
impl BackupHost for super::Core {
    fn paths(&self) -> &Paths {
        &self.paths
    }
    fn settings(&self) -> BackupSettings {
        self.app_settings().backup
    }
    fn now_ms(&self) -> i64 {
        super::now_ms()
    }
    fn emit(&self, event: CoreEvent) {
        super::Core::emit(self, event);
    }
    fn report(&self, doing: &str, error: String) {
        super::Core::report(self, "backup.written", doing, error);
    }
}

impl super::Core {
    /// Writes a backup of the library to `path`.
    pub fn back_up(&self, path: &Path, options: BackupOptions) -> Result<BackupWritten, String> {
        self.backups.back_up(self, path, options, false)
    }

    /// What the backup at `path` holds, and whether it can be restored here.
    pub fn inspect_backup(&self, path: &Path) -> Result<BackupInfoView, String> {
        let (manifest, schema) = inspect(path)?;
        let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        Ok(BackupInfoView {
            created_at: manifest.created_at,
            aoide_version: manifest.aoide_version,
            books_included: manifest.options.books,
            bytes,
            refused: (schema > Library::schema_version()).then(|| NEWER.to_owned()),
        })
    }

    /// Stages the backup at `path` to replace this library at the next start, after saving the
    /// current one. The caller restarts Aoide.
    pub fn restore(&self, path: &Path) -> Result<(), String> {
        stage(&self.paths, path)?;
        if let Err(error) = self.backups.save_before_restore(self) {
            let _ = std::fs::remove_dir_all(staging_dir(&self.paths));
            return Err(format!(
                "Could not save your current library before restoring, so nothing was changed: {error}"
            ));
        }
        Ok(())
    }

    /// Writes an automatic backup if one is due.
    pub(super) fn run_automatic_backup(&self) {
        self.backups.run_automatic(self);
    }

    /// The restore staged at the last start failed; the previous library is in use.
    pub fn restore_failed(&self, error: String) {
        self.report(
            "backup.restored",
            "Could not restore the backup; your previous library is still here",
            error,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_library::{BookId, ItemKind, NewBook};

    /// A home on "another computer": its folders, all under `root`.
    fn home(root: &Path) -> Paths {
        let data = root.join("data");
        let paths = Paths {
            home_dir: root.to_path_buf(),
            library_dir: root.join("Documents/Aoide/Books"),
            covers_dir: data.join("covers"),
            audio_dir: root.join("cache/audio"),
            db_path: data.join("library.sqlite3"),
            config_dir: root.join("config/aoide"),
            models_dir: data.join("models"),
            articles_dir: data.join("articles"),
            browser_dir: data.join("browser"),
            pictures_dir: data.join("pictures"),
        };
        for dir in [
            &paths.library_dir,
            &paths.covers_dir,
            &paths.articles_dir,
            &paths.config_dir,
            &paths.browser_dir,
        ] {
            std::fs::create_dir_all(dir).unwrap();
        }
        paths
    }

    /// A library with one book (its file, cover, a saved article and settings).
    fn filled(root: &Path) -> Paths {
        let paths = home(root);
        let book = paths.library_dir.join("Tides.epub");
        std::fs::write(&book, b"an epub's bytes").unwrap();
        let cover = paths.covers_dir.join("tides.jpg");
        std::fs::write(&cover, b"a cover").unwrap();
        std::fs::create_dir_all(paths.articles_dir.join("a1")).unwrap();
        std::fs::write(paths.articles_dir.join("a1/page.html"), b"<p>Saved</p>").unwrap();
        std::fs::write(paths.config_dir.join("settings.toml"), "[engine]\n").unwrap();
        std::fs::create_dir_all(paths.config_dir.join("themes")).unwrap();
        std::fs::write(paths.config_dir.join("themes/sea.toml"), "x = 1\n").unwrap();
        std::fs::write(paths.browser_dir.join("Cookies"), b"a session").unwrap();
        let library = Library::open(&paths.db_path).unwrap();
        library
            .upsert_book(
                &NewBook {
                    id: BookId("tides".into()),
                    kind: ItemKind::Book,
                    path: book,
                    title: Some("Tides".into()),
                    authors: vec![],
                    language: None,
                    cover_path: Some(cover),
                    error: None,
                },
                1,
            )
            .unwrap();
        paths
    }

    fn names(path: &Path) -> Vec<String> {
        let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        let mut names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_backup_holds_the_library_and_settings_and_never_the_browser_profile() {
        let (old, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let paths = filled(old.path());
        let file = out.path().join("library.aoide-backup");
        let mut steps = Vec::new();
        let (manifest, bytes) = write(&paths, &file, BackupOptions::default(), 7, |done, of| {
            steps.push((done, of))
        })
        .unwrap();
        assert!(bytes > 0);
        assert_eq!(steps.last().map(|(d, o)| d == o), Some(true));
        assert_eq!(
            names(&file),
            [
                "articles/a1/page.html",
                "config/settings.toml",
                "config/themes/sea.toml",
                "covers/tides.jpg",
                "library.sqlite3",
                "manifest.json",
            ]
        );
        assert_eq!(manifest.format, FORMAT);
        assert_eq!(manifest.schema_version, Library::schema_version());
        assert_eq!(manifest.from.library_dir, paths.library_dir);
        assert_eq!(manifest.created_at, 7);
        // With books, their files too; an existing file is not overwritten.
        let with = out.path().join("with-books.aoide-backup");
        write(&paths, &with, BackupOptions { books: true }, 7, |_, _| {}).unwrap();
        assert!(names(&with).contains(&"books/Tides.epub".to_owned()));
        assert!(write(&paths, &with, BackupOptions::default(), 7, |_, _| {}).is_err());
        let (read, schema) = inspect(&with).unwrap();
        assert_eq!(read.options, BackupOptions { books: true });
        assert_eq!(schema, Library::schema_version());
    }

    #[test]
    fn a_restore_on_another_computer_brings_the_library_back_with_its_paths_moved() {
        let (old, new, out) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let file = out.path().join("library.aoide-backup");
        write(
            &filled(old.path()),
            &file,
            BackupOptions { books: true },
            7,
            |_, _| {},
        )
        .unwrap();

        // A clean computer, whose library has something else in it.
        let paths = home(new.path());
        drop(Library::open(&paths.db_path).unwrap());
        assert_eq!(apply_staged(&paths), Ok(None));
        stage(&paths, &file).unwrap();
        let applied = apply_staged(&paths).unwrap().unwrap();
        assert_eq!(applied.created_at, 7);
        assert!(!staging_dir(&paths).exists());
        assert!(!aside_dir(&paths).exists());

        let library = Library::open(&paths.db_path).unwrap();
        let book = library.book(&BookId("tides".into())).unwrap().unwrap();
        assert_eq!(book.path, paths.library_dir.join("Tides.epub"));
        assert_eq!(book.cover_path, Some(paths.covers_dir.join("tides.jpg")));
        assert_eq!(std::fs::read(&book.path).unwrap(), b"an epub's bytes");
        assert!(paths.articles_dir.join("a1/page.html").exists());
        assert!(paths.config_dir.join("themes/sea.toml").exists());
        assert!(library.scanned_files().unwrap().is_empty());
    }

    #[test]
    fn a_restore_that_fails_part_way_leaves_the_previous_library_in_use() {
        let (old, new, out) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let file = out.path().join("library.aoide-backup");
        write(
            &filled(old.path()),
            &file,
            BackupOptions::default(),
            7,
            |_, _| {},
        )
        .unwrap();
        let paths = filled(new.path());
        std::fs::write(paths.config_dir.join("settings.toml"), "[mine]\n").unwrap();
        stage(&paths, &file).unwrap();
        // The staged database is broken: opening it fails after the files were swapped.
        std::fs::write(staging_dir(&paths).join(DATABASE), b"not a database").unwrap();
        assert!(apply_staged(&paths).is_err());
        let library = Library::open(&paths.db_path).unwrap();
        assert_eq!(library.books().unwrap().len(), 1);
        assert_eq!(
            std::fs::read_to_string(paths.config_dir.join("settings.toml")).unwrap(),
            "[mine]\n"
        );
        assert!(!aside_dir(&paths).exists());
        // Not tried again at the next start.
        assert_eq!(apply_staged(&paths), Ok(None));
    }

    #[test]
    fn a_start_that_stopped_part_way_puts_the_previous_library_back() {
        let (old, new, out) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let file = out.path().join("library.aoide-backup");
        write(
            &filled(old.path()),
            &file,
            BackupOptions::default(),
            7,
            |_, _| {},
        )
        .unwrap();
        let paths = filled(new.path());
        std::fs::write(paths.config_dir.join("settings.toml"), "[mine]\n").unwrap();
        stage(&paths, &file).unwrap();
        // Stopped after the settings were set aside and the backup's put in their place.
        let aside = aside_dir(&paths);
        std::fs::create_dir_all(&aside).unwrap();
        let settings = paths.config_dir.join("settings.toml");
        std::fs::rename(&settings, aside.join("config-settings.toml")).unwrap();
        std::fs::write(&settings, "[theirs]\n").unwrap();
        assert!(apply_staged(&paths).is_err());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "[mine]\n");
        assert_eq!(
            Library::open(&paths.db_path)
                .unwrap()
                .books()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(apply_staged(&paths), Ok(None));

        // Stopped after it was in place: nothing is undone.
        stage(&paths, &file).unwrap();
        std::fs::write(staging_dir(&paths).join(APPLIED), b"").unwrap();
        std::fs::create_dir_all(&aside).unwrap();
        assert_eq!(apply_staged(&paths), Ok(None));
        assert!(!aside.exists() && !staging_dir(&paths).exists());
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), "[mine]\n");
    }

    #[test]
    fn a_backup_from_a_newer_aoide_is_refused_and_a_changed_file_is_caught() {
        let (old, new, out) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let old = filled(old.path());
        // From a newer Aoide: its database has a later schema.
        rusqlite::Connection::open(&old.db_path)
            .unwrap()
            .pragma_update(None, "user_version", Library::schema_version() + 1)
            .unwrap();
        let file = out.path().join("newer.aoide-backup");
        write(&old, &file, BackupOptions::default(), 7, |_, _| {}).unwrap();
        let paths = home(new.path());
        let refused = stage(&paths, &file).unwrap_err();
        assert!(refused.contains("newer version of Aoide"), "{refused}");
        assert!(!staging_dir(&paths).exists());
        assert!(inspect(&out.path().join("none.aoide-backup")).is_err());
        // A file changed after the backup was written is caught when unpacking.
        let fine = out.path().join("fine.aoide-backup");
        let older = filled(tempfile::tempdir().unwrap().keep().as_path());
        write(&older, &fine, BackupOptions::default(), 7, |_, _| {}).unwrap();
        let (manifest, _) = inspect(&fine).unwrap();
        let changed = out.path().join("changed.aoide-backup");
        let mut zip = zip::ZipWriter::new(File::create(&changed).unwrap());
        let mut source = zip::ZipArchive::new(File::open(&fine).unwrap()).unwrap();
        for i in 0..source.len() {
            let mut entry = source.by_index(i).unwrap();
            let name = entry.name().to_owned();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            if name == "covers/tides.jpg" {
                bytes = b"another cover".to_vec();
            }
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap();
        assert!(manifest.files.iter().any(|f| f.path == "covers/tides.jpg"));
        let caught = stage(&paths, &changed).unwrap_err();
        assert!(caught.contains("damaged"), "{caught}");
        assert!(!staging_dir(&paths).exists());
    }

    struct Host {
        paths: Paths,
        settings: Mutex<BackupSettings>,
        now: std::sync::atomic::AtomicI64,
        progress: Mutex<Vec<BackupProgressView>>,
        told: Mutex<Vec<String>>,
    }

    impl Host {
        fn new(paths: Paths) -> Self {
            Self {
                paths,
                settings: Mutex::default(),
                now: std::sync::atomic::AtomicI64::new(1_700_000_000_000),
                progress: Mutex::default(),
                told: Mutex::default(),
            }
        }
    }

    impl BackupHost for Host {
        fn paths(&self) -> &Paths {
            &self.paths
        }
        fn settings(&self) -> BackupSettings {
            lock(&self.settings).clone()
        }
        fn now_ms(&self) -> i64 {
            self.now.load(std::sync::atomic::Ordering::SeqCst)
        }
        fn emit(&self, event: CoreEvent) {
            if let CoreEvent::BackupProgress(view) = event {
                lock(&self.progress).push(view);
            }
        }
        fn report(&self, doing: &str, _: String) {
            lock(&self.told).push(doing.to_owned());
        }
    }

    #[test]
    fn a_backup_tells_the_window_how_far_it_is() {
        let (root, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let host = Host::new(filled(root.path()));
        let to = out.path().join("mine.aoide-backup");
        let written = Backups::default()
            .back_up(&host, &to, BackupOptions::default(), false)
            .unwrap();
        assert_eq!(written.bytes, std::fs::metadata(&to).unwrap().len());
        let progress = lock(&host.progress);
        assert_eq!(progress.first().map(|p| p.done), Some(0));
        assert!(progress
            .last()
            .is_some_and(|p| p.finished && p.done == p.of));
    }

    #[test]
    fn automatic_backups_are_written_when_due_and_the_oldest_removed() {
        let (root, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let host = Host::new(filled(root.path()));
        let backups = Backups::default();
        // Off until a folder is chosen.
        backups.run_automatic(&host);
        assert!(std::fs::read_dir(out.path()).unwrap().next().is_none());
        let folder = out.path().join("Backups");
        *lock(&host.settings) = BackupSettings {
            folder: Some(folder.clone()),
            every_days: 1,
            keep: 2,
            books: false,
        };
        let count = || automatic_backups(&folder).len();
        backups.run_automatic(&host);
        assert_eq!(count(), 1);
        // Not due again until a day has passed (the file's time says when it was written).
        backups.run_automatic(&host);
        assert_eq!(count(), 1);
        let age = |days: u64| {
            for (path, _) in automatic_backups(&folder) {
                let file = File::options().write(true).open(path).unwrap();
                let at =
                    std::time::SystemTime::now() - std::time::Duration::from_secs(days * 86_400);
                file.set_modified(at).unwrap();
            }
        };
        for _ in 0..3 {
            age(2);
            host.now.store(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64
                    + count() as i64 * 1000,
                std::sync::atomic::Ordering::SeqCst,
            );
            backups.run_automatic(&host);
        }
        assert_eq!(count(), 2, "only the newest two are kept");
        // Another file in the folder is never removed.
        std::fs::write(folder.join("notes.txt"), b"mine").unwrap();
        age(2);
        backups.run_automatic(&host);
        assert!(folder.join("notes.txt").exists());
    }

    #[test]
    fn a_failing_folder_is_told_once_until_a_backup_works() {
        let (root, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let host = Host::new(filled(root.path()));
        let blocked = out.path().join("a file, not a folder");
        std::fs::write(&blocked, b"x").unwrap();
        *lock(&host.settings) = BackupSettings {
            folder: Some(blocked.join("Backups")),
            ..BackupSettings::default()
        };
        let backups = Backups::default();
        backups.run_automatic(&host);
        backups.run_automatic(&host);
        assert_eq!(lock(&host.told).len(), 1);
        lock(&host.settings).folder = Some(out.path().join("Backups"));
        backups.run_automatic(&host);
        lock(&host.settings).folder = Some(blocked.join("Backups"));
        backups.run_automatic(&host);
        assert_eq!(lock(&host.told).len(), 2);
    }

    #[tokio::test]
    async fn a_library_restored_without_its_books_finds_each_once_its_file_is_back() {
        use crate::core::article_tests::Pages;
        use crate::core::tests::{library_folder, paths, start_fetching};
        let engine = aoide_tts::testing::FakeEngine::new();
        let (old, new, out) = (
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
            tempfile::tempdir().unwrap(),
        );
        let id = library_folder(old.path(), "");
        let (core, _) = start_fetching(old.path(), &engine, Pages::with(&[]));
        core.rescan().unwrap();
        core.set_reading_place(&id, 1, "1:0:0").unwrap();
        let place = core.list_books().unwrap()[0].progress.clone();
        let file = out.path().join("mine.aoide-backup");
        core.back_up(&file, BackupOptions::default()).unwrap();
        let book = core.list_books().unwrap()[0].clone();
        let epub = std::fs::read(paths(old.path()).library_dir.join("book.epub"));
        core.shutdown();

        // Another computer: restored, without the book's file.
        let to = paths(new.path());
        std::fs::create_dir_all(&to.library_dir).unwrap();
        stage(&to, &file).unwrap();
        apply_staged(&to).unwrap().unwrap();
        let (core, _) = start_fetching(new.path(), &engine, Pages::with(&[]));
        core.rescan().unwrap();
        let listed = core.list_books().unwrap();
        assert_eq!(listed[0].id, book.id);
        assert_eq!(listed[0].status, aoide_library::BookStatus::Missing);
        assert_eq!(listed[0].progress, place, "the same place");

        // Its file is put in the library folder (under any name): found by its content.
        let bytes = epub.unwrap_or_else(|_| {
            std::fs::read_dir(paths(old.path()).library_dir)
                .unwrap()
                .flatten()
                .find(|e| e.path().extension().is_some_and(|x| x == "epub"))
                .map(|e| std::fs::read(e.path()).unwrap())
                .unwrap()
        });
        std::fs::write(to.library_dir.join("Copied over.epub"), bytes).unwrap();
        core.rescan().unwrap();
        let listed = core.list_books().unwrap();
        assert_eq!(listed[0].status, aoide_library::BookStatus::Present);
        assert_eq!(listed[0].progress, place);
        core.shutdown();
    }
}
