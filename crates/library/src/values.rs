//! Stored values: app-wide and per-book JSON values, and cached metadata lookups.

use std::collections::HashMap;

use super::*;

/// Every value stored for the whole app, each as JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalKey {
    /// Books the user removed, which a rescan leaves out.
    IgnoredBooks,
    /// The books queued for preparing, in order.
    PrepareQueue,
    /// The queued books that are paused.
    PreparePaused,
    /// The queued books being prepared for some chapters only, with those chapters.
    PrepareChapters,
    /// Settings from before `settings.toml`, read once to move them there.
    OldEngineProfile,
    OldPlayerSpeed,
    OldAppSettings,
    /// The library's books have had their subjects read for tags (books listed before tags
    /// came have theirs read once).
    SubjectsRead,
    /// The library's books have had their series read (books listed before series came have
    /// theirs read once).
    SeriesRead,
}

impl GlobalKey {
    fn name(self) -> &'static str {
        match self {
            GlobalKey::IgnoredBooks => "library.ignored",
            GlobalKey::PrepareQueue => "prepare.queue",
            GlobalKey::PreparePaused => "prepare.paused",
            GlobalKey::PrepareChapters => "prepare.chapters",
            GlobalKey::OldEngineProfile => "engine.profile",
            GlobalKey::OldPlayerSpeed => "player.speed",
            GlobalKey::OldAppSettings => "app.settings",
            GlobalKey::SubjectsRead => "tags.subjects-read",
            GlobalKey::SeriesRead => "library.series-read",
        }
    }
}

/// Every value stored for one book, each as JSON. Some outlive the book (kept if it is removed
/// and added again); the rest are removed with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookKey {
    /// Its voice, speed and narration choice. Outlives the book.
    Settings,
    /// How far into the book the listener is, from 0 to 1. Outlives the book.
    ProgressFraction,
    /// How many segments made it prepared (0: not prepared). Removed with the book.
    Prepared,
    /// Where on its recording the listener was, kept from the audiobook it was merged from
    /// until the sync reaches it (milliseconds). Removed with the book.
    PendingPlace,
    /// The details the user corrected by hand (title, authors, cover…). Outlives the book.
    Details,
    /// How much listening time shortened silences saved in its recording (milliseconds).
    /// Removed with the book.
    SilenceSaved,
}

/// Stands for the book's id in a value's name, to find every book's.
const ID: &str = "\u{1}";

