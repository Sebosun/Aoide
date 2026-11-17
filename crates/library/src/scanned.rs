//! What the last scan of the library folder found, for the next scan to compare with.

use super::*;

impl Library {
    /// The files the last scan of the library folder found.
    pub fn scanned_files(&self) -> Result<Vec<ScannedFile>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, book_id, size, modified_ns FROM library_files ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok(ScannedFile {
                path: PathBuf::from(row.get::<_, String>(0)?),
                id: BookId(row.get(1)?),
                size: row.get::<_, i64>(2)? as u64,
                modified_ns: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Remembers what a scan of the library folder found, in place of the last scan's.
    /// Forgets every file's hash, so the next scan reads each once (after a restore, when the
    /// files' times and places are another computer's).
    pub fn clear_scanned_files(&self) -> Result<()> {
        self.conn.execute("DELETE FROM library_files", [])?;
        Ok(())
    }

    pub fn store_scanned_files(&self, files: &[ScannedFile]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM library_files", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO library_files (path, book_id, size, modified_ns) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for file in files {
                stmt.execute(rusqlite::params![
                    store::path_text(&file.path)?,
                    file.id.0,
                    file.size as i64,
                    file.modified_ns,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
