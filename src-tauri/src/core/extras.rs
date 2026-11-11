//! Bookmarks, listening statistics, book details, app settings and audio-cache limits.

use super::*;

use aoide_library::{BookDetails, Bookmark};

/// Listening time is written to the database at least this often while playing.
const LISTEN_FLUSH_MS: u64 = 15_000;
/// Cache-size checks run at most this often.
const EVICTION_INTERVAL: Duration = Duration::from_secs(20);
const EVICTION_BATCH: usize = 400;

/// User preferences that are not part of an engine profile.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppSettings {
    /// Look books up on Open Library when the user asks for details.
    pub metadata_lookup: bool,
    /// Delete the oldest generated audio above this many bytes.
    pub cache_limit_bytes: Option<u64>,
    /// Local forced-alignment service for exact word timings; `None` uses Aoide's estimate.
    pub aligner: Option<AlignerSettings>,
    /// Library order: "recent", "title", "author" or "progress".
    pub library_sort: LibrarySort,
    /// Folders searched by "Find books".
    pub scan: super::discovery::ScanSettings,
    /// Carry on preparing queued books when Aoide starts.
    pub resume_prepare: bool,
}

/// Library order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum LibrarySort {
    Title,
    Author,
    Progress,
    /// Also used for an order this version does not know.
    #[default]
    #[serde(other)]
    Recent,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            metadata_lookup: true,
            cache_limit_bytes: None,
            aligner: None,
            library_sort: LibrarySort::Recent,
            scan: Default::default(),
            resume_prepare: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AlignerSettings {
    pub base_url: String,
    #[serde(default)]
    pub network: NetworkScope,
    /// Optional command that starts the aligner when it is not running.
    #[serde(default)]
    pub launch: Option<LaunchCommand>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookListeningView {
    pub book_id: String,
    pub title: String,
    pub ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "ListeningStats"))]
pub struct ListeningStatsView {
    pub total_ms: u64,
    pub by_book: Vec<BookListeningView>,
    pub by_day: Vec<aoide_library::DayListening>,
}

