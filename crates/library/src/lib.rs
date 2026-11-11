//! Aoide's persistent library: books identified by content hash, reading progress,
//! the generated-audio segment index, and key/value settings. SQLite-backed.
//!
//! Timestamps are Unix milliseconds supplied by the caller so behaviour is deterministic.

use std::path::{Path, PathBuf};

use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};

mod store;

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
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordTiming {
    pub start_ms: u32,
    pub end_ms: u32,
}

/// A user-made shelf of books.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    /// Books on it.
    pub books: u64,
}

/// Generated audio indexed for one book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioUsage {
    pub book_id: BookId,
    pub bytes: u64,
    pub segments: u64,
    /// `created_at` of the newest segment.
    pub newest_at: i64,
}

/// Index entry for one generated audio segment stored in the audio cache directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSegment {
    /// Hex cache key (hash of speech text + engine identity + settings).
    pub key: String,
    pub book_id: BookId,
    /// Path relative to the audio cache directory.
    pub file: PathBuf,
    pub duration_ms: u64,
    pub bytes: u64,
    pub word_timings: Option<Vec<WordTiming>>,
    pub created_at: i64,
}

/// An EPUB found by [`scan_folder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub id: BookId,
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
    #[allow(dead_code)]
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
             (id, path, status, error, title, authors, language, cover_path, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
             path = excluded.path, status = excluded.status, error = excluded.error,
             title = excluded.title, authors = excluded.authors,
             language = excluded.language, cover_path = excluded.cover_path",
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
            ],
        )?;
        Ok(())
    }

    /// All books, most recently opened first, then most recently added.
    pub fn books(&self) -> Result<Vec<BookRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM books ORDER BY last_opened_at DESC, added_at DESC, id ASC")?;
        let mut rows = stmt.query([])?;
        let mut books = Vec::new();
        while let Some(row) = rows.next()? {
            books.push(store::read_book(row)?);
        }
        Ok(books)
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
            let mut stmt = tx.prepare("SELECT id FROM books WHERE status = ?1 ORDER BY id")?;
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
            "INSERT INTO progress (book_id, chapter, char_offset, segment_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(book_id) DO UPDATE SET chapter = excluded.chapter,
             char_offset = excluded.char_offset, segment_id = excluded.segment_id,
             updated_at = excluded.updated_at",
            rusqlite::params![
                id.0,
                progress.chapter,
                progress.char_offset,
                progress.segment_id,
                progress.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn progress(&self, id: &BookId) -> Result<Option<Progress>> {
        Ok(self.conn.query_row(
            "SELECT chapter, char_offset, segment_id, updated_at FROM progress WHERE book_id = ?1",
            [&id.0],
            |row| Ok(Progress {
                chapter: row.get(0)?,
                char_offset: row.get(1)?,
                segment_id: row.get(2)?,
                updated_at: row.get(3)?,
            }),
        ).optional()?)
    }

    /// Inserts or replaces the segment with the same key.
    pub fn record_segment(&self, segment: &CachedSegment) -> Result<()> {
        let timings = segment.word_timings.as_ref().map(store::json).transpose()?;
        self.conn.execute(
            "INSERT INTO segments (key, book_id, file, duration_ms, bytes, word_timings, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(key) DO UPDATE SET book_id = excluded.book_id,
             file = excluded.file, duration_ms = excluded.duration_ms, bytes = excluded.bytes,
             word_timings = excluded.word_timings, created_at = excluded.created_at",
            rusqlite::params![segment.key, segment.book_id.0, store::path_text(&segment.file)?,
                segment.duration_ms, segment.bytes, timings, segment.created_at],
        )?;
        Ok(())
    }

    pub fn segment(&self, key: &str) -> Result<Option<CachedSegment>> {
        let mut stmt = self.conn.prepare("SELECT * FROM segments WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let timings: Option<String> = row.get("word_timings")?;
        Ok(Some(CachedSegment {
            key: row.get("key")?,
            book_id: BookId(row.get("book_id")?),
            file: row.get::<_, String>("file")?.into(),
            duration_ms: row.get("duration_ms")?,
            bytes: row.get("bytes")?,
            word_timings: timings.as_deref().map(store::parse_json).transpose()?,
            created_at: row.get("created_at")?,
        }))
    }

    /// Which of `keys` are present in the index (order of `keys` preserved).
    pub fn cached_keys(&self, keys: &[String]) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT EXISTS(SELECT 1 FROM segments WHERE key = ?1)")?;
        let mut cached = Vec::new();
        for key in keys {
            if stmt.query_row([key], |row| row.get::<_, bool>(0))? {
                cached.push(key.clone());
            }
        }
        Ok(cached)
    }

    /// Total `bytes` of all segments recorded for the book.
    pub fn book_audio_bytes(&self, id: &BookId) -> Result<u64> {
        let mut stmt = self
            .conn
            .prepare("SELECT bytes FROM segments WHERE book_id = ?1")?;
        let mut rows = stmt.query([&id.0])?;
        let mut total = 0_u64;
        while let Some(row) = rows.next()? {
            total = total
                .checked_add(row.get(0)?)
                .ok_or_else(|| LibraryError::Corrupt("book audio byte total exceeds u64".into()))?;
        }
        Ok(total)
    }

    /// Removes the book's segment rows, returning their cache-relative files.
    pub fn delete_book_segments(&self, id: &BookId) -> Result<Vec<PathBuf>> {
        let tx = self.conn.unchecked_transaction()?;
        let files = store::segment_files(&tx, id)?;
        tx.execute("DELETE FROM segments WHERE book_id = ?1", [&id.0])?;
        tx.commit()?;
        Ok(files)
    }

    // ---- schema v2: bookmarks, listening, details, cache eviction ----

    /// Adds a bookmark and returns its id.
    #[allow(clippy::too_many_arguments)]
    pub fn add_bookmark(
        &self,
        book: &BookId,
        chapter: u32,
        char_offset: u32,
        segment_id: Option<&str>,
        excerpt: &str,
        note: &str,
        now: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO bookmarks (book_id, chapter, char_offset, segment_id, excerpt, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![book.0, chapter, char_offset, segment_id, excerpt, note, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// The book's bookmarks ordered by chapter, then offset, then creation time.
    pub fn bookmarks(&self, book: &BookId) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, chapter, char_offset, segment_id, excerpt, note, created_at
             FROM bookmarks WHERE book_id = ?1 ORDER BY chapter, char_offset, created_at",
        )?;
        let rows = stmt.query_map([&book.0], |row| {
            Ok(Bookmark {
                id: row.get(0)?,
                book_id: BookId(row.get(1)?),
                chapter: row.get(2)?,
                char_offset: row.get(3)?,
                segment_id: row.get(4)?,
                excerpt: row.get(5)?,
                note: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Returns false when no bookmark has that id.
    pub fn update_bookmark_note(&self, id: i64, note: &str) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE bookmarks SET note = ?1 WHERE id = ?2",
            rusqlite::params![note, id],
        )? != 0)
    }

    pub fn delete_bookmark(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM bookmarks WHERE id = ?1", [id])?
            != 0)
    }

    /// Adds listened time to the book's total for a local day (`YYYY-MM-DD`).
    pub fn record_listening(&self, book: &BookId, day: &str, ms: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO listening (book_id, day, ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(book_id, day) DO UPDATE SET ms = listening.ms + excluded.ms",
            rusqlite::params![book.0, day, ms],
        )?;
        Ok(())
    }

    pub fn listening_stats(&self) -> Result<ListeningStats> {
        let total_ms =
            self.conn
                .query_row("SELECT COALESCE(SUM(ms), 0) FROM listening", [], |row| {
                    row.get(0)
                })?;
        let mut stmt = self.conn.prepare(
            "SELECT book_id, SUM(ms) AS total FROM listening GROUP BY book_id
             ORDER BY total DESC, book_id ASC",
        )?;
        let by_book = stmt
            .query_map([], |row| {
                Ok(BookListening {
                    book_id: BookId(row.get(0)?),
                    ms: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        let mut stmt = self
            .conn
            .prepare("SELECT day, SUM(ms) FROM listening GROUP BY day ORDER BY day ASC")?;
        let by_day = stmt
            .query_map([], |row| {
                Ok(DayListening {
                    day: row.get(0)?,
                    ms: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(ListeningStats {
            total_ms,
            by_book,
            by_day,
        })
    }

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

    /// Sum of `bytes` over every recorded segment.
    pub fn total_audio_bytes(&self) -> Result<u64> {
        let mut stmt = self.conn.prepare("SELECT bytes FROM segments")?;
        let mut rows = stmt.query([])?;
        let mut total = 0_u64;
        while let Some(row) = rows.next()? {
            total = total
                .checked_add(row.get(0)?)
                .ok_or_else(|| LibraryError::Corrupt("audio byte total exceeds u64".into()))?;
        }
        Ok(total)
    }

    /// Indexed audio per book that has any, largest first (ties by id).
    pub fn audio_usage(&self) -> Result<Vec<AudioUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, SUM(bytes), COUNT(*), MAX(created_at) FROM segments
             GROUP BY book_id ORDER BY SUM(bytes) DESC, book_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AudioUsage {
                book_id: BookId(row.get(0)?),
                bytes: row.get(1)?,
                segments: row.get(2)?,
                newest_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Cache-relative files of every indexed segment.
    pub fn segment_files(&self) -> Result<std::collections::HashSet<PathBuf>> {
        let mut stmt = self.conn.prepare("SELECT file FROM segments")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut files = std::collections::HashSet::new();
        for file in rows {
            files.insert(PathBuf::from(file?));
        }
        Ok(files)
    }

    /// Up to `limit` segments, oldest `created_at` first (ties by key), skipping `except` book.
    pub fn oldest_segments(
        &self,
        except: Option<&BookId>,
        limit: usize,
    ) -> Result<Vec<CachedSegment>> {
        let mut stmt = self.conn.prepare(
            "SELECT key FROM segments WHERE (?1 IS NULL OR book_id != ?1)
             ORDER BY created_at ASC, key ASC LIMIT ?2",
        )?;
        let keys = stmt.query_map(
            rusqlite::params![
                except.map(|id| &id.0),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut segments = Vec::new();
        for key in keys {
            if let Some(segment) = self.segment(&key?)? {
                segments.push(segment);
            }
        }
        Ok(segments)
    }

    /// Deletes index rows for these keys; unknown keys are ignored.
    pub fn delete_segments(&self, keys: &[String]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM segments WHERE key = ?1")?;
            for key in keys {
                stmt.execute([key])?;
            }
        }
        tx.commit()?;
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

    /// Creates a collection and returns its id. Names are trimmed and unique, ignoring case.
    pub fn create_collection(&self, name: &str, now: i64) -> Result<i64> {
        let name = Self::collection_name(name)?;
        self.conn
            .execute(
                "INSERT INTO collections (name, created_at) VALUES (?1, ?2)",
                rusqlite::params![name, now],
            )
            .map_err(|e| Self::unique_name(e, name))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> Result<()> {
        let name = Self::collection_name(name)?;
        let changed = self
            .conn
            .execute(
                "UPDATE collections SET name = ?1 WHERE id = ?2",
                rusqlite::params![name, id],
            )
            .map_err(|e| Self::unique_name(e, name))?;
        if changed == 0 {
            return Err(LibraryError::Invalid("no such collection".into()));
        }
        Ok(())
    }

    /// Deletes the collection; its books stay in the library.
    pub fn delete_collection(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM collections WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Every collection with its number of books, by name.
    pub fn collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.created_at, COUNT(bc.book_id) FROM collections c
             LEFT JOIN book_collections bc ON bc.collection_id = c.id
             GROUP BY c.id ORDER BY c.name COLLATE NOCASE, c.id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                books: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Puts the book on exactly these collections (duplicates ignored).
    pub fn set_book_collections(&self, book: &BookId, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM book_collections WHERE book_id = ?1", [&book.0])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO book_collections (book_id, collection_id) VALUES (?1, ?2)",
            )?;
            for id in ids {
                stmt.execute(rusqlite::params![book.0, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The collections of every book that is on any, each list ordered by collection name.
    pub fn book_collections(&self) -> Result<std::collections::HashMap<BookId, Vec<i64>>> {
        let mut stmt = self.conn.prepare(
            "SELECT bc.book_id, bc.collection_id FROM book_collections bc
             JOIN collections c ON c.id = bc.collection_id
             ORDER BY c.name COLLATE NOCASE, c.id",
        )?;
        let rows = stmt.query_map([], |row| Ok((BookId(row.get(0)?), row.get::<_, i64>(1)?)))?;
        let mut shelves = std::collections::HashMap::<BookId, Vec<i64>>::new();
        for row in rows {
            let (book, id) = row?;
            shelves.entry(book).or_default().push(id);
        }
        Ok(shelves)
    }

    // ---- schema v4: files found by scanning the computer ----

    pub fn discovered(&self, path: &Path) -> Result<Option<DiscoveredFile>> {
        self.conn
            .query_row(
                &format!("SELECT {DISCOVERED_COLUMNS} FROM discovered_files WHERE path = ?1"),
                [store::path_text(path)?],
                discovered_row,
            )
            .optional()?
            .transpose()
    }

    /// Every remembered file, by path.
    pub fn discovered_files(&self) -> Result<Vec<DiscoveredFile>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {DISCOVERED_COLUMNS} FROM discovered_files ORDER BY path"
        ))?;
        let rows = stmt.query_map([], discovered_row)?;
        rows.map(|row| row?).collect()
    }

    pub fn upsert_discovered(&self, file: &DiscoveredFile) -> Result<()> {
        self.conn.execute(
            "INSERT INTO discovered_files
             (path, size, modified_ns, inode, hash, title, authors, language, isbn, cover_path, error, seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(path) DO UPDATE SET size = excluded.size, modified_ns = excluded.modified_ns,
             inode = excluded.inode, hash = excluded.hash, title = excluded.title,
             authors = excluded.authors, language = excluded.language, isbn = excluded.isbn,
             cover_path = excluded.cover_path, error = excluded.error, seen_at = excluded.seen_at",
            rusqlite::params![
                store::path_text(&file.path)?,
                file.size as i64,
                file.modified_ns,
                file.inode as i64,
                file.hash,
                file.title,
                store::json(&file.authors)?,
                file.language,
                file.isbn,
                file.cover_path.as_deref().map(store::path_text).transpose()?,
                file.error,
                file.seen_at,
            ],
        )?;
        Ok(())
    }

    /// Records that unchanged files were seen again.
    pub fn mark_discovered_seen(&self, paths: &[PathBuf], seen_at: i64) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt =
                tx.prepare("UPDATE discovered_files SET seen_at = ?1 WHERE path = ?2")?;
            for path in paths {
                stmt.execute(rusqlite::params![seen_at, store::path_text(path)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Forgets files not seen since `seen_at` (they were moved or deleted). Returns how many.
    pub fn forget_discovered_before(&self, seen_at: i64) -> Result<usize> {
        Ok(self
            .conn
            .execute("DELETE FROM discovered_files WHERE seen_at < ?1", [seen_at])?)
    }

    // ---- schema v3: cached online lookups ----

    /// A stored lookup result and when it was fetched, by normalised query.
    pub fn cached_lookup(&self, query: &str) -> Result<Option<(String, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT json, fetched_at FROM metadata_cache WHERE query = ?1",
                [query],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    /// Stores (or replaces) a lookup result.
    pub fn store_lookup(&self, query: &str, json: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO metadata_cache (query, json, fetched_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(query) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
            rusqlite::params![query, json, now],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}

/// A saved position with an optional note.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Bookmark {
    pub id: i64,
    pub book_id: BookId,
    pub chapter: u32,
    pub char_offset: u32,
    pub segment_id: Option<String>,
    /// Short excerpt of the text at the position, for display.
    pub excerpt: String,
    pub note: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BookListening {
    pub book_id: BookId,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DayListening {
    /// Local calendar day, `YYYY-MM-DD`.
    pub day: String,
    pub ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ListeningStats {
    pub total_ms: u64,
    /// Most listened first.
    pub by_book: Vec<BookListening>,
    /// Oldest day first.
    pub by_day: Vec<DayListening>,
}

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
pub fn scan_folder(dir: &Path) -> Result<Vec<ScannedFile>> {
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
        if let Ok(bytes) = std::fs::read(entry.path()) {
            found.push(ScannedFile {
                path: entry.into_path(),
                id: content_hash(&bytes),
            });
        }
    }
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

/// An EPUB found by scanning the computer, with what was learned about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredFile {
    pub path: PathBuf,
    pub size: u64,
    /// Modification time in nanoseconds since the Unix epoch.
    pub modified_ns: i64,
    pub inode: u64,
    /// Content hash (the book id it would get); `None` when the file could not be read.
    pub hash: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub isbn: Option<String>,
    pub cover_path: Option<PathBuf>,
    /// Why the file could not be read as an EPUB.
    pub error: Option<String>,
    pub seen_at: i64,
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
