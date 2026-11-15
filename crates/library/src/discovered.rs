//! EPUB files found by "Find books" and what was read from them.

use super::*;

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

impl Library {
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

    /// Stores what a scan read from each file (replacing what was known by path), together.
    pub fn upsert_discovered(&self, files: &[DiscoveredFile]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO discovered_files
                 (path, size, modified_ns, inode, hash, title, authors, language, isbn, cover_path, error, seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(path) DO UPDATE SET size = excluded.size, modified_ns = excluded.modified_ns,
                 inode = excluded.inode, hash = excluded.hash, title = excluded.title,
                 authors = excluded.authors, language = excluded.language, isbn = excluded.isbn,
                 cover_path = excluded.cover_path, error = excluded.error, seen_at = excluded.seen_at",
            )?;
            for file in files {
                stmt.execute(rusqlite::params![
                    store::path_text(&file.path)?,
                    file.size as i64,
                    file.modified_ns,
                    file.inode as i64,
                    file.hash,
                    file.title,
                    store::json(&file.authors)?,
                    file.language,
                    file.isbn,
                    file.cover_path
                        .as_deref()
                        .map(store::path_text)
                        .transpose()?,
                    file.error,
                    file.seen_at,
                ])?;
            }
        }
        tx.commit()?;
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
}
