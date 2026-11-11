//! Online metadata lookup (Open Library) and applying a match the user picked.

use super::*;

use aoide_library::BookDetails;
use aoide_metadata::{Match, OpenLibrary, Options, Query};

fn client() -> Result<OpenLibrary, String> {
    OpenLibrary::new(Options::default()).map_err(|e| e.to_string())
}

const CACHE_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Normalised cache key of a lookup query.
fn cache_key(query: &Query) -> String {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    format!(
        "openlibrary:{}|{}|{}",
        query.isbn.as_deref().map(norm).unwrap_or_default(),
        norm(&query.title),
        query.authors.first().map(|a| norm(a)).unwrap_or_default()
    )
}

/// The large variant of a medium cover URL from a search result.
fn large_cover(url: &str) -> String {
    url.strip_suffix("-M.jpg")
        .map_or_else(|| url.to_owned(), |stem| format!("{stem}-L.jpg"))
}

impl Core {
    /// Candidate matches for the book; only its title, first author and ISBN are sent. Results
    /// are cached in the library for 30 days; `refresh` asks Open Library again.
    pub async fn lookup_metadata(&self, id: &str, refresh: bool) -> Result<Vec<Match>, String> {
        if !self.app_settings().metadata_lookup {
            return Err("Online lookups are turned off in Settings.".into());
        }
        let book = self.load_book(id)?;
        let stored = lock(&self.library)
            .book_details(&BookId(id.into()))
            .ok()
            .flatten();
        let query = Query {
            isbn: stored.and_then(|d| d.isbn).or_else(|| book.isbn.clone()),
            title: book.title.clone(),
            authors: book.authors.clone(),
        };
        self.lookup_query(&query, refresh).await
    }

    /// Open Library matches for a query, through the 30-day SQLite cache.
    pub(super) async fn lookup_query(
        &self,
        query: &Query,
        refresh: bool,
    ) -> Result<Vec<Match>, String> {
        let cache_key = cache_key(query);
        if !refresh {
            let cached = lock(&self.library).cached_lookup(&cache_key).ok().flatten();
            if let Some((json, fetched_at)) = cached {
                if now_ms() - fetched_at < CACHE_TTL_MS {
                    if let Ok(matches) = serde_json::from_str(&json) {
                        return Ok(matches);
                    }
                }
            }
        }
        let matches = client()?.search(query).await.map_err(|e| e.to_string())?;
        if let Ok(json) = serde_json::to_string(&matches) {
            let _ = lock(&self.library).store_lookup(&cache_key, &json, now_ms());
        }
        Ok(matches)
    }

    /// The best cached match for a query, without any network request.
    pub(super) fn cached_match(&self, query: &Query) -> Option<Match> {
        let (json, _) = lock(&self.library)
            .cached_lookup(&cache_key(query))
            .ok()
            .flatten()?;
        serde_json::from_str::<Vec<Match>>(&json)
            .ok()?
            .into_iter()
            .next()
    }

    /// Stores the picked match. The book's own description and cover win when it has them.
    pub async fn apply_metadata(&self, id: &str, picked: Match) -> Result<BookDetails, String> {
        if !self.app_settings().metadata_lookup {
            return Err("Online lookups are turned off in Settings.".into());
        }
        let book = self.load_book(id)?;
        let client = client()?;
        let description = match &book.description {
            Some(own) => Some(own.clone()),
            None => client
                .description(&picked.key)
                .await
                .ok()
                .flatten()
                .map(|d| crate::book::plain_text(&d)),
        };
        let details = BookDetails {
            description,
            publisher: picked.publisher.clone().or_else(|| book.publisher.clone()),
            isbn: picked.isbn.clone().or_else(|| book.isbn.clone()),
            published: picked.published.clone(),
            subjects: picked.subjects.clone(),
            open_library_key: Some(picked.key.clone()),
        };
        lock(&self.library)
            .set_book_details(&BookId(id.into()), &details)
            .map_err(|e| e.to_string())?;
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        if let (Some(record), Some(url)) = (record, picked.cover_url.as_deref()) {
            if record.cover_path.is_none() {
                if let Ok((media_type, bytes)) = client.cover(&large_cover(url)).await {
                    let ext = if media_type == "image/png" {
                        "png"
                    } else {
                        "jpg"
                    };
                    let path = self.paths.covers_dir.join(format!("{id}-online.{ext}"));
                    if std::fs::write(&path, bytes).is_ok() {
                        let _ = lock(&self.library).upsert_book(
                            &NewBook {
                                id: record.id,
                                path: record.path,
                                title: record.title,
                                authors: record.authors,
                                language: record.language,
                                cover_path: Some(path),
                                error: None,
                            },
                            now_ms(),
                        );
                    }
                }
            }
        }
        self.emit(CoreEvent::LibraryChanged);
        self.book_details(id)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cache_keys_ignore_case_and_spacing() {
        let query = |title: &str| aoide_metadata::Query {
            isbn: None,
            title: title.into(),
            authors: vec!["Mira  Vale".into()],
        };
        assert_eq!(
            super::cache_key(&query("Quiet  Places")),
            super::cache_key(&query("quiet places"))
        );
        assert_eq!(
            super::cache_key(&query("Quiet")),
            "openlibrary:|quiet|mira vale"
        );
    }

    #[test]
    fn large_cover_swaps_the_size_suffix() {
        assert_eq!(
            super::large_cover("https://covers.openlibrary.org/b/id/42-M.jpg"),
            "https://covers.openlibrary.org/b/id/42-L.jpg"
        );
        assert_eq!(super::large_cover("https://x/y.png"), "https://x/y.png");
    }
}
