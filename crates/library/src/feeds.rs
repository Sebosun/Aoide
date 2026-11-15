//! Feeds subscribed to and the entries they brought. An entry opened is linked to the item it
//! was read as; the item may be on the feed shelf (not saved) or in the library.

use rusqlite::{OptionalExtension, Row};

use crate::{BookId, Library, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedRow {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub added_at: i64,
    pub last_checked_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub error: Option<String>,
    /// Checks failed in a row.
    pub failures: u32,
}

/// An entry as a feed gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEntry {
    pub guid: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<i64>,
    /// Plain text, short.
    pub summary: Option<String>,
    /// Only when the feed carries the whole text.
    pub content_html: Option<String>,
    /// Of `content_html`.
    pub words: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRow {
    pub id: i64,
    pub feed_id: i64,
    pub guid: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub published_at: Option<i64>,
    /// When a check first found it.
    pub seen_at: i64,
    pub summary: Option<String>,
    pub content_html: Option<String>,
    pub words: Option<u32>,
    pub read_at: Option<i64>,
    /// The item it was opened as.
    pub book_id: Option<BookId>,
}

fn read_feed(row: &Row<'_>) -> rusqlite::Result<FeedRow> {
    Ok(FeedRow {
        id: row.get("id")?,
        url: row.get("url")?,
        title: row.get("title")?,
        site_url: row.get("site_url")?,
        added_at: row.get("added_at")?,
        last_checked_at: row.get("last_checked_at")?,
        last_ok_at: row.get("last_ok_at")?,
        etag: row.get("etag")?,
        last_modified: row.get("last_modified")?,
        error: row.get("error")?,
        failures: row.get("failures")?,
    })
}

pub(crate) fn read_entry(row: &Row<'_>) -> rusqlite::Result<EntryRow> {
    Ok(EntryRow {
        id: row.get("id")?,
        feed_id: row.get("feed_id")?,
        guid: row.get("guid")?,
        url: row.get("url")?,
        title: row.get("title")?,
        author: row.get("author")?,
        published_at: row.get("published_at")?,
        seen_at: row.get("seen_at")?,
        summary: row.get("summary")?,
        content_html: row.get("content_html")?,
        words: row.get("words")?,
        read_at: row.get("read_at")?,
        book_id: row.get::<_, Option<String>>("book_id")?.map(BookId),
    })
}

impl Library {
    /// Subscribes to the feed at `url`, or finds it subscribed; returns its id.
    pub fn add_feed(&self, url: &str, now: i64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO feeds (url, added_at) VALUES (?1, ?2) ON CONFLICT(url) DO NOTHING",
            rusqlite::params![url, now],
        )?;
        Ok(self
            .conn
            .query_row("SELECT id FROM feeds WHERE url = ?1", [url], |r| r.get(0))?)
    }

    /// Every feed, oldest subscription first.
    pub fn feeds(&self) -> Result<Vec<FeedRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM feeds ORDER BY added_at, id")?;
        let feeds = stmt.query_map([], read_feed)?;
        Ok(feeds.collect::<rusqlite::Result<_>>()?)
    }

    pub fn feed(&self, id: i64) -> Result<Option<FeedRow>> {
        Ok(self
            .conn
            .query_row("SELECT * FROM feeds WHERE id = ?1", [id], read_feed)
            .optional()?)
    }

    /// Stores everything about feed `feed.id` but when it was added.
    pub fn update_feed(&self, feed: &FeedRow) -> Result<()> {
        self.conn.execute(
            "UPDATE feeds SET url = ?2, title = ?3, site_url = ?4, last_checked_at = ?5,
             last_ok_at = ?6, etag = ?7, last_modified = ?8, error = ?9, failures = ?10
             WHERE id = ?1",
            rusqlite::params![
                feed.id,
                feed.url,
                feed.title,
                feed.site_url,
                feed.last_checked_at,
                feed.last_ok_at,
                feed.etag,
                feed.last_modified,
                feed.error,
                feed.failures,
            ],
        )?;
        Ok(())
    }

    /// Removes the feed and its entries (items opened from them are the caller's to keep or
    /// delete).
    pub fn remove_feed(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM feeds WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Stores an entry `feed` gave, first seen at `now`. Returns its id, or `None` when the
    /// feed gave it before; then only a missing title or link is filled in.
    pub fn insert_entry(&self, feed: i64, entry: &NewEntry, now: i64) -> Result<Option<i64>> {
        let added = self.conn.execute(
            "INSERT INTO feed_entries
             (feed_id, guid, url, title, author, published_at, seen_at, summary, content_html,
              words)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(feed_id, guid) DO NOTHING",
            rusqlite::params![
                feed,
                entry.guid,
                entry.url,
                entry.title,
                entry.author,
                entry.published_at,
                now,
                entry.summary,
                entry.content_html,
                entry.words,
            ],
        )?;
        if added == 1 {
            return Ok(Some(self.conn.last_insert_rowid()));
        }
        self.conn.execute(
            "UPDATE feed_entries SET title = COALESCE(title, ?3), url = COALESCE(url, ?4)
             WHERE feed_id = ?1 AND guid = ?2",
            rusqlite::params![feed, entry.guid, entry.title, entry.url],
        )?;
        Ok(None)
    }

    pub fn entry(&self, id: i64) -> Result<Option<EntryRow>> {
        Ok(self
            .conn
            .query_row("SELECT * FROM feed_entries WHERE id = ?1", [id], read_entry)
            .optional()?)
    }

    /// Records the item entry `id` was opened as.
    pub fn link_entry(&self, id: i64, book: &BookId) -> Result<()> {
        self.conn.execute(
            "UPDATE feed_entries SET book_id = ?2 WHERE id = ?1",
            rusqlite::params![id, book.0],
        )?;
        Ok(())
    }
}
