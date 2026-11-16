//! Aoide's persistent library: books identified by content hash, reading progress,
//! the generated-audio segment index, and key/value settings. SQLite-backed.
//!
//! Timestamps are Unix milliseconds supplied by the caller so behaviour is deterministic.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};

mod activity;
mod articles;
mod bookmarks;
mod collections;
mod discovered;
mod feeds;
mod listening;
mod merge;
mod recordings;
mod scanned;
mod segments;
mod sites;
mod store;
mod tags;
mod values;
pub use activity::{ActivityStats, BookActivity, DayActivity};
pub use articles::ArticleRow;
pub use bookmarks::Bookmark;
pub use collections::Collection;
pub use discovered::DiscoveredFile;
pub use feeds::{EntryListing, EntryRow, FeedRow, NewEntry};
pub use listening::{BookListening, DayListening, ListeningStats};
pub use recordings::{RecordingRow, StoredSyncMap, TranscriptBlock};
pub use segments::{AudioUsage, CachedSegment, WordTiming};
pub use sites::SiteRow;
pub use tags::{tidy_tags, TagCount, TagSource};
pub use values::{BookKey, GlobalKey};

/// Lower-case hex SHA-256 of the EPUB file bytes.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BookId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BookStatus {
    Present,
    Missing,
    /// The file exists but could not be parsed; message is user-facing.
    Error(String),
}

/// What an import or scan knows about a book file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewBook {
    pub id: BookId,
    pub kind: ItemKind,
    pub path: PathBuf,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub cover_path: Option<PathBuf>,
    /// `Some` marks the book `BookStatus::Error`.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookRecord {
    pub id: BookId,
    pub path: PathBuf,
    pub status: BookStatus,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub cover_path: Option<PathBuf>,
    pub added_at: i64,
    pub last_opened_at: Option<i64>,
    /// When the reader finished it; `None` while unread or in progress.
    pub finished_at: Option<i64>,
    /// Its length in words, once it has been parsed.
    pub word_count: Option<u32>,
    pub kind: ItemKind,
    /// `library`, or `feed` for an entry opened from a feed and not saved.
    pub shelf: String,
}

/// What a library item is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ItemKind {
    /// An EPUB, possibly with a recording paired to it.
    #[default]
    Book,
    /// A recording with no text (`path` is its M4B or its folder).
    Audiobook,
    /// A web article (phase 4).
    Article,
}

