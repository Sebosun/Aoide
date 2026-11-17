//! The library's text in a full-text index (SQLite FTS5), one row per block (paragraph), and
//! searching it. The core decides what a block's text and sentences are; this stores and finds.

use crate::{BookId, Library, Result};

/// One block of an item, as indexed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassageRow {
    pub chapter: u32,
    pub block: u32,
    /// Where the block starts in its chapter's text (characters).
    pub at: u32,
    pub text: String,
    /// Where each sentence starts in the block (characters), with its segment id.
    pub segments: Vec<(u32, String)>,
}

/// A passage that matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassageHit {
    pub chapter: u32,
    pub block: u32,
    /// The sentence holding the first match.
    pub segment_id: String,
    /// Where the first match starts in the chapter's text (characters).
    pub first: u32,
    /// The words around the matches.
    pub snippet: String,
    /// The matches in `snippet`, as character ranges.
    pub marks: Vec<(usize, usize)>,
}

/// An item's best passages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemPassages {
    pub book_id: BookId,
    pub hits: Vec<PassageHit>,
    /// Passages that matched beyond those given.
    pub more: u32,
}

/// Passages given per item.
const PER_ITEM: i64 = 3;
/// Words of context in a snippet.
const SNIPPET_TOKENS: i64 = 24;
const OPEN: char = '\u{2}';
const CLOSE: char = '\u{3}';

/// The user's words as an FTS5 query: each a quoted term (so operators and punctuation are
/// text), all required, the last also matching as a prefix. `None` when no word is in it.
pub fn fts_query(typed: &str) -> Option<String> {
    let words: Vec<&str> = typed
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect();
    let last = words.len().checked_sub(1)?;
    Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let quoted = format!("\"{}\"", w.replace('"', "\"\""));
                if i == last {
                    quoted + "*"
                } else {
                    quoted
                }
            })
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

/// `text` with its marked runs taken out: the plain text and the runs as character ranges.
fn unmark(text: &str) -> (String, Vec<(usize, usize)>) {
    let (mut plain, mut marks, mut count, mut from) = (String::new(), Vec::new(), 0, None);
    for c in text.chars() {
        match c {
            OPEN => from = Some(count),
            CLOSE => {
                if let Some(start) = from.take() {
                    marks.push((start, count));
                }
            }
            c => {
                plain.push(c);
                count += 1;
            }
        }
    }
    (plain, marks)
}

