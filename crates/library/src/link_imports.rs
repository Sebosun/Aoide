//! Imports of links from another app (Pocket and the like): each import, and the items it made.

use crate::{BookId, Library, Result};

/// An import of links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkImportRow {
    pub id: i64,
    /// Which app's export (`aoide_imports::Source::name`).
    pub source: String,
    pub started_at: i64,
    pub paused: bool,
    pub finished_at: Option<i64>,
}

impl Library {
    /// Starts an import from `source`; returns its id.
    pub fn start_link_import(&self, source: &str, now: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO link_imports (source, started_at) VALUES (?1, ?2)",
            rusqlite::params![source, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Adds items to import `id`, after those it has (one already in it keeps its place).
    pub fn add_link_import_items(&self, id: i64, books: &[BookId]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO link_import_items (import_id, book_id) VALUES (?1, ?2)",
            )?;
            for book in books {
                insert.execute(rusqlite::params![id, book.0])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Import `id`'s items, in the order they were added.
    pub fn link_import_items(&self, id: i64) -> Result<Vec<BookId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT book_id FROM link_import_items WHERE import_id = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map([id], |row| Ok(BookId(row.get(0)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The import not finished yet, if any (the latest).
    pub fn current_link_import(&self) -> Result<Option<LinkImportRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source, started_at, paused, finished_at FROM link_imports
             WHERE finished_at IS NULL ORDER BY id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query([])?;
        Ok(rows
            .next()?
            .map(|row| -> rusqlite::Result<LinkImportRow> {
                Ok(LinkImportRow {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    started_at: row.get(2)?,
                    paused: row.get(3)?,
                    finished_at: row.get(4)?,
                })
            })
            .transpose()?)
    }

    pub fn set_link_import_paused(&self, id: i64, paused: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE link_imports SET paused = ?2 WHERE id = ?1",
            rusqlite::params![id, paused],
        )?;
        Ok(())
    }

    /// Import `id` is over: its summary was dismissed.
    pub fn finish_link_import(&self, id: i64, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE link_imports SET finished_at = ?2 WHERE id = ?1",
            rusqlite::params![id, now],
        )?;
        Ok(())
    }
}
