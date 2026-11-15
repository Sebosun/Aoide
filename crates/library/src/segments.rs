//! The index of generated speech: one row per segment of audio, with its file, length and word timings.

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordTiming {
    pub start_ms: u32,
    pub end_ms: u32,
}

/// Generated audio indexed for one book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioUsage {
    pub book_id: BookId,
    pub bytes: u64,
    pub segments: u64,
    /// `created_at` of the newest segment.
    pub newest_at: i64,
}

/// Index entry for one generated audio segment stored in the audio cache directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedSegment {
    /// Hex cache key (hash of speech text + engine identity + settings).
    pub key: String,
    pub book_id: BookId,
    /// Path relative to the audio cache directory.
    pub file: PathBuf,
    pub duration_ms: u64,
    pub bytes: u64,
    pub word_timings: Option<Vec<WordTiming>>,
    pub created_at: i64,
}

impl Library {
    /// Inserts or replaces the segment with the same key.
    pub fn record_segment(&self, segment: &CachedSegment) -> Result<()> {
        let timings = segment.word_timings.as_ref().map(store::json).transpose()?;
        self.conn.execute(
            "INSERT INTO segments (key, book_id, file, duration_ms, bytes, word_timings, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(key) DO UPDATE SET book_id = excluded.book_id,
             file = excluded.file, duration_ms = excluded.duration_ms, bytes = excluded.bytes,
             word_timings = excluded.word_timings, created_at = excluded.created_at",
            rusqlite::params![segment.key, segment.book_id.0, store::path_text(&segment.file)?,
                segment.duration_ms, segment.bytes, timings, segment.created_at],
        )?;
        Ok(())
    }

    pub fn segment(&self, key: &str) -> Result<Option<CachedSegment>> {
        let mut stmt = self.conn.prepare("SELECT * FROM segments WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let timings: Option<String> = row.get("word_timings")?;
        Ok(Some(CachedSegment {
            key: row.get("key")?,
            book_id: BookId(row.get("book_id")?),
            file: row.get::<_, String>("file")?.into(),
            duration_ms: row.get("duration_ms")?,
            bytes: row.get("bytes")?,
            word_timings: timings.as_deref().map(store::parse_json).transpose()?,
            created_at: row.get("created_at")?,
        }))
    }

    /// Which of `keys` are present in the index (order of `keys` preserved).
    pub fn cached_keys(&self, keys: &[String]) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT EXISTS(SELECT 1 FROM segments WHERE key = ?1)")?;
        let mut cached = Vec::new();
        for key in keys {
            if stmt.query_row([key], |row| row.get::<_, bool>(0))? {
                cached.push(key.clone());
            }
        }
        Ok(cached)
    }

    /// Total `bytes` of all segments recorded for the book.
    pub fn book_audio_bytes(&self, id: &BookId) -> Result<u64> {
        let mut stmt = self
            .conn
            .prepare("SELECT bytes FROM segments WHERE book_id = ?1")?;
        let mut rows = stmt.query([&id.0])?;
        let mut total = 0_u64;
        while let Some(row) = rows.next()? {
            total = total
                .checked_add(row.get(0)?)
                .ok_or_else(|| LibraryError::Corrupt("book audio byte total exceeds u64".into()))?;
        }
        Ok(total)
    }

    /// Removes the book's segment rows, returning their cache-relative files.
    pub fn delete_book_segments(&self, id: &BookId) -> Result<Vec<PathBuf>> {
        let tx = self.conn.unchecked_transaction()?;
        let files = store::segment_files(&tx, id)?;
        tx.execute("DELETE FROM segments WHERE book_id = ?1", [&id.0])?;
        tx.commit()?;
        Ok(files)
    }

    /// Sum of `bytes` over every recorded segment.
    pub fn total_audio_bytes(&self) -> Result<u64> {
        let mut stmt = self.conn.prepare("SELECT bytes FROM segments")?;
        let mut rows = stmt.query([])?;
        let mut total = 0_u64;
        while let Some(row) = rows.next()? {
            total = total
                .checked_add(row.get(0)?)
                .ok_or_else(|| LibraryError::Corrupt("audio byte total exceeds u64".into()))?;
        }
        Ok(total)
    }

    /// Indexed audio per book that has any, largest first (ties by id).
    pub fn audio_usage(&self) -> Result<Vec<AudioUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, SUM(bytes), COUNT(*), MAX(created_at) FROM segments
             GROUP BY book_id ORDER BY SUM(bytes) DESC, book_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AudioUsage {
                book_id: BookId(row.get(0)?),
                bytes: row.get(1)?,
                segments: row.get(2)?,
                newest_at: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Cache-relative files of every indexed segment.
    pub fn segment_files(&self) -> Result<std::collections::HashSet<PathBuf>> {
        let mut stmt = self.conn.prepare("SELECT file FROM segments")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut files = std::collections::HashSet::new();
        for file in rows {
            files.insert(PathBuf::from(file?));
        }
        Ok(files)
    }

    /// Up to `limit` segments, oldest `created_at` first (ties by key), skipping `except` book.
    pub fn oldest_segments(
        &self,
        except: Option<&BookId>,
        limit: usize,
    ) -> Result<Vec<CachedSegment>> {
        let mut stmt = self.conn.prepare(
            "SELECT key FROM segments WHERE (?1 IS NULL OR book_id != ?1)
             ORDER BY created_at ASC, key ASC LIMIT ?2",
        )?;
        let keys = stmt.query_map(
            rusqlite::params![
                except.map(|id| &id.0),
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |row| row.get::<_, String>(0),
        )?;
        let mut segments = Vec::new();
        for key in keys {
            if let Some(segment) = self.segment(&key?)? {
                segments.push(segment);
            }
        }
        Ok(segments)
    }

    /// Deletes index rows for these keys; unknown keys are ignored.
    pub fn delete_segments(&self, keys: &[String]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare("DELETE FROM segments WHERE key = ?1")?;
            for key in keys {
                stmt.execute([key])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}
