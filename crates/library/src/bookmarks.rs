//! Bookmarks: a place in a book (a sentence, or a time in a recording) with a note.

use super::*;

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
    /// For an audiobook, the place on its timeline; `None` for text.
    pub position_ms: Option<u64>,
}

impl Library {
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

    /// Adds a bookmark at `position_ms` on an audiobook's timeline, in part `part`, and returns
    /// its id.
    pub fn add_time_bookmark(
        &self,
        book: &BookId,
        part: u32,
        position_ms: u64,
        excerpt: &str,
        now: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO bookmarks
             (book_id, chapter, char_offset, segment_id, excerpt, note, created_at, position_ms)
             VALUES (?1, ?2, 0, NULL, ?3, '', ?4, ?5)",
            rusqlite::params![book.0, part, excerpt, now, position_ms],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// The book's bookmarks ordered by chapter, then offset, then creation time.
    pub fn bookmarks(&self, book: &BookId) -> Result<Vec<Bookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, book_id, chapter, char_offset, segment_id, excerpt, note, created_at,
             position_ms FROM bookmarks WHERE book_id = ?1
             ORDER BY chapter, char_offset, position_ms, created_at",
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
                position_ms: row.get(8)?,
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
}
