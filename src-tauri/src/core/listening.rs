//! Listening time: counted while playing, saved per book and day, and summed for the stats.

use super::*;

/// Listening time is written to the database at least this often while playing.
const LISTEN_FLUSH_MS: u64 = 15_000;

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
    pub(super) last_eviction: Option<Instant>,
    /// Book being exported, if any.
    pub(super) exporting: Option<String>,
    /// Set while a "Find books" scan runs; storing `true` cancels it.
    pub(super) scan_cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl Core {
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
                let saved = lock(&self.library).record_listening(&BookId(book), &today(), ms);
                if let Err(error) = saved {
                    self.report(
                        "listening.save",
                        "Could not save your listening time",
                        error,
                    );
                }
                extras = lock(&self.extras);
            } else {
                extras.pending_ms = 0;
            }
        }
        extras.listening_book = state.book_id.clone();
        extras.last_tick = playing.then_some(now);
    }

    /// One book's listened time per local day, oldest first.
    pub fn book_listening(&self, id: &str) -> Result<Vec<aoide_library::DayListening>, String> {
        lock(&self.library)
            .book_listening(&BookId(id.into()))
            .map_err(|e| e.to_string())
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
}