/// Where a book's value is stored.
enum Place {
    /// In `settings`, under this name: kept when the book is removed.
    Settings(String),
    /// In `book_values`, under this key: removed with the book.
    Book(&'static str),
}

impl BookKey {
    fn place(self, id: &BookId) -> Place {
        match self {
            BookKey::Settings => Place::Settings(format!("book.{}.settings", id.0)),
            BookKey::ProgressFraction => Place::Settings(format!("progress.fraction.{}", id.0)),
            BookKey::Prepared => Place::Book("prepared"),
            BookKey::PendingPlace => Place::Book(PENDING_PLACE),
            BookKey::Details => Place::Settings(format!("book.{}.details", id.0)),
            BookKey::SilenceSaved => Place::Book("silence-saved"),
        }
    }
}

impl Library {
    /// A stored lookup result and when it was fetched, by normalised query.
    pub fn cached_lookup(&self, query: &str) -> Result<Option<(String, i64)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT json, fetched_at FROM metadata_cache WHERE query = ?1",
                [query],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?)
    }

    /// Stores (or replaces) a lookup result.
    pub fn store_lookup(&self, query: &str, json: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO metadata_cache (query, json, fetched_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(query) DO UPDATE SET json = excluded.json, fetched_at = excluded.fetched_at",
            rusqlite::params![query, json, now],
        )?;
        Ok(())
    }

    /// The value stored under `key`, if any; one that is not a `T` is [`LibraryError::Corrupt`].
    pub fn value<T: serde::de::DeserializeOwned>(&self, key: GlobalKey) -> Result<Option<T>> {
        self.setting(key.name())?
            .map(|text| store::parse_json(&text))
            .transpose()
    }

    pub fn set_value<T: serde::Serialize + ?Sized>(&self, key: GlobalKey, value: &T) -> Result<()> {
        self.set_setting(key.name(), &store::json(&value)?)
    }

    pub fn remove_value(&self, key: GlobalKey) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [key.name()])?;
        Ok(())
    }

    /// Book `id`'s value under `key`, if any; one that is not a `T` is
    /// [`LibraryError::Corrupt`].
    pub fn book_value<T: serde::de::DeserializeOwned>(
        &self,
        id: &BookId,
        key: BookKey,
    ) -> Result<Option<T>> {
        let text = match key.place(id) {
            Place::Settings(name) => self.setting(&name)?,
            Place::Book(name) => self
                .conn
                .query_row(
                    "SELECT value FROM book_values WHERE book_id = ?1 AND key = ?2",
                    [&id.0, name],
                    |row| row.get(0),
                )
                .optional()?,
        };
        text.map(|text: String| store::parse_json(&text))
            .transpose()
    }

    /// Stores book `id`'s value under `key`. A value that goes with the book needs the book in
    /// the library.
    pub fn set_book_value<T: serde::Serialize + ?Sized>(
        &self,
        id: &BookId,
        key: BookKey,
        value: &T,
    ) -> Result<()> {
        let text = store::json(&value)?;
        match key.place(id) {
            Place::Settings(name) => self.set_setting(&name, &text),
            Place::Book(name) => {
                self.conn.execute(
                    "INSERT INTO book_values (book_id, key, value) VALUES (?1, ?2, ?3)
                     ON CONFLICT(book_id, key) DO UPDATE SET value = excluded.value",
                    [&id.0, name, &text],
                )?;
                Ok(())
            }
        }
    }

    /// Every book's value under `key`, by book: what listing the library reads at once instead
    /// of book by book. A value that is not a `T` is that book's [`LibraryError::Corrupt`].
    pub fn book_values<T: serde::de::DeserializeOwned>(
        &self,
        key: BookKey,
    ) -> Result<HashMap<BookId, Result<T>>> {
        let rows: Vec<(BookId, String)> = match key.place(&BookId(ID.into())) {
            Place::Settings(name) => {
                let (prefix, suffix) = name.split_once(ID).expect("the name holds the id");
                let mut stmt = self
                    .conn
                    .prepare("SELECT key, value FROM settings WHERE key LIKE ?1")?;
                let rows = stmt.query_map([format!("{prefix}%{suffix}")], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                let mut found = Vec::new();
                for row in rows {
                    let (name, value) = row?;
                    let id = name
                        .strip_prefix(prefix)
                        .and_then(|rest| rest.strip_suffix(suffix));
                    if let Some(id) = id {
                        found.push((BookId(id.into()), value));
                    }
                }
                found
            }
            Place::Book(name) => {
                let mut stmt = self
                    .conn
                    .prepare("SELECT book_id, value FROM book_values WHERE key = ?1")?;
                let rows = stmt.query_map([name], |row| {
                    Ok((BookId(row.get(0)?), row.get::<_, String>(1)?))
                })?;
                rows.collect::<std::result::Result<_, _>>()?
            }
        };
        Ok(rows
            .into_iter()
            .map(|(id, text)| (id, store::parse_json(&text)))
            .collect())
    }

    pub fn remove_book_value(&self, id: &BookId, key: BookKey) -> Result<()> {
        match key.place(id) {
            Place::Settings(name) => {
                self.conn
                    .execute("DELETE FROM settings WHERE key = ?1", [&name])?;
            }
            Place::Book(name) => {
                self.conn.execute(
                    "DELETE FROM book_values WHERE book_id = ?1 AND key = ?2",
                    [&id.0, name],
                )?;
            }
        }
        Ok(())
    }

    fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }
}
