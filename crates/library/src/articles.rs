//! Articles saved from the web: where each came from and where its fetch stands. The page
//! itself is a snapshot file the item's `path` points at; the stage, reason and source are
//! names the caller defines.

use rusqlite::OptionalExtension;

use crate::{BookId, Library, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleRow {
    pub book_id: BookId,
    /// As the user or a feed gave it.
    pub url: String,
    /// Normalised; the item's id is its hash.
    pub canonical_url: String,
    pub site_name: Option<String>,
    pub byline: Option<String>,
    pub published_at: Option<i64>,
    /// The fetch stage's name.
    pub stage: String,
    /// Why it needs the user, when it does.
    pub reason: Option<String>,
    pub error: Option<String>,
    /// How the stored snapshot was got.
    pub via: Option<String>,
    pub fetched_at: Option<i64>,
    pub word_count: Option<u64>,
    pub declared_words: Option<u64>,
    /// The article's first lines, for its card.
    pub lead: Option<String>,
}

impl Library {
    /// Adds an article's row, or replaces it.
    pub fn save_article(&self, row: &ArticleRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO articles (book_id, url, canonical_url, site_name, byline, published_at,
                 stage, reason, error, via, fetched_at, word_count, declared_words, lead)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(book_id) DO UPDATE SET url = ?2, canonical_url = ?3, site_name = ?4,
                 byline = ?5, published_at = ?6, stage = ?7, reason = ?8, error = ?9, via = ?10,
                 fetched_at = ?11, word_count = ?12, declared_words = ?13, lead = ?14",
            rusqlite::params![
                row.book_id.0,
                row.url,
                row.canonical_url,
                row.site_name,
                row.byline,
                row.published_at,
                row.stage,
                row.reason,
                row.error,
                row.via,
                row.fetched_at,
                row.word_count.map(|n| n as i64),
                row.declared_words.map(|n| n as i64),
                row.lead,
            ],
        )?;
        Ok(())
    }

    pub fn article(&self, book: &BookId) -> Result<Option<ArticleRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM articles WHERE book_id = ?1",
                [&book.0],
                read_article,
            )
            .optional()?)
    }

    pub fn articles(&self) -> Result<Vec<ArticleRow>> {
        let mut statement = self
            .conn
            .prepare("SELECT * FROM articles ORDER BY book_id")?;
        let rows = statement.query_map([], read_article)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Moves an article's fetch to `stage`, with its reason and error (either may be cleared).
    pub fn set_article_stage(
        &self,
        book: &BookId,
        stage: &str,
        reason: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE articles SET stage = ?2, reason = ?3, error = ?4 WHERE book_id = ?1",
            rusqlite::params![book.0, stage, reason, error],
        )?;
        Ok(())
    }
}

fn read_article(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArticleRow> {
    Ok(ArticleRow {
        book_id: BookId(row.get("book_id")?),
        url: row.get("url")?,
        canonical_url: row.get("canonical_url")?,
        site_name: row.get("site_name")?,
        byline: row.get("byline")?,
        published_at: row.get("published_at")?,
        stage: row.get("stage")?,
        reason: row.get("reason")?,
        error: row.get("error")?,
        via: row.get("via")?,
        fetched_at: row.get("fetched_at")?,
        word_count: row.get::<_, Option<i64>>("word_count")?.map(|n| n as u64),
        declared_words: row
            .get::<_, Option<i64>>("declared_words")?
            .map(|n| n as u64),
        lead: row.get("lead")?,
    })
}