impl Library {
    /// Stores item `book`'s blocks under indexing rule `version`, replacing what it had.
    pub fn index_passages(&self, book: &BookId, version: i64, rows: &[PassageRow]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM passage_rows WHERE book_id = ?1", [&book.0])?;
        {
            let mut insert = tx.prepare(
                "INSERT INTO passage_rows (book_id, chapter, block, at, segments, text)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for row in rows {
                let segments = serde_json::to_string(&row.segments)
                    .map_err(|e| crate::LibraryError::Corrupt(e.to_string()))?;
                insert.execute(rusqlite::params![
                    book.0,
                    row.chapter,
                    row.block,
                    row.at,
                    segments,
                    row.text
                ])?;
            }
        }
        tx.execute(
            "INSERT INTO indexed (book_id, version, blocks) VALUES (?1, ?2, ?3)
             ON CONFLICT(book_id) DO UPDATE SET version = excluded.version,
             blocks = excluded.blocks",
            rusqlite::params![book.0, version, rows.len() as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The indexing rule item `book` was indexed under, if it was.
    pub fn indexed_version(&self, book: &BookId) -> Result<Option<i64>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT version FROM indexed WHERE book_id = ?1",
                [&book.0],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Item `book` is to be indexed again (its text changed).
    pub fn forget_passages(&self, book: &BookId) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM passage_rows WHERE book_id = ?1", [&book.0])?;
        tx.execute("DELETE FROM indexed WHERE book_id = ?1", [&book.0])?;
        tx.commit()?;
        Ok(())
    }

    /// Drops the whole index.
    pub fn clear_passages(&self) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM passage_rows", [])?;
        tx.execute("DELETE FROM indexed", [])?;
        tx.commit()?;
        Ok(())
    }

    /// Blocks stored.
    pub fn passages_rows(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM passage_rows", [], |row| row.get(0))?)
    }

    /// Items on the library shelf with text (books and articles, present), not indexed under
    /// rule `version`: the most recently opened first, then the most recently added.
    pub fn to_index(&self, version: i64) -> Result<Vec<BookId>> {
        let mut query = self.conn.prepare(
            "SELECT b.id FROM books b LEFT JOIN indexed i ON i.book_id = b.id
             WHERE b.shelf = 'library' AND b.kind IN ('book', 'article')
               AND b.status = 'present' AND (i.version IS NULL OR i.version < ?1)
             ORDER BY b.last_opened_at IS NULL, b.last_opened_at DESC, b.added_at DESC",
        )?;
        let ids = query
            .query_map([version], |row| row.get(0).map(BookId))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Bytes the index takes on disk.
    pub fn passages_bytes(&self) -> Result<u64> {
        let bytes: Option<i64> = self.conn.query_row(
            "SELECT SUM(pgsize) FROM dbstat
             WHERE name IN ('passage_rows', 'passage_rows_book', 'indexed')
                OR name LIKE 'passages%'",
            [],
            |row| row.get(0),
        )?;
        Ok(bytes.unwrap_or(0).max(0) as u64)
    }

    /// The passages matching `query` (see [`fts_query`]) on the library shelf: at most `items`
    /// items, ordered by their best passage, with their best three each.
    pub fn search_passages(&self, query: &str, items: usize) -> Result<Vec<ItemPassages>> {
        let mut ranked = self.conn.prepare(
            "WITH m AS (SELECT rowid AS id, bm25(passages) AS rank FROM passages
                        WHERE passages MATCH ?1),
             r AS (SELECT p.id, p.book_id, m.rank,
                          ROW_NUMBER() OVER (PARTITION BY p.book_id ORDER BY m.rank, p.id) AS n,
                          COUNT(*) OVER (PARTITION BY p.book_id) AS total,
                          MIN(m.rank) OVER (PARTITION BY p.book_id) AS best
                   FROM m JOIN passage_rows p ON p.id = m.id
                   JOIN books b ON b.id = p.book_id AND b.shelf = 'library')
             SELECT id, book_id, total FROM r
             WHERE n <= ?2 AND book_id IN (
                 SELECT book_id FROM r WHERE n = 1 ORDER BY best, book_id LIMIT ?3)
             ORDER BY best, book_id, n",
        )?;
        let rows: Vec<(i64, String, i64)> = ranked
            .query_map(rusqlite::params![query, PER_ITEM, items as i64], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut detail = self.conn.prepare(
            "SELECT p.chapter, p.block, p.at, p.segments,
                    snippet(passages, 0, char(2), char(3), '…', ?3),
                    highlight(passages, 0, char(2), char(3))
             FROM passages JOIN passage_rows p ON p.id = passages.rowid
             WHERE passages MATCH ?1 AND passages.rowid = ?2",
        )?;
        let mut found: Vec<ItemPassages> = Vec::new();
        for (id, book, total) in rows {
            let hit = detail.query_row(rusqlite::params![query, id, SNIPPET_TOKENS], |row| {
                let segments: String = row.get(3)?;
                let snippet: String = row.get(4)?;
                let whole: String = row.get(5)?;
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    segments,
                    snippet,
                    whole,
                ))
            })?;
            let (chapter, block, at, segments, snippet, whole): (
                u32,
                u32,
                u32,
                String,
                String,
                String,
            ) = hit;
            let segments: Vec<(u32, String)> = serde_json::from_str(&segments)
                .map_err(|e| crate::LibraryError::Corrupt(e.to_string()))?;
            let (_, in_block) = unmark(&whole);
            let start = in_block.first().map_or(0, |m| m.0) as u32;
            let segment_id = segments
                .iter()
                .take_while(|(from, _)| *from <= start)
                .last()
                .or(segments.first())
                .map(|(_, id)| id.clone())
                .unwrap_or_default();
            let (snippet, marks) = unmark(&snippet);
            let hit = PassageHit {
                chapter,
                block,
                segment_id,
                first: at + start,
                snippet,
                marks,
            };
            match found.last_mut() {
                Some(item) if item.book_id.0 == book => item.hits.push(hit),
                _ => found.push(ItemPassages {
                    book_id: BookId(book),
                    hits: vec![hit],
                    more: 0,
                }),
            }
            let item = found.last_mut().unwrap();
            item.more = (total as u32).saturating_sub(item.hits.len() as u32);
        }
        Ok(found)
    }
}
