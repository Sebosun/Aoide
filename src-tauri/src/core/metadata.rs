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

/// Details about books beyond their EPUB's: looked up on Open Library and stored.
pub(super) struct Metadata {
    library: Arc<Mutex<Library>>,
    covers_dir: PathBuf,
    reporter: error::Reporter,
}

const TURNED_OFF: &str = "Online lookups are turned off in Settings.";

impl Metadata {
    pub fn new(
        library: Arc<Mutex<Library>>,
        covers_dir: PathBuf,
        reporter: error::Reporter,
    ) -> Self {
        Self {
            library,
            covers_dir,
            reporter,
        }
    }

    /// Candidate matches for `book`; only its title, first author and ISBN are sent. Results
    /// are cached in the library for 30 days; `refresh` asks Open Library again. `allowed`:
    /// online lookups are turned on.
    pub async fn lookup(
        &self,
        book: &BookModel,
        allowed: bool,
        refresh: bool,
    ) -> Result<Vec<Match>, String> {
        if !allowed {
            return Err(TURNED_OFF.into());
        }
        let stored = lock(&self.library)
            .book_details(&BookId(book.id.clone()))
            .ok()
            .flatten();
        let query = Query {
            isbn: stored.and_then(|d| d.isbn).or_else(|| book.isbn.clone()),
            title: book.title.clone(),
            authors: book.authors.clone(),
        };
        self.search(&query, refresh).await
    }

    /// Open Library matches for a query, through the 30-day SQLite cache.
    pub async fn search(&self, query: &Query, refresh: bool) -> Result<Vec<Match>, String> {
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
        let started = std::time::Instant::now();
        let searched = client()?.search(query).await;
        match &searched {
            Ok(found) => tracing::debug!(
                event = "http",
                service = "openlibrary",
                op = "search",
                found = found.len(),
                took_ms = crate::logging::took(started),
                outcome = "ok",
                "Searched Open Library"
            ),
            Err(error) => {
                tracing::warn!(event = "http", service = "openlibrary", op = "search", took_ms = crate::logging::took(started), outcome = "failed", %error, "Open Library search failed")
            }
        }
        let matches = searched.map_err(|e| e.to_string())?;
        if let Ok(json) = serde_json::to_string(&matches) {
            let _ = lock(&self.library).store_lookup(&cache_key, &json, now_ms());
        }
        Ok(matches)
    }

    /// The best cached match for a query, without any network request.
    pub fn cached_match(&self, query: &Query) -> Option<Match> {
        let (json, _) = lock(&self.library)
            .cached_lookup(&cache_key(query))
            .ok()
            .flatten()?;
        serde_json::from_str::<Vec<Match>>(&json)
            .ok()?
            .into_iter()
            .next()
    }

    /// Stores the match picked for `book`. The book's own description and cover win when it
    /// has them. Returns the details now stored.
    pub async fn apply(
        &self,
        book: &BookModel,
        picked: Match,
        allowed: bool,
    ) -> Result<BookDetails, String> {
        if !allowed {
            return Err(TURNED_OFF.into());
        }
        let id = BookId(book.id.clone());
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
            .set_book_details(&id, &details)
            .map_err(|e| e.to_string())?;
        let record = lock(&self.library).book(&id).map_err(|e| e.to_string())?;
        if let (Some(record), Some(url)) = (record, picked.cover_url.as_deref()) {
            if record.cover_path.is_none() {
                if let Ok((media_type, bytes)) = client.cover(&large_cover(url)).await {
                    let ext = if media_type == "image/png" {
                        "png"
                    } else {
                        "jpg"
                    };
                    let path = self.covers_dir.join(format!("{}-online.{ext}", id.0));
                    if std::fs::write(&path, bytes).is_ok() {
                        let saved = lock(&self.library).upsert_book(
                            &NewBook {
                                id: record.id,
                                path: record.path,
                                title: record.title,
                                authors: record.authors,
                                language: record.language,
                                cover_path: Some(path),
                                error: None,
                                kind: aoide_library::ItemKind::Book,
                            },
                            now_ms(),
                        );
                        if let Err(error) = saved {
                            self.reporter.report(
                                "metadata.apply",
                                "Could not save the cover found online",
                                error,
                            );
                        }
                    }
                }
            }
        }
        self.details(book)
    }

