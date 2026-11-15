//! Reading without listening: when a book was finished, its length in words, and time spent
//! reading, which adds up with listening time into the activity shown per day and per book.

use std::collections::BTreeMap;

use crate::{BookId, Library, Result};

/// Reading and listening per local day.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct DayActivity {
    /// Local calendar day, `YYYY-MM-DD`.
    pub day: String,
    pub reading_ms: u64,
    pub listening_ms: u64,
}

/// Reading and listening for one book, over all days.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BookActivity {
    pub book_id: BookId,
    pub reading_ms: u64,
    pub listening_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActivityStats {
    pub reading_ms: u64,
    pub listening_ms: u64,
    /// Oldest day first.
    pub by_day: Vec<DayActivity>,
    /// Most time first, then by id.
    pub by_book: Vec<BookActivity>,
}

impl Library {
    /// Marks the book finished at `at`, or not finished.
    pub fn set_finished_at(&self, id: &BookId, at: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE books SET finished_at = ?2 WHERE id = ?1",
            rusqlite::params![id.0, at],
        )?;
        Ok(())
    }

    pub fn set_word_count(&self, id: &BookId, words: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE books SET word_count = ?2 WHERE id = ?1",
            rusqlite::params![id.0, words],
        )?;
        Ok(())
    }

    /// Forgets where the reader is in the book.
    pub fn delete_progress(&self, id: &BookId) -> Result<()> {
        self.conn
            .execute("DELETE FROM progress WHERE book_id = ?1", [&id.0])?;
        Ok(())
    }

    /// Adds reading time to the book's total for a local day (`YYYY-MM-DD`).
    pub fn record_reading(&self, book: &BookId, day: &str, ms: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO reading (book_id, day, ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(book_id, day) DO UPDATE SET ms = reading.ms + excluded.ms",
            rusqlite::params![book.0, day, ms],
        )?;
        Ok(())
    }

    /// Reading and listening per day and per book.
    pub fn activity_stats(&self) -> Result<ActivityStats> {
        let mut days: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        let mut books: BTreeMap<String, (u64, u64)> = BTreeMap::new();
        for (table, reading) in [("reading", true), ("listening", false)] {
            let mut stmt = self
                .conn
                .prepare(&format!("SELECT book_id, day, ms FROM {table}"))?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                ))
            })?;
            for row in rows {
                let (book, day, ms) = row?;
                let add = |entry: &mut (u64, u64)| {
                    if reading {
                        entry.0 += ms;
                    } else {
                        entry.1 += ms;
                    }
                };
                add(days.entry(day).or_default());
                add(books.entry(book).or_default());
            }
        }
        let by_day: Vec<DayActivity> = days
            .into_iter()
            .map(|(day, (reading_ms, listening_ms))| DayActivity {
                day,
                reading_ms,
                listening_ms,
            })
            .collect();
        let mut by_book: Vec<BookActivity> = books
            .into_iter()
            .map(|(id, (reading_ms, listening_ms))| BookActivity {
                book_id: BookId(id),
                reading_ms,
                listening_ms,
            })
            .collect();
        by_book.sort_by_key(|b| std::cmp::Reverse(b.reading_ms + b.listening_ms));
        Ok(ActivityStats {
            reading_ms: by_day.iter().map(|d| d.reading_ms).sum(),
            listening_ms: by_day.iter().map(|d| d.listening_ms).sum(),
            by_day,
            by_book,
        })
    }
}
