//! Recordings paired with books: the audio files (used in place, never copied), the transcript
//! made from them block by block, and the sync map matched from it. The library stores the
//! recording's details, transcript words and report as text the caller defines.

use rusqlite::OptionalExtension;

use crate::{BookId, Library, Result};

/// A book's recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingRow {
    pub book_id: BookId,
    /// The files and what was learnt about them (JSON).
    pub files: String,
    pub total_ms: u64,
    /// The sync stage's name.
    pub stage: String,
    pub error: Option<String>,
    /// The user chose to keep a poor or doubtful match.
    pub accepted: bool,
    pub added_at: i64,
}

/// What was heard in one stretch of the recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptBlock {
    pub start_ms: u64,
    pub end_ms: u64,
    /// Which transcriber and model heard it.
    pub transcriber: String,
    /// The heard words with their times (JSON).
    pub words: String,
    /// Loudness of each frame of the block, for placing cuts in pauses.
    pub levels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSyncMap {
    /// Identifies the text and matcher the map was made for.
    pub text_hash: String,
    pub map: Vec<u8>,
    /// The match report (JSON).
    pub report: String,
    pub created_at: i64,
}

impl Library {
    /// Adds a book's recording, or replaces its details (stored transcript and map are kept).
    pub fn save_recording(&self, row: &RecordingRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO recordings (book_id, files, total_ms, stage, error, accepted, added_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(book_id) DO UPDATE SET files = ?2, total_ms = ?3, stage = ?4,
                error = ?5, accepted = ?6, added_at = ?7",
            rusqlite::params![
                row.book_id.0,
                row.files,
                row.total_ms as i64,
                row.stage,
                row.error,
                row.accepted,
                row.added_at
            ],
        )?;
        Ok(())
    }

    pub fn recording(&self, book: &BookId) -> Result<Option<RecordingRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM recordings WHERE book_id = ?1",
                [&book.0],
                read_recording,
            )
            .optional()?)
    }

    /// All recordings, oldest first (the order they sync in).
    pub fn recordings(&self) -> Result<Vec<RecordingRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM recordings ORDER BY added_at, book_id")?;
        let rows = stmt.query_map([], read_recording)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn set_recording_stage(
        &self,
        book: &BookId,
        stage: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE recordings SET stage = ?2, error = ?3 WHERE book_id = ?1",
            rusqlite::params![book.0, stage, error],
        )?;
        Ok(())
    }

    pub fn set_recording_accepted(&self, book: &BookId, accepted: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE recordings SET accepted = ?2 WHERE book_id = ?1",
            rusqlite::params![book.0, accepted],
        )?;
        Ok(())
    }

    /// Forgets the recording with its transcript and map. The audio files are not touched.
    pub fn remove_recording(&self, book: &BookId) -> Result<()> {
        self.conn
            .execute("DELETE FROM recordings WHERE book_id = ?1", [&book.0])?;
        Ok(())
    }

    pub fn add_transcript_block(&self, book: &BookId, block: &TranscriptBlock) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO recording_blocks
                (book_id, start_ms, end_ms, transcriber, words, levels)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                book.0,
                block.start_ms as i64,
                block.end_ms as i64,
                block.transcriber,
                block.words,
                block.levels
            ],
        )?;
        Ok(())
    }

    /// The transcript so far, in recording order.
    pub fn transcript_blocks(&self, book: &BookId) -> Result<Vec<TranscriptBlock>> {
        let mut stmt = self.conn.prepare(
            "SELECT start_ms, end_ms, transcriber, words, levels FROM recording_blocks
             WHERE book_id = ?1 ORDER BY start_ms",
        )?;
        let rows = stmt.query_map([&book.0], |row| {
            Ok(TranscriptBlock {
                start_ms: row.get::<_, i64>(0)? as u64,
                end_ms: row.get::<_, i64>(1)? as u64,
                transcriber: row.get(2)?,
                words: row.get(3)?,
                levels: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn clear_transcript(&self, book: &BookId) -> Result<()> {
        self.conn
            .execute("DELETE FROM recording_blocks WHERE book_id = ?1", [&book.0])?;
        Ok(())
    }

    pub fn save_sync_map(&self, book: &BookId, map: &StoredSyncMap) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO sync_maps (book_id, text_hash, map, report, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![book.0, map.text_hash, map.map, map.report, map.created_at],
        )?;
        Ok(())
    }

    pub fn sync_map(&self, book: &BookId) -> Result<Option<StoredSyncMap>> {
        Ok(self
            .conn
            .query_row(
                "SELECT text_hash, map, report, created_at FROM sync_maps WHERE book_id = ?1",
                [&book.0],
                |row| {
                    Ok(StoredSyncMap {
                        text_hash: row.get(0)?,
                        map: row.get(1)?,
                        report: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn clear_sync_map(&self, book: &BookId) -> Result<()> {
        self.conn
            .execute("DELETE FROM sync_maps WHERE book_id = ?1", [&book.0])?;
        Ok(())
    }

    /// Bytes of transcripts, loudness and sync maps stored for all recordings.
    pub fn sync_data_bytes(&self) -> Result<u64> {
        let bytes: i64 = self.conn.query_row(
            "SELECT
                (SELECT COALESCE(SUM(LENGTH(words) + LENGTH(levels)), 0) FROM recording_blocks)
              + (SELECT COALESCE(SUM(LENGTH(map) + LENGTH(report)), 0) FROM sync_maps)",
            [],
            |row| row.get(0),
        )?;
        Ok(bytes as u64)
    }
}

fn read_recording(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecordingRow> {
    Ok(RecordingRow {
        book_id: BookId(row.get("book_id")?),
        files: row.get("files")?,
        total_ms: row.get::<_, i64>("total_ms")? as u64,
        stage: row.get("stage")?,
        error: row.get("error")?,
        accepted: row.get("accepted")?,
        added_at: row.get("added_at")?,
    })
}
