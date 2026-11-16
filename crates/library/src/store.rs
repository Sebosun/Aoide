use std::path::{Path, PathBuf};

use rusqlite::{Connection, Row};

use crate::{BookId, BookRecord, BookStatus, ItemKind, Library, LibraryError, Result};

/// One step of the schema: SQL, or Rust where SQL alone cannot say it.
enum Step {
    Sql(&'static str),
    Run(fn(&rusqlite::Transaction) -> rusqlite::Result<()>),
}

/// The schema, one step per version: step `n` (counting from one) takes a database at
/// version `n - 1` to version `n`. A new version is a new step at the end; a step that has
/// shipped is never edited, since databases that already ran it would not run it again.
const MIGRATIONS: &[Step] = &[
    // Version 1.
    Step::Sql("CREATE TABLE books (
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
                    );"),
    // Version 2.
    Step::Sql("CREATE TABLE bookmarks (
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
                CREATE INDEX segments_created_at ON segments(created_at);"),
    // Version 3.
    Step::Sql("CREATE TABLE metadata_cache (
                    query TEXT PRIMARY KEY NOT NULL,
                    json TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL
                );"),
    // Version 4.
    Step::Sql("CREATE TABLE discovered_files (
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
                CREATE INDEX discovered_files_hash ON discovered_files(hash);"),
    // Version 5.
    Step::Sql("CREATE TABLE collections (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE book_collections (
                    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
                    PRIMARY KEY (book_id, collection_id)
                );
                CREATE INDEX book_collections_collection ON book_collections(collection_id);"),
    // Version 6.
    Step::Sql("CREATE TABLE recordings (
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
                );"),
    // Version 7.
    // Values that belong to a book and go with it when it is removed. Asked on
    // 2026-11-15, the user chose that a book removed and added again keeps its voice
    // settings and its place, so `BookKey::Settings` and `BookKey::ProgressFraction`
    // stay in `settings`; only the prepared mark moves here.
    Step::Sql("CREATE TABLE book_values (
                    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    PRIMARY KEY (book_id, key)
                );
                INSERT INTO book_values (book_id, key, value)
                    SELECT substr(key, 6, length(key) - 14), 'prepared', value FROM settings
                    WHERE key LIKE 'book.%.prepared'
                    AND substr(key, 6, length(key) - 14) IN (SELECT id FROM books);
                DELETE FROM settings WHERE key LIKE 'book.%.prepared';"),
    // Version 8.
    // Reading without listening: when a book was finished, its length in words (known
    // once it has been parsed), and time spent reading per local day.
    Step::Sql("ALTER TABLE books ADD COLUMN finished_at INTEGER;
                ALTER TABLE books ADD COLUMN word_count INTEGER CHECK(word_count >= 0);
                CREATE TABLE reading (
                    book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    day TEXT NOT NULL,
                    ms INTEGER NOT NULL CHECK(ms >= 0),
                    PRIMARY KEY (book_id, day)
                );"),
    // Version 9.
    // Items that are not EPUBs: an audiobook on its own (and, later, an article). An
    // audiobook's place is a time on its recording, not a character offset.
    Step::Sql("ALTER TABLE books ADD COLUMN kind TEXT NOT NULL DEFAULT 'book'
                    CHECK(kind IN ('book', 'audiobook', 'article'));
                ALTER TABLE progress ADD COLUMN position_ms INTEGER CHECK(position_ms >= 0);
                ALTER TABLE bookmarks ADD COLUMN position_ms INTEGER CHECK(position_ms >= 0);"),
    // Version 10.
    // Articles saved from the web: their source and their fetch.
    Step::Sql("CREATE TABLE articles (
                    book_id TEXT PRIMARY KEY NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                    url TEXT NOT NULL,
                    canonical_url TEXT NOT NULL,
                    site_name TEXT,
                    byline TEXT,
                    published_at INTEGER,
                    stage TEXT NOT NULL,
                    reason TEXT,
                    error TEXT,
                    via TEXT,
                    fetched_at INTEGER,
                    word_count INTEGER CHECK(word_count >= 0),
                    declared_words INTEGER CHECK(declared_words >= 0),
                    lead TEXT
                );"),
    // Version 11.
    // Feeds and their entries; an entry opened without being saved is an item on the
    // feed shelf, out of the library's lists.
    Step::Sql("ALTER TABLE books ADD COLUMN shelf TEXT NOT NULL DEFAULT 'library'
                    CHECK(shelf IN ('library', 'feed'));
                CREATE TABLE feeds (
                    id INTEGER PRIMARY KEY,
                    url TEXT NOT NULL UNIQUE,
                    title TEXT,
                    site_url TEXT,
                    added_at INTEGER NOT NULL,
                    last_checked_at INTEGER,
                    last_ok_at INTEGER,
                    etag TEXT,
                    last_modified TEXT,
                    error TEXT,
                    failures INTEGER NOT NULL DEFAULT 0 CHECK(failures >= 0),
                    failing_since INTEGER
                );
                CREATE TABLE feed_entries (
                    id INTEGER PRIMARY KEY,
                    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
                    guid TEXT NOT NULL,
                    url TEXT,
                    title TEXT,
                    author TEXT,
                    published_at INTEGER,
                    seen_at INTEGER NOT NULL,
                    summary TEXT,
                    content_html TEXT,
                    words INTEGER CHECK(words >= 0),
                    read_at INTEGER,
                    book_id TEXT REFERENCES books(id) ON DELETE SET NULL,
                    UNIQUE(feed_id, guid)
                );
                CREATE INDEX feed_entries_feed ON feed_entries(feed_id, published_at);"),
    // Version 12.
    // Sites that needed the user (a sign-in, a check) and whether they are passed.
    Step::Sql("CREATE TABLE sites (
                    host TEXT PRIMARY KEY NOT NULL,
                    signed_in_at INTEGER,
                    check_passed_at INTEGER,
                    last_wall TEXT,
                    last_wall_at INTEGER
                );"),
    // Version 13.
    // Audio per book is summed from the index alone, without reading the segments' word times.
    Step::Sql("CREATE INDEX IF NOT EXISTS segments_book_audio ON segments(book_id, bytes, created_at);
                DROP INDEX IF EXISTS segments_book_id;"),
    // Version 14.
    // What the last scan of the library folder hashed, so unchanged files are not read again.
    Step::Sql("CREATE TABLE IF NOT EXISTS library_files (
                    path TEXT PRIMARY KEY NOT NULL,
                    book_id TEXT NOT NULL,
                    size INTEGER NOT NULL,
                    modified_ns INTEGER NOT NULL
                );"),
    // Version 15.
    // `articles.lead` and `feeds.failing_since` were added by editing steps 10 and 11 after
    // they shipped, so a library that ran those steps before then lacks the columns and one
    // made since has them.
    Step::Run(|tx| {
        add_column_if_missing(tx, "articles", "lead", "TEXT")?;
        add_column_if_missing(tx, "feeds", "failing_since", "INTEGER")
    }),
];

fn add_column_if_missing(
    tx: &rusqlite::Transaction,
    table: &str,
    column: &str,
    declaration: &str,
) -> rusqlite::Result<()> {
    let present: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
        [table, column],
        |row| row.get(0),
    )?;
    if !present {
        tx.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {declaration};"
        ))?;
    }
    Ok(())
}

impl Library {
    pub(super) fn initialize(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        // A write-ahead log, synced at checkpoints rather than at each commit: saving a place or
        // a minute of listening costs no fsync. A crash can lose the last commits, never the file.
        // (An in-memory database keeps its own journal and ignores this.)
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let tx = conn.transaction()?;
        let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let latest = MIGRATIONS.len() as i64;
        if !(0..=latest).contains(&version) {
            return Err(LibraryError::Corrupt(format!(
                "unsupported schema version {version}"
            )));
        }
        for (done, step) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            match step {
                Step::Sql(sql) => tx.execute_batch(sql)?,
                Step::Run(run) => run(&tx)?,
            }
            tx.pragma_update(None, "user_version", done as i64 + 1)?;
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
        finished_at: row.get("finished_at")?,
        word_count: row.get("word_count")?,
        kind: ItemKind::from_column(&row.get::<_, String>("kind")?)?,
        shelf: row.get("shelf")?,
    })
}

pub(super) fn segment_files(conn: &Connection, id: &BookId) -> Result<Vec<PathBuf>> {
    let mut stmt = conn.prepare("SELECT file FROM segments WHERE book_id = ?1 ORDER BY key")?;
    let files = stmt.query_map([&id.0], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?;
    Ok(files.collect::<std::result::Result<_, _>>()?)
}
