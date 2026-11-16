//! Giving an audiobook on its own the EPUB it reads: one transaction that moves what the
//! audiobook has to the book and removes the audiobook.

use crate::{BookId, Library, Result};

impl Library {
    /// Moves audiobook `audiobook`'s recording (with any transcript and sync map), bookmarks
    /// (kept as times), collections, tags and listening and reading time to book `book`, keeps the
    /// earlier of their `added_at`, keeps the audiobook's place on its timeline as the book's
    /// [`crate::BookKey::PendingPlace`] (when it had one), and removes the audiobook. All or
    /// nothing. The caller checks that one is an audiobook, the other a book with no recording.
    pub fn merge_audiobook(&self, audiobook: &BookId, book: &BookId) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let (from, to) = (&audiobook.0, &book.0);
        // The recording's key is the book's id: copied to the new key, its transcript and sync
        // map moved under it, then the old row dropped.
        tx.execute(
            "INSERT INTO recordings (book_id, files, total_ms, stage, error, accepted, added_at)
             SELECT ?2, files, total_ms, stage, error, accepted, added_at FROM recordings
             WHERE book_id = ?1",
            [from, to],
        )?;
        for table in ["recording_blocks", "sync_maps"] {
            tx.execute(
                &format!("UPDATE {table} SET book_id = ?2 WHERE book_id = ?1"),
                [from, to],
            )?;
        }
        tx.execute("DELETE FROM recordings WHERE book_id = ?1", [from])?;
        tx.execute(
            "UPDATE bookmarks SET book_id = ?2 WHERE book_id = ?1",
            [from, to],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO book_collections (book_id, collection_id)
             SELECT ?2, collection_id FROM book_collections WHERE book_id = ?1",
            [from, to],
        )?;
        // The book keeps what it has (its own tags, those the user removed from it).
        tx.execute(
            "INSERT OR IGNORE INTO book_tags (book_id, tag_id, source)
             SELECT ?2, tag_id, source FROM book_tags WHERE book_id = ?1",
            [from, to],
        )?;
        for table in ["listening", "reading"] {
            tx.execute(
                &format!(
                    "INSERT INTO {table} (book_id, day, ms)
                     SELECT ?2, day, ms FROM {table} WHERE book_id = ?1
                     ON CONFLICT(book_id, day) DO UPDATE SET ms = {table}.ms + excluded.ms"
                ),
                [from, to],
            )?;
        }
        tx.execute(
            "UPDATE books SET added_at = MIN(added_at, (SELECT added_at FROM books WHERE id = ?1))
             WHERE id = ?2 AND EXISTS (SELECT 1 FROM books WHERE id = ?1)",
            [from, to],
        )?;
        tx.execute(
            "INSERT INTO book_values (book_id, key, value)
             SELECT ?2, ?3, position_ms FROM progress
             WHERE book_id = ?1 AND position_ms IS NOT NULL
             ON CONFLICT(book_id, key) DO UPDATE SET value = excluded.value",
            [from, to, crate::PENDING_PLACE],
        )?;
        tx.execute("DELETE FROM books WHERE id = ?1", [from])?;
        tx.commit()?;
        Ok(())
    }
}
