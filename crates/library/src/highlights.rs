//! Highlights: passages of a book's text the user marked, with an optional note. They are the
//! user's writing, so like a book's settings they stay when the book is removed (no foreign
//! key) and come back when the same file is added again.

use super::*;

/// A highlighted passage: characters `start..end` (Unicode scalar values) of chapter
/// `chapter`'s display text, and the text that was there when it was made.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Highlight {
    pub id: i64,
    pub book_id: BookId,
    pub chapter: u32,
    pub start: u32,
    pub end: u32,
    pub text: String,
    pub note: String,
    pub created_at: i64,
}

/// What [`Library::add_highlight`] stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewHighlight {
    pub chapter: u32,
    pub start: u32,
    pub end: u32,
    pub text: String,
    pub note: String,
}

const COLUMNS: &str = "id, book_id, chapter, start, end, text, note, created_at";

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Highlight> {
    Ok(Highlight {
        id: row.get(0)?,
        book_id: BookId(row.get(1)?),
        chapter: row.get(2)?,
        start: row.get(3)?,
        end: row.get(4)?,
        text: row.get(5)?,
        note: row.get(6)?,
        created_at: row.get(7)?,
    })
}

impl Library {
    /// Book `book`'s highlights in reading order.
    pub fn highlights(&self, book: &BookId) -> Result<Vec<Highlight>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM highlights WHERE book_id = ?1 ORDER BY chapter, start, id"
        ))?;
        let rows = stmt.query_map([&book.0], read)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn highlight(&self, id: i64) -> Result<Option<Highlight>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM highlights WHERE id = ?1"),
                [id],
                read,
            )
            .optional()?)
    }

    /// Adds `highlight` to book `book` in place of the highlights `replacing` (the ones it
    /// joins), in one transaction, and returns its id.
    pub fn add_highlight(
        &self,
        book: &BookId,
        highlight: &NewHighlight,
        replacing: &[i64],
        now: i64,
    ) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        for id in replacing {
            tx.execute(
                "DELETE FROM highlights WHERE id = ?1 AND book_id = ?2",
                rusqlite::params![id, book.0],
            )?;
        }
        tx.execute(
            "INSERT INTO highlights (book_id, chapter, start, end, text, note, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                book.0,
                highlight.chapter,
                highlight.start,
                highlight.end,
                highlight.text,
                highlight.note,
                now
            ],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(id)
    }

    /// Moves highlight `id` to `start..end` of its chapter (its text found elsewhere after the
    /// chapter changed). Returns false when no highlight has that id.
    pub fn move_highlight(&self, id: i64, start: u32, end: u32) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE highlights SET start = ?2, end = ?3 WHERE id = ?1",
            rusqlite::params![id, start, end],
        )? != 0)
    }

    /// Returns false when no highlight has that id.
    pub fn update_highlight_note(&self, id: i64, note: &str) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE highlights SET note = ?2 WHERE id = ?1",
            rusqlite::params![id, note],
        )? != 0)
    }

    /// Returns false when no highlight has that id.
    pub fn delete_highlight(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM highlights WHERE id = ?1", [id])?
            != 0)
    }
}