impl ItemKind {
    pub(crate) fn column(self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Audiobook => "audiobook",
            Self::Article => "article",
        }
    }

    pub(crate) fn from_column(text: &str) -> Result<Self> {
        match text {
            "book" => Ok(Self::Book),
            "audiobook" => Ok(Self::Audiobook),
            "article" => Ok(Self::Article),
            other => Err(LibraryError::Corrupt(format!("unknown item kind {other}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub chapter: u32,
    /// Character (Unicode scalar) offset into the chapter's display text.
    pub char_offset: u32,
    /// Segment ID hint; the char offset is authoritative.
    pub segment_id: Option<String>,
    pub updated_at: i64,
    /// For an audiobook, the place on its recording's timeline (`chapter` is the part,
    /// `char_offset` 0); `None` for text.
    pub position_ms: Option<u64>,
}

/// An EPUB found by [`scan_folder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub id: BookId,
    /// Its size in bytes, when it was hashed.
    pub size: u64,
    /// Its modification time in nanoseconds since the Unix epoch, when it was hashed.
    pub modified_ns: i64,
}

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("stored data is invalid: {0}")]
    Corrupt(String),
    /// A request the library refuses, such as a duplicate collection name.
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, LibraryError>;

pub struct Library {
    conn: rusqlite::Connection,
}

impl Library {
    /// Opens (creating if needed) the database file and applies pending migrations.
    pub fn open(path: &Path) -> Result<Self> {
        Self::initialize(rusqlite::Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::initialize(rusqlite::Connection::open_in_memory()?)
    }

    /// Inserts a new book, or updates path/metadata/status of an existing one with the
    /// same id. `added_at` is kept from the first insert; `now` is used only on insert.
    pub fn upsert_book(&self, book: &NewBook, now: i64) -> Result<()> {
        let authors = store::json(&book.authors)?;
        self.conn.execute(
            "INSERT INTO books
             (id, path, status, error, title, authors, language, cover_path, added_at, kind)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
             path = excluded.path, status = excluded.status, error = excluded.error,
             title = excluded.title, authors = excluded.authors,
             language = excluded.language, cover_path = excluded.cover_path,
             kind = excluded.kind",
            rusqlite::params![
                book.id.0,
                store::path_text(&book.path)?,
                if book.error.is_some() {
                    "error"
                } else {
                    "present"
                },
                book.error,
                book.title,
                authors,
                book.language,
                book.cover_path
                    .as_deref()
                    .map(store::path_text)
                    .transpose()?,
                now,
                book.kind.column(),
            ],
        )?;
        Ok(())
    }

    /// Sets (or with `None`, clears) item `id`'s cover file, leaving the rest of it as it is.
    pub fn set_cover(&self, id: &BookId, cover: Option<&Path>) -> Result<()> {
        self.conn.execute(
            "UPDATE books SET cover_path = ?2 WHERE id = ?1",
            rusqlite::params![id.0, cover.map(store::path_text).transpose()?],
        )?;
        Ok(())
    }

    /// All books, most recently opened first, then most recently added.
    /// The items in the library: not those on the feed shelf.
    pub fn books(&self) -> Result<Vec<BookRecord>> {
        self.books_on("library")
    }

    /// Entries opened from feeds and not saved.
    pub fn feed_shelf_books(&self) -> Result<Vec<BookRecord>> {
        self.books_on("feed")
    }

    fn books_on(&self, shelf: &str) -> Result<Vec<BookRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM books WHERE shelf = ?1
             ORDER BY last_opened_at DESC, added_at DESC, id ASC",
        )?;
        let mut rows = stmt.query([shelf])?;
        let mut books = Vec::new();
        while let Some(row) = rows.next()? {
            books.push(store::read_book(row)?);
        }
        Ok(books)
    }

    /// Puts item `id` on `shelf` (`library` or `feed`).
    pub fn set_shelf(&self, id: &BookId, shelf: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE books SET shelf = ?2 WHERE id = ?1",
            rusqlite::params![id.0, shelf],
        )?;
        Ok(())
    }

    pub fn book(&self, id: &BookId) -> Result<Option<BookRecord>> {
        let mut stmt = self.conn.prepare("SELECT * FROM books WHERE id = ?1")?;
        let mut rows = stmt.query([&id.0])?;
        rows.next()?.map(store::read_book).transpose()
    }

    /// A folder moved from `from` to `to`: rewrites stored book, cover and found-file paths
    /// inside it. Returns how many paths changed.
    pub fn move_paths(&self, from: &Path, to: &Path) -> Result<usize> {
        let (from, to) = (from.to_string_lossy(), to.to_string_lossy());
        let tx = self.conn.unchecked_transaction()?;
        let mut changed = 0;
        for (table, column) in [
            ("books", "path"),
            ("books", "cover_path"),
            ("discovered_files", "path"),
            ("discovered_files", "cover_path"),
        ] {
            changed += tx.execute(
                &format!(
                    "UPDATE {table} SET {column} = ?2 || substr({column}, length(?1) + 1)
                     WHERE {column} = ?1 OR substr({column}, 1, length(?1) + 1) = ?1 || '/'"
                ),
                [from.as_ref(), to.as_ref()],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Marks every `Present` book whose id is not in `present` as `Missing`.
    /// Returns the ids that changed. `Error` books are left alone.
    pub fn mark_missing_except(&self, present: &[BookId]) -> Result<Vec<BookId>> {
        let present: std::collections::HashSet<_> = present.iter().collect();
        let tx = self.conn.unchecked_transaction()?;
        let changed = {
            // Only EPUBs live in the library folder; other items are checked where they are.
            let mut stmt =
                tx.prepare("SELECT id FROM books WHERE status = ?1 AND kind = 'book' ORDER BY id")?;
            let ids = stmt.query_map(["present"], |row| Ok(BookId(row.get(0)?)))?;
            let mut changed = Vec::new();
            for id in ids {
                let id = id?;
                if !present.contains(&id) {
                    changed.push(id);
                }
            }
            changed
        };
        for id in &changed {
            tx.execute(
                "UPDATE books SET status = ?1 WHERE id = ?2",
                rusqlite::params!["missing", id.0],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// Marks an item missing, or present again; an item that could not be read stays so.
    /// Returns whether its status changed.
    pub fn set_missing(&self, id: &BookId, missing: bool) -> Result<bool> {
        let (to, from) = if missing {
            ("missing", "present")
        } else {
            ("present", "missing")
        };
        Ok(self.conn.execute(
            "UPDATE books SET status = ?1 WHERE id = ?2 AND status = ?3",
            rusqlite::params![to, id.0, from],
        )? > 0)
    }

    pub fn set_last_opened(&self, id: &BookId, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE books SET last_opened_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id.0],
        )?;
        Ok(())
    }

    /// Deletes the book, its progress, segment index rows, bookmarks, listening rows and details.
    /// Returns the cache-relative files of the removed segments so the caller can delete them.
    pub fn remove_book(&self, id: &BookId) -> Result<Vec<PathBuf>> {
        let tx = self.conn.unchecked_transaction()?;
        let files = store::segment_files(&tx, id)?;
        tx.execute("DELETE FROM books WHERE id = ?1", [&id.0])?;
        tx.commit()?;
        Ok(files)
    }

    pub fn save_progress(&self, id: &BookId, progress: &Progress) -> Result<()> {
        self.conn.execute(
            "INSERT INTO progress (book_id, chapter, char_offset, segment_id, updated_at, position_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(book_id) DO UPDATE SET chapter = excluded.chapter,
             char_offset = excluded.char_offset, segment_id = excluded.segment_id,
             updated_at = excluded.updated_at, position_ms = excluded.position_ms",
            rusqlite::params![
                id.0,
                progress.chapter,
                progress.char_offset,
                progress.segment_id,
                progress.updated_at,
                progress.position_ms,
            ],
        )?;
        Ok(())
    }

    pub fn progress(&self, id: &BookId) -> Result<Option<Progress>> {
        Ok(self
            .conn
            .query_row(
                "SELECT chapter, char_offset, segment_id, updated_at, position_ms FROM progress
             WHERE book_id = ?1",
                [&id.0],
                |row| {
                    Ok(Progress {
                        chapter: row.get(0)?,
                        char_offset: row.get(1)?,
                        segment_id: row.get(2)?,
                        updated_at: row.get(3)?,
                        position_ms: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Every book's place, by book.
    pub fn all_progress(&self) -> Result<std::collections::HashMap<BookId, Progress>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, chapter, char_offset, segment_id, updated_at, position_ms FROM progress",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                BookId(row.get(0)?),
                Progress {
                    chapter: row.get(1)?,
                    char_offset: row.get(2)?,
                    segment_id: row.get(3)?,
                    updated_at: row.get(4)?,
                    position_ms: row.get(5)?,
                },
            ))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- schema v2: bookmarks, listening, details, cache eviction ----

    pub fn book_details(&self, id: &BookId) -> Result<Option<BookDetails>> {
        let json: Option<String> = self
            .conn
            .query_row(
                "SELECT json FROM book_details WHERE book_id = ?1",
                [&id.0],
                |row| row.get(0),
            )
            .optional()?;
        json.as_deref().map(store::parse_json).transpose()
    }

    pub fn set_book_details(&self, id: &BookId, details: &BookDetails) -> Result<()> {
        self.conn.execute(
            "INSERT INTO book_details (book_id, json) VALUES (?1, ?2)
             ON CONFLICT(book_id) DO UPDATE SET json = excluded.json",
            rusqlite::params![id.0, store::json(details)?],
        )?;
        Ok(())
    }

    // ---- schema v5: collections ----

    fn collection_name(name: &str) -> Result<&str> {
        let name = name.trim();
        if name.is_empty() {
            return Err(LibraryError::Invalid("a collection needs a name".into()));
        }
        Ok(name)
    }

    fn unique_name(error: rusqlite::Error, name: &str) -> LibraryError {
        match error {
            rusqlite::Error::SqliteFailure(e, _)
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                LibraryError::Invalid(format!("there is already a collection called “{name}”"))
            }
            other => other.into(),
        }
    }

    // ---- schema v4: files found by scanning the computer ----

    // ---- schema v3: cached online lookups ----
}

/// [`BookKey::PendingPlace`]'s key in `book_values`.
const PENDING_PLACE: &str = "pending_place";

/// Extra metadata from the EPUB or an online lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BookDetails {
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub isbn: Option<String>,
    pub published: Option<String>,
    pub subjects: Vec<String>,
    /// Open Library work or edition key (e.g. "/works/OL45883W") once the user picked a match.
    pub open_library_key: Option<String>,
}

/// Lower-case hex SHA-256 of `bytes`.
pub fn content_hash(bytes: &[u8]) -> BookId {
    BookId(format!("{:x}", Sha256::digest(bytes)))
}

/// Recursively finds `*.epub` files (case-insensitive extension) under `dir`, hashing each.
/// Hidden files/directories (name starting with '.') are skipped. Results are sorted by path.
/// Unreadable individual files are skipped; a missing `dir` yields an empty list.
/// The EPUBs under `dir` (hidden files and folders left out), by path, each with the id its
/// content hashes to. A file `known` from an earlier scan, with the same size and modification
/// time, keeps the id it had there without being read again.
pub fn scan_folder(dir: &Path, known: &[ScannedFile]) -> Result<Vec<ScannedFile>> {
    let known: std::collections::HashMap<&Path, &ScannedFile> =
        known.iter().map(|f| (f.path.as_path(), f)).collect();
    let mut found = Vec::new();
    let entries = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0 || !entry.file_name().as_encoded_bytes().starts_with(b".")
        });
    for entry in entries.filter_map(std::result::Result::ok) {
        if !entry.file_type().is_file()
            || !entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("epub"))
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let size = metadata.len();
        let modified_ns = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos() as i64);
        let id = match known.get(entry.path()) {
            Some(was) if was.size == size && was.modified_ns == modified_ns => was.id.clone(),
            _ => match std::fs::read(entry.path()) {
                Ok(bytes) => content_hash(&bytes),
                Err(_) => continue,
            },
        };
        found.push(ScannedFile {
            path: entry.into_path(),
            id,
            size,
            modified_ns,
        });
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

const DISCOVERED_COLUMNS: &str =
    "path, size, modified_ns, inode, hash, title, authors, language, isbn, cover_path, error, seen_at";

fn discovered_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Result<DiscoveredFile>> {
    let authors: String = row.get(6)?;
    let path: String = row.get(0)?;
    let cover: Option<String> = row.get(9)?;
    let size: i64 = row.get(1)?;
    let inode: i64 = row.get(3)?;
    Ok((|| {
        Ok(DiscoveredFile {
            path: PathBuf::from(path),
            size: size as u64,
            modified_ns: row.get(2)?,
            inode: inode as u64,
            hash: row.get(4)?,
            title: row.get(5)?,
            authors: store::parse_json(&authors)?,
            language: row.get(7)?,
            isbn: row.get(8)?,
            cover_path: cover.map(PathBuf::from),
            error: row.get(10)?,
            seen_at: row.get(11)?,
        })
    })())
}
