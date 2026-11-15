//! Time spent listening, per book and day.

use super::*;

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

impl Library {
    /// Adds listened time to the book's total for a local day (`YYYY-MM-DD`).
    pub fn record_listening(&self, book: &BookId, day: &str, ms: u64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO listening (book_id, day, ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(book_id, day) DO UPDATE SET ms = listening.ms + excluded.ms",
            rusqlite::params![book.0, day, ms],
        )?;
        Ok(())
    }

    /// One book's listened time per local day, oldest day first.
    pub fn book_listening(&self, book: &BookId) -> Result<Vec<DayListening>> {
        let mut stmt = self
            .conn
            .prepare("SELECT day, ms FROM listening WHERE book_id = ?1 ORDER BY day ASC")?;
        let days = stmt
            .query_map([&book.0], |row| {
                Ok(DayListening {
                    day: row.get(0)?,
                    ms: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(days)
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
}
