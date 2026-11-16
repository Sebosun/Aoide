//! Tags on items: the user's own and those that came with an item (its subjects or genre, a
//! classifier's guess), for filtering the library. A tag the user removes is kept as removed,
//! so it does not come back when the item's subjects are read again.

use std::collections::HashMap;

use super::*;

/// Where an item's tag came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagSource {
    /// The user put it on.
    User,
    /// The item's own: an EPUB's subjects, an audiobook's genre, a looked-up record's subjects.
    Metadata,
    /// A classifier's guess.
    Classifier,
}

impl TagSource {
    fn as_str(self) -> &'static str {
        match self {
            TagSource::User => "user",
            TagSource::Metadata => "metadata",
            TagSource::Classifier => "classifier",
        }
    }
}

/// A tag and how many items have it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TagCount {
    pub name: String,
    pub books: u64,
}

/// Tags at most per item, from what came with it.
const MAX_TAGS: usize = 8;
/// Words that say nothing about an item (that one is an audiobook, the library's kinds say).
const EMPTY_TAGS: &[&str] = &[
    "general",
    "fiction & literature",
    "unknown",
    "other",
    "misc",
    "none",
    "audiobook",
    "audiobooks",
    "audio book",
    "speech",
    "spoken word",
];

/// Tags made from what came with an item: subject headings split at `/`, `--` and `;`
/// ("Fiction / Science Fiction / General" gives "Fiction" and "Science Fiction"), spaces
/// collapsed, empty words ("General"), codes and long headings and repeats (ignoring case) left
/// out, at most eight.
pub fn tidy_tags(raw: &[String]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for heading in raw {
        for part in heading.split(['/', ';']).flat_map(|p| p.split("--")) {
            let tag = part.split_whitespace().collect::<Vec<_>>().join(" ");
            let tag = tag.trim_matches(|c: char| c == '.' || c == ',' || c.is_whitespace());
            // A list's code ("nyt:hardcover-fiction=2014-01-05") is not a subject.
            let long = tag.chars().count() > 40 || tag.contains([':', '=']);
            let empty = tag.is_empty() || EMPTY_TAGS.contains(&tag.to_lowercase().as_str());
            if !long && !empty && !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                tags.push(tag.to_owned());
            }
        }
    }
    tags.truncate(MAX_TAGS);
    tags
}

/// A tag as stored: trimmed, inner spaces collapsed.
fn tag_name(name: &str) -> Option<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then_some(name)
}

impl Library {
    /// Each item's tags (not those removed), by name.
    pub fn book_tags(&self) -> Result<HashMap<BookId, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT bt.book_id, t.name FROM book_tags bt JOIN tags t ON t.id = bt.tag_id
             WHERE bt.source != 'removed' ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((BookId(row.get(0)?), row.get::<_, String>(1)?))
        })?;
        let mut tags = HashMap::<BookId, Vec<String>>::new();
        for row in rows {
            let (book, name) = row?;
            tags.entry(book).or_default().push(name);
        }
        Ok(tags)
    }

    /// Every tag on some item, with how many items have it, by name.
    pub fn tags(&self) -> Result<Vec<TagCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT t.name, COUNT(*) FROM book_tags bt JOIN tags t ON t.id = bt.tag_id
             WHERE bt.source != 'removed' GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TagCount {
                name: row.get(0)?,
                books: row.get(1)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Adds tags to an item from `source`. Tags it has, or that the user removed, are left as
    /// they are.
    pub fn add_tags(&self, book: &BookId, names: &[String], source: TagSource) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        for name in names.iter().filter_map(|n| tag_name(n)) {
            let tag = Self::tag_id(&tx, &name)?;
            tx.execute(
                "INSERT OR IGNORE INTO book_tags (book_id, tag_id, source) VALUES (?1, ?2, ?3)",
                rusqlite::params![book.0, tag, source.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The user's say on an item's tags: exactly these are shown. The others are kept as
    /// removed; one the user removed before is shown again when named.
    pub fn set_tags(&self, book: &BookId, names: &[String]) -> Result<()> {
        let names: Vec<String> = names.iter().filter_map(|n| tag_name(n)).collect();
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE book_tags SET source = 'removed' WHERE book_id = ?1",
            [&book.0],
        )?;
        for name in &names {
            let tag = Self::tag_id(&tx, name)?;
            tx.execute(
                "INSERT INTO book_tags (book_id, tag_id, source) VALUES (?1, ?2, 'user')
                 ON CONFLICT(book_id, tag_id) DO UPDATE SET source = 'user'",
                rusqlite::params![book.0, tag],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The tag's id, made when new.
    fn tag_id(tx: &rusqlite::Transaction, name: &str) -> Result<i64> {
        tx.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
        Ok(tx.query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            [name],
            |row| row.get(0),
        )?)
    }
}
