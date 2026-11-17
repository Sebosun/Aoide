//! Pronunciations: how to say a written word, in a list for each book and one for all books.

use super::*;

/// One entry of a pronunciation list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Pronunciation {
    pub id: i64,
    /// The book whose list it is in; `None` for the list for all books.
    pub book_id: Option<BookId>,
    pub written: String,
    pub spoken: String,
    pub match_case: bool,
    pub created_at: i64,
}

const COLUMNS: &str = "id, book_id, written, spoken, match_case, created_at";

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Pronunciation> {
    Ok(Pronunciation {
        id: row.get(0)?,
        book_id: row.get::<_, Option<String>>(1)?.map(BookId),
        written: row.get(2)?,
        spoken: row.get(3)?,
        match_case: row.get(4)?,
        created_at: row.get(5)?,
    })
}

impl Library {
    /// Book `book`'s list, or with `None` the list for all books, oldest first.
    pub fn pronunciations(&self, book: Option<&BookId>) -> Result<Vec<Pronunciation>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM pronunciations WHERE book_id IS ?1 ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([book.map(|b| &b.0)], read)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// What applies to book `book`: its own list, then the list for all books.
    pub fn pronunciations_for(&self, book: &BookId) -> Result<Vec<Pronunciation>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM pronunciations WHERE book_id = ?1 OR book_id IS NULL
             ORDER BY book_id IS NULL, created_at, id"
        ))?;
        let rows = stmt.query_map([&book.0], read)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn pronunciation(&self, id: i64) -> Result<Option<Pronunciation>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM pronunciations WHERE id = ?1"),
                [id],
                read,
            )
            .optional()?)
    }

    /// Adds `written` to the list (book `book`'s, or with `None` the one for all books), or
    /// changes the entry the list has for it already, and returns the entry.
    pub fn save_pronunciation(
        &self,
        book: Option<&BookId>,
        written: &str,
        spoken: &str,
        match_case: bool,
        now: i64,
    ) -> Result<Pronunciation> {
        let book = book.map(|b| &b.0);
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE pronunciations SET spoken = ?3, match_case = ?4
             WHERE book_id IS ?1 AND written = ?2",
            rusqlite::params![book, written, spoken, match_case],
        )?;
        if changed == 0 {
            tx.execute(
                "INSERT INTO pronunciations (book_id, written, spoken, match_case, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![book, written, spoken, match_case, now],
            )?;
        }
        let saved = tx.query_row(
            &format!("SELECT {COLUMNS} FROM pronunciations WHERE book_id IS ?1 AND written = ?2"),
            rusqlite::params![book, written],
            read,
        )?;
        tx.commit()?;
        Ok(saved)
    }

    /// Returns false when no entry has that id.
    pub fn delete_pronunciation(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM pronunciations WHERE id = ?1", [id])?
            != 0)
    }
}