    /// Stored details over the EPUB's own metadata.
    pub fn details(&self, book: &BookModel) -> Result<BookDetails, String> {
        let stored = lock(&self.library)
            .book_details(&BookId(book.id.clone()))
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        Ok(BookDetails {
            description: stored.description.or_else(|| book.description.clone()),
            publisher: stored.publisher.or_else(|| book.publisher.clone()),
            isbn: stored.isbn.or_else(|| book.isbn.clone()),
            published: stored.published,
            subjects: stored.subjects,
            open_library_key: stored.open_library_key,
        })
    }
}

impl Core {
    /// Candidate matches for the book from Open Library.
    pub async fn lookup_metadata(&self, id: &str, refresh: bool) -> Result<Vec<Match>, String> {
        let book = self.load_book(id)?;
        let allowed = self.app_settings().metadata_lookup;
        self.metadata.lookup(&book, allowed, refresh).await
    }

    /// Stores the picked match.
    pub async fn apply_metadata(&self, id: &str, picked: Match) -> Result<BookDetails, String> {
        let book = self.load_book(id)?;
        let allowed = self.app_settings().metadata_lookup;
        let details = self.metadata.apply(&book, picked, allowed).await?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(details)
    }

    /// Stored details over the EPUB's own metadata.
    pub fn book_details(&self, id: &str) -> Result<BookDetails, String> {
        self.metadata.details(&*self.load_book(id)?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{epub, library_with};
    use super::*;

    fn metadata(dir: &Path) -> Metadata {
        Metadata::new(library_with(&["b"]), dir.to_path_buf(), Default::default())
    }

    fn book() -> BookModel {
        BookModel::parse("b", &epub(), false).unwrap()
    }

    fn found(key: &str) -> Match {
        Match {
            key: key.into(),
            title: "A Title".into(),
            authors: vec!["Someone".into()],
            published: Some("1999".into()),
            publisher: Some("A Press".into()),
            isbn: None,
            subjects: vec!["Rivers".into()],
            cover_url: None,
        }
    }

    #[tokio::test]
    async fn a_lookup_answered_in_the_last_month_is_taken_from_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let metadata = metadata(dir.path());
        let book = book();
        let query = Query {
            isbn: book.isbn.clone(),
            title: book.title.clone(),
            authors: book.authors.clone(),
        };
        let json = serde_json::to_string(&vec![found("/works/OL1W")]).unwrap();
        lock(&metadata.library)
            .store_lookup(&cache_key(&query), &json, now_ms())
            .unwrap();

        let matches = metadata.lookup(&book, true, false).await.unwrap();

        assert_eq!(matches[0].key, "/works/OL1W");
        assert_eq!(metadata.cached_match(&query).unwrap().key, "/works/OL1W");
        assert!(
            metadata.lookup(&book, false, false).await.is_err(),
            "turned off"
        );
    }

    #[tokio::test]
    async fn a_picked_match_is_stored_over_the_books_own_details() {
        let dir = tempfile::tempdir().unwrap();
        let metadata = metadata(dir.path());
        let mut book = book();
        book.description = Some("Its own words.".into());

        let details = metadata
            .apply(&book, found("/works/OL2W"), true)
            .await
            .unwrap();

        assert_eq!(details.description.as_deref(), Some("Its own words."));
        assert_eq!(details.publisher.as_deref(), Some("A Press"));
        assert_eq!(details.open_library_key.as_deref(), Some("/works/OL2W"));
        assert_eq!(metadata.details(&book).unwrap().subjects, ["Rivers"]);
    }

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
