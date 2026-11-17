//! The queue: items lined up to listen to next, in order. An item removed from the library
//! leaves it.

use super::*;

impl Library {
    /// The items queued, first to play first.
    pub fn queue(&self) -> Result<Vec<BookId>> {
        let mut stmt = self
            .conn
            .prepare("SELECT book_id FROM queue ORDER BY position, book_id")?;
        let rows = stmt.query_map([], |row| Ok(BookId(row.get(0)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Puts `book` at place `at` in the queue (0 is next; past the end is last), moving it there
    /// when it is queued already.
    pub fn queue_put(&self, book: &BookId, at: usize) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        let mut order = Self::queued(&tx)?;
        order.retain(|queued| queued != &book.0);
        order.insert(at.min(order.len()), book.0.clone());
        Self::renumber(&tx, &order)?;
        tx.commit()?;
        Ok(())
    }

    /// Takes `book` off the queue; false when it was not on it.
    pub fn queue_remove(&self, book: &BookId) -> Result<bool> {
        let tx = self.conn.unchecked_transaction()?;
        let removed = tx.execute("DELETE FROM queue WHERE book_id = ?1", [&book.0])? > 0;
        if removed {
            let order = Self::queued(&tx)?;
            Self::renumber(&tx, &order)?;
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Empties the queue.
    pub fn queue_clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM queue", [])?;
        Ok(())
    }

    fn queued(tx: &rusqlite::Transaction) -> Result<Vec<String>> {
        let mut stmt = tx.prepare("SELECT book_id FROM queue ORDER BY position, book_id")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    fn renumber(tx: &rusqlite::Transaction, order: &[String]) -> Result<()> {
        tx.execute("DELETE FROM queue", [])?;
        let mut stmt = tx.prepare("INSERT INTO queue (book_id, position) VALUES (?1, ?2)")?;
        for (position, id) in order.iter().enumerate() {
            stmt.execute(rusqlite::params![id, position as i64])?;
        }
        Ok(())
    }
}