/// Mutable bookkeeping for listening time and cache eviction.
#[derive(Default)]
pub struct ExtrasState {
    listening_book: Option<String>,
    last_tick: Option<Instant>,
    pending_ms: u64,
    last_eviction: Option<Instant>,
    /// Book being exported, if any.
    pub(super) exporting: Option<String>,
    /// Segments waiting for exact alignment: (book id, playlist index, cache key).
    pub(super) align_queue: std::collections::VecDeque<(String, usize, String)>,
    /// Cache keys already aligned or given up on in this run.
    pub(super) align_done: HashSet<String>,
    pub(super) aligner_child: Option<std::process::Child>,
    pub(super) aligner_launched: bool,
    /// Set while a "Find books" scan runs; storing `true` cancels it.
    pub(super) scan_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn excerpt(text: &str) -> String {
    const MAX: usize = 160;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX {
        return text;
    }
    let cut: String = text.chars().take(MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

impl Core {
    // ------------------------------------------------------------ bookmarks

    pub fn bookmarks(&self, id: &str) -> Result<Vec<Bookmark>, String> {
        lock(&self.library)
            .bookmarks(&BookId(id.into()))
            .map_err(|e| e.to_string())
    }

    /// Bookmarks the playhead when `id` is playing, else the book's saved position.
    pub fn add_bookmark(&self, id: &str, note: &str) -> Result<Bookmark, String> {
        let playing_index = lock(&self.session)
            .as_ref()
            .filter(|s| s.target.book.id == id)
            .map(|_| lock(&self.player).position().index);
        let book = self.load_book(id)?;
        let index = match playing_index {
            Some(index) => index.min(book.total_entries().saturating_sub(1)),
            None => {
                let progress = lock(&self.library)
                    .progress(&BookId(id.into()))
                    .map_err(|e| e.to_string())?;
                progress
                    .and_then(|p| {
                        p.segment_id
                            .as_deref()
                            .and_then(|s| book.index_of(s))
                            .or_else(|| book.index_at(p.chapter as usize, p.char_offset as usize))
                    })
                    .or_else(|| book.chapter_start(0))
                    .unwrap_or(0)
            }
        };
        let (chapter, local) = book.locate(index).ok_or("the book has no text")?;
        let segment = &book.chapters[chapter].segments[local];
        let library = lock(&self.library);
        let bookmark_id = library
            .add_bookmark(
                &BookId(id.into()),
                chapter as u32,
                segment.range.start as u32,
                Some(&segment.id),
                &excerpt(&segment.display),
                note.trim(),
                now_ms() / 1000,
            )
            .map_err(|e| e.to_string())?;
        library
            .bookmarks(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|b| b.id == bookmark_id)
            .ok_or_else(|| "bookmark was not saved".into())
    }

    pub fn update_bookmark_note(&self, bookmark: i64, note: &str) -> Result<(), String> {
        let found = lock(&self.library)
            .update_bookmark_note(bookmark, note.trim())
            .map_err(|e| e.to_string())?;
        found
            .then_some(())
            .ok_or_else(|| "bookmark not found".into())
    }

    pub fn delete_bookmark(&self, bookmark: i64) -> Result<(), String> {
        lock(&self.library)
            .delete_bookmark(bookmark)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    // ------------------------------------------------------------ listening time

    /// Called every tick: accumulates real playing time and flushes it per local day.
    pub(super) fn track_listening(&self, state: &PlayerStateView) {
        let now = Instant::now();
        let playing = state.status == PlaybackStatus::Playing;
        let mut extras = lock(&self.extras);
        let switched = extras.listening_book.as_deref() != state.book_id.as_deref();
        if let (true, false, Some(last)) = (playing, switched, extras.last_tick) {
            // Cap the step so a suspended machine does not count as listening.
            extras.pending_ms += now.duration_since(last).as_millis().min(1_000) as u64;
        }
        let flush =
            extras.pending_ms > 0 && (switched || !playing || extras.pending_ms >= LISTEN_FLUSH_MS);
        if flush {
            if let Some(book) = extras.listening_book.clone() {
                let ms = std::mem::take(&mut extras.pending_ms);
                drop(extras);
                let _ = lock(&self.library).record_listening(&BookId(book), &today(), ms);
                extras = lock(&self.extras);
            } else {
                extras.pending_ms = 0;
            }
        }
        extras.listening_book = state.book_id.clone();
        extras.last_tick = playing.then_some(now);
    }

    pub fn listening_stats(&self) -> Result<ListeningStatsView, String> {
        let library = lock(&self.library);
        let stats = library.listening_stats().map_err(|e| e.to_string())?;
        let by_book = stats
            .by_book
            .into_iter()
            .map(|b| BookListeningView {
                title: library
                    .book(&b.book_id)
                    .ok()
                    .flatten()
                    .and_then(|r| r.title)
                    .unwrap_or_else(|| "Untitled".into()),
                book_id: b.book_id.0,
                ms: b.ms,
            })
            .collect();
        Ok(ListeningStatsView {
            total_ms: stats.total_ms,
            by_book,
            by_day: stats.by_day,
        })
    }

    // ------------------------------------------------------------ details

    /// Stored details over the EPUB's own metadata.
    pub fn book_details(&self, id: &str) -> Result<BookDetails, String> {
        let book = self.load_book(id)?;
        let stored = lock(&self.library)
            .book_details(&BookId(id.into()))
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

    // ------------------------------------------------------------ app settings

    pub fn app_settings(&self) -> AppSettings {
        lock(&self.config)
            .get::<AppSettings>(super::config::APP)
            .unwrap_or_default()
    }

    pub fn save_app_settings(&self, settings: AppSettings) -> Result<(), String> {
        if let Some(aligner) = &settings.aligner {
            aoide_aligner::Aligner::new(aoide_aligner::Options {
                base_url: aligner.base_url.clone(),
                network: aligner.network,
                ..Default::default()
            })
            .map_err(|e| e.to_string())?;
        }
        lock(&self.config).update(|file| file.set_section(super::config::APP, &settings))?;
        lock(&self.extras).last_eviction = None;
        self.enforce_cache_limit();
        Ok(())
    }

    // ------------------------------------------------------------ cache limit

    /// Deletes the oldest generated audio until the cache fits the configured limit. Audio of
    /// the playing book and of books being prepared is kept. Throttled.
    pub(super) fn enforce_cache_limit(&self) {
        {
            let mut extras = lock(&self.extras);
            if extras
                .last_eviction
                .is_some_and(|at| at.elapsed() < EVICTION_INTERVAL)
            {
                return;
            }
            extras.last_eviction = Some(Instant::now());
        }
        let Some(limit) = self.app_settings().cache_limit_bytes else {
            return;
        };
        let playing = lock(&self.session)
            .as_ref()
            .map(|s| BookId(s.target.book.id.clone()));
        let mut protected: HashSet<String> = lock(&self.preparation).queued().into_iter().collect();
        protected.extend(lock(&self.extras).exporting.clone());
        let mut evicted_books = HashSet::new();
        let mut evicted_keys = HashSet::new();
        loop {
            let library = lock(&self.library);
            let Ok(total) = library.total_audio_bytes() else {
                return;
            };
            if total <= limit {
                break;
            }
            // Read well past the batch: protected books' old audio may come first.
            let Ok(oldest) = library.oldest_segments(playing.as_ref(), EVICTION_BATCH * 25) else {
                return;
            };
            let mut excess = total - limit;
            let victims: Vec<CachedSegment> = oldest
                .into_iter()
                .filter(|s| !protected.contains(&s.book_id.0))
                .take(EVICTION_BATCH)
                .take_while(|s| {
                    let take = excess > 0;
                    excess = excess.saturating_sub(s.bytes);
                    take
                })
                .collect();
            if victims.is_empty() {
                break; // Everything left belongs to protected books.
            }
            let keys: Vec<String> = victims.iter().map(|s| s.key.clone()).collect();
            if library.delete_segments(&keys).is_err() {
                return;
            }
            drop(library);
            for victim in victims {
                let _ = std::fs::remove_file(self.paths.audio_dir.join(&victim.file));
                evicted_books.insert(victim.book_id.0);
                evicted_keys.insert(victim.key);
            }
        }
        if evicted_keys.is_empty() {
            return;
        }
        if let Some(session) = lock(&self.session).as_mut() {
            session.audio_lost(&evicted_keys);
        }
        lock(&self.worker).forget(evicted_keys.iter().cloned());
        {
            let mut preparation = lock(&self.preparation);
            preparation.audio_lost(&lock(&self.library), &evicted_books, &evicted_keys);
        }
        self.emit(CoreEvent::LibraryChanged);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn excerpt_collapses_whitespace_and_cuts_at_a_word() {
        assert_eq!(super::excerpt("A  short\n line."), "A short line.");
        let long = "word ".repeat(60);
        let cut = super::excerpt(&long);
        assert!(cut.ends_with("word…"), "{cut}");
        assert!(cut.chars().count() <= 161);
    }

    #[test]
    fn settings_saved_before_resume_prepare_existed_keep_resuming() {
        let old: super::AppSettings =
            serde_json::from_str(r#"{"metadataLookup":false,"librarySort":"title"}"#).unwrap();
        assert!(old.resume_prepare);
        assert!(!old.metadata_lookup);
        let off: super::AppSettings = serde_json::from_str(r#"{"resumePrepare":false}"#).unwrap();
        assert!(!off.resume_prepare);
    }
}
