//! Takes: how many times the user asked for a sentence to be said again. A take belongs to the
//! sentence's speech text; once that text changes, the take no longer applies.

use std::collections::HashMap;

use super::*;

/// The takes of one book's sentences, by segment id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Takes(HashMap<String, (String, u32)>);

impl Takes {
    /// The take of the sentence `segment_id` saying `speech`: 0 when it was never said again,
    /// or was said again when its text was different.
    pub fn take(&self, segment_id: &str, speech: &str) -> u32 {
        match self.0.get(segment_id) {
            Some((hash, take)) if *hash == speech_hash(speech) => *take,
            _ => 0,
        }
    }
}

fn speech_hash(speech: &str) -> String {
    format!("{:x}", Sha256::digest(speech.as_bytes()))
}

impl Library {
    /// Counts up the take of the sentence `segment_id` saying `speech` and returns it: 1 the
    /// first time, and again 1 after the sentence's text changed.
    pub fn count_take(&self, book: &BookId, segment_id: &str, speech: &str) -> Result<u32> {
        let hash = speech_hash(speech);
        Ok(self.conn.query_row(
            "INSERT INTO segment_takes (book_id, segment_id, speech_hash, take)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(book_id, segment_id) DO UPDATE SET
             take = CASE WHEN speech_hash = excluded.speech_hash THEN take + 1 ELSE 1 END,
             speech_hash = excluded.speech_hash
             RETURNING take",
            rusqlite::params![book.0, segment_id, hash],
            |row| row.get(0),
        )?)
    }

    /// The takes of the book's sentences.
    pub fn takes(&self, book: &BookId) -> Result<Takes> {
        let mut stmt = self.conn.prepare(
            "SELECT segment_id, speech_hash, take FROM segment_takes WHERE book_id = ?1",
        )?;
        let rows = stmt.query_map([&book.0], |row| {
            Ok((row.get::<_, String>(0)?, (row.get(1)?, row.get(2)?)))
        })?;
        Ok(Takes(rows.collect::<std::result::Result<_, _>>()?))
    }
}
