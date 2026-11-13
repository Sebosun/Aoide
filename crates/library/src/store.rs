use std::path::{Path, PathBuf};

use rusqlite::{Connection, Row};

use crate::{BookId, BookRecord, BookStatus, Library, LibraryError, Result};

impl Library {
    pub(super) fn initialize(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        let tx = conn.transaction()?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if !(0..=6).contains(&version) {
            return Err(LibraryError::Corrupt(format!(
                "unsupported schema version {version}"
            )));
        }
        if version == 0 {
            tx.execute_batch(
                "CREATE TABLE books (
                        id TEXT PRIMARY KEY NOT NULL,
                        path TEXT NOT NULL,
                        status TEXT NOT NULL CHECK(status IN ('present', 'missing', 'error')),
                        error TEXT,
                        title TEXT,
                        authors TEXT NOT NULL,
                        language TEXT,
                        cover_path TEXT,
                        added_at INTEGER NOT NULL,
                        last_opened_at INTEGER,
                        CHECK ((status = 'error') = (error IS NOT NULL))
                    );
                    CREATE TABLE progress (
                        book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                        chapter INTEGER NOT NULL CHECK(chapter >= 0),
                        char_offset INTEGER NOT NULL CHECK(char_offset >= 0),
                        segment_id TEXT,
                        updated_at INTEGER NOT NULL
                    );
                    CREATE TABLE segments (
                        key TEXT PRIMARY KEY NOT NULL,
                        book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                        file TEXT NOT NULL,
                        duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0),
                        bytes INTEGER NOT NULL CHECK(bytes >= 0),
                        word_timings TEXT,
                        created_at INTEGER NOT NULL
                    );
                    CREATE INDEX segments_book_id ON segments(book_id);
                    CREATE TABLE settings (
                        key TEXT PRIMARY KEY NOT NULL,
                        value TEXT NOT NULL
                    );",
            )?;
            tx.pragma_update(None, "user_version", 1)?;
        }
        if version <= 1 {
            tx.execute_batch(
                "CREATE TABLE bookmarks (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    chapter INTEGER NOT NULL CHECK(chapter >= 0),
                    char_offset INTEGER NOT NULL CHECK(char_offset >= 0),
                    segment_id TEXT,
                    excerpt TEXT NOT NULL,
                    note TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE listening (
                    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    day TEXT NOT NULL,
                    ms INTEGER NOT NULL CHECK(ms >= 0),
                    PRIMARY KEY (book_id, day)
                );
                CREATE TABLE book_details (
                    book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    json TEXT NOT NULL
                );
                CREATE INDEX segments_created_at ON segments(created_at);",
            )?;
            tx.pragma_update(None, "user_version", 2)?;
        }
        if version <= 2 {
            tx.execute_batch(
                "CREATE TABLE metadata_cache (
                    query TEXT PRIMARY KEY NOT NULL,
                    json TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );",
            )?;
            tx.pragma_update(None, "user_version", 3)?;
        }
        if version <= 3 {
            tx.execute_batch(
                "CREATE TABLE discovered_files (
                    path TEXT PRIMARY KEY NOT NULL,
                    size INTEGER NOT NULL,
                    modified_ns INTEGER NOT NULL,
                    inode INTEGER NOT NULL,
                    hash TEXT,
                    title TEXT,
                    authors TEXT NOT NULL,
                    language TEXT,
                    isbn TEXT,
                    cover_path TEXT,
                    error TEXT,
                    seen_at INTEGER NOT NULL
                );
                CREATE INDEX discovered_files_hash ON discovered_files(hash);",
            )?;
            tx.pragma_update(None, "user_version", 4)?;
        }
        if version <= 4 {
            tx.execute_batch(
                "CREATE TABLE collections (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE book_collections (
                    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                    PRIMARY KEY (book_id, collection_id)
                );
                CREATE INDEX book_collections_collection ON book_collections(collection_id);",
            )?;
            tx.pragma_update(None, "user_version", 5)?;
        }
        if version <= 5 {
            tx.execute_batch(
                "CREATE TABLE recordings (
                    book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    files TEXT NOT NULL,
                    total_ms INTEGER NOT NULL CHECK(total_ms >= 0),
                    stage TEXT NOT NULL,
                    error TEXT,
                    accepted INTEGER NOT NULL DEFAULT 0,
                    added_at INTEGER NOT NULL
                );
                CREATE TABLE recording_blocks (
                    book_id TEXT NOT NULL REFERENCES recordings(book_id) ON DELETE CASCADE,
                    start_ms INTEGER NOT NULL CHECK(start_ms >= 0),
                    end_ms INTEGER NOT NULL CHECK(end_ms >= start_ms),
                    transcriber TEXT NOT NULL,
                    words TEXT NOT NULL,
                    levels BLOB NOT NULL,
                    PRIMARY KEY (book_id, start_ms)
                );
                CREATE TABLE sync_maps (
                    book_id TEXT PRIMARY KEY NOT NULL REFERENCES recordings(book_id) ON DELETE CASCADE,
                    text_hash TEXT NOT NULL,
                    map BLOB NOT NULL,
                    report TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );",
            )?;
            tx.pragma_update(None, "user_version", 6)?;
        }
        tx.commit()?;
        Ok(Self { conn })
    }
}

pub(super) fn json(value: &impl serde::Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|error| LibraryError::Corrupt(error.to_string()))
}

pub(super) fn parse_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value).map_err(|error| LibraryError::Corrupt(error.to_string()))
}

pub(super) fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| LibraryError::Corrupt("path is not valid UTF-8".into()))
}

pub(super) fn read_book(row: &Row<'_>) -> Result<BookRecord> {
    let status: String = row.get("status")?;
    let status = match status.as_str() {
        "present" => BookStatus::Present,
        "missing" => BookStatus::Missing,
        "error" => BookStatus::Error(row.get("error")?),
        _ => {
            return Err(LibraryError::Corrupt(format!(
                "unknown book status {status}"
            )))
        }
    };
    Ok(BookRecord {
        id: BookId(row.get("id")?),
        path: row.get::<_, String>("path")?.into(),
        status,
        title: row.get("title")?,
        authors: parse_json(&row.get::<_, String>("authors")?)?,
        language: row.get("language")?,
        cover_path: row.get::<_, Option<String>>("cover_path")?.map(Into::into),
        added_at: row.get("added_at")?,
        last_opened_at: row.get("last_opened_at")?,
    })
}

pub(super) fn segment_files(conn: &Connection, id: &BookId) -> Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare("SELECT file FROM segments WHERE book_id = ?1 ORDER BY key")?;
    let files = stmt.query_map([&id.0], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?;
    Ok(files.collect::<std::result::Result<_, _>>()?)
}
