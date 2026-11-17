//! Collections the user puts books in.

use super::*;

/// A user-made shelf of books.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    /// Items on it, archived ones left out (they are on it still).
    pub books: u64,
}

impl Library {
    /// Creates a collection and returns its id. Names are trimmed and unique, ignoring case.
    pub fn create_collection(&self, name: &str, now: i64) -> Result<i64> {
        let name = Self::collection_name(name)?;
        self.conn
            .execute(
                "INSERT INTO collections (name, created_at) VALUES (?1, ?2)",
                rusqlite::params![name, now],
            )
            .map_err(|e| Self::unique_name(e, name))?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rename_collection(&self, id: i64, name: &str) -> Result<()> {
        let name = Self::collection_name(name)?;
        let changed = self
            .conn
            .execute(
                "UPDATE collections SET name = ?1 WHERE id = ?2",
                rusqlite::params![name, id],
            )
            .map_err(|e| Self::unique_name(e, name))?;
        if changed == 0 {
            return Err(LibraryError::Invalid("no such collection".into()));
        }
        Ok(())
    }

    /// Deletes the collection; its books stay in the library.
    pub fn delete_collection(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM collections WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Every collection with its number of items not archived, by name.
    pub fn collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.name, c.created_at, COUNT(b.id) FROM collections c
             LEFT JOIN book_collections bc ON bc.collection_id = c.id
             LEFT JOIN books b ON b.id = bc.book_id AND b.archived_at IS NULL
             GROUP BY c.id ORDER BY c.name COLLATE NOCASE, c.id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Collection {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                books: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Puts the book on exactly these collections (duplicates ignored).
    pub fn set_book_collections(&self, book: &BookId, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM book_collections WHERE book_id = ?1", [&book.0])?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO book_collections (book_id, collection_id) VALUES (?1, ?2)",
            )?;
            for id in ids {
                stmt.execute(rusqlite::params![book.0, id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Puts the book on collection `id` too (nothing changes when it is on it).
    pub fn add_to_collection(&self, book: &BookId, id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO book_collections (book_id, collection_id) VALUES (?1, ?2)",
            rusqlite::params![book.0, id],
        )?;
        Ok(())
    }

    /// The collections of every book that is on any, each list ordered by collection name.
    pub fn book_collections(&self) -> Result<std::collections::HashMap<BookId, Vec<i64>>> {
        let mut stmt = self.conn.prepare(
            "SELECT bc.book_id, bc.collection_id FROM book_collections bc
             JOIN collections c ON c.id = bc.collection_id
             ORDER BY c.name COLLATE NOCASE, c.id",
        )?;
        let rows = stmt.query_map([], |row| Ok((BookId(row.get(0)?), row.get::<_, i64>(1)?)))?;
        let mut shelves = std::collections::HashMap::<BookId, Vec<i64>>::new();
        for row in rows {
            let (book, id) = row?;
            shelves.entry(book).or_default().push(id);
        }
        Ok(shelves)
    }
}
