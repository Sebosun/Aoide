//! Listening time: counted while playing and saved per book and day (summed with reading time
//! by `Core::activity_stats`).

use super::*;

/// Listening time is written to the database at least this often while playing.
const LISTEN_FLUSH_MS: u64 = 15_000;

/// Listening time not yet saved.
#[derive(Default)]
struct Tally {
    book: Option<String>,
    last_tick: Option<Instant>,
    pending_ms: u64,
}

/// Counts listening time and sums it for the stats.
pub(super) struct Listening {
    library: Arc<Mutex<Library>>,
    reporter: error::Reporter,
    tally: Mutex<Tally>,
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

impl Listening {
    pub fn new(library: Arc<Mutex<Library>>, reporter: error::Reporter) -> Self {
        Self {
            library,
            reporter,
            tally: Default::default(),
        }
    }

    /// Called every tick: accumulates real playing time and saves it per local day.
    pub fn track(&self, state: &PlayerStateView) {
        let now = Instant::now();
        let playing = state.status == PlaybackStatus::Playing;
        let mut tally = lock(&self.tally);
        let switched = tally.book.as_deref() != state.book_id.as_deref();
        if let (true, false, Some(last)) = (playing, switched, tally.last_tick) {
            // Cap the step so a suspended machine does not count as listening.
            tally.pending_ms += now.duration_since(last).as_millis().min(1_000) as u64;
        }
        let flush =
            tally.pending_ms > 0 && (switched || !playing || tally.pending_ms >= LISTEN_FLUSH_MS);
        if flush {
            if let Some(book) = tally.book.clone() {
                let ms = std::mem::take(&mut tally.pending_ms);
                drop(tally);
                let saved = lock(&self.library).record_listening(&BookId(book), &today(), ms);
                if let Err(error) = saved {
                    self.reporter.report(
                        "listening.save",
                        "Could not save your listening time",
                        error,
                    );
                }
                tally = lock(&self.tally);
            } else {
                tally.pending_ms = 0;
            }
        }
        tally.book = state.book_id.clone();
        tally.last_tick = playing.then_some(now);
    }

    /// One book's listened time per local day, oldest first.
    pub fn book(&self, id: &str) -> Result<Vec<aoide_library::DayListening>, String> {
        lock(&self.library)
            .book_listening(&BookId(id.into()))
            .map_err(|e| e.to_string())
    }
}

impl Core {
    /// One book's listened time per local day, oldest first.
    pub fn book_listening(&self, id: &str) -> Result<Vec<aoide_library::DayListening>, String> {
        self.listening.book(id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::library_with;
    use super::*;

    fn heard(book: &str, status: PlaybackStatus) -> PlayerStateView {
        PlayerStateView {
            book_id: Some(book.into()),
            status,
            chapter: 0,
            segment_id: None,
            word_index: None,
            speed: 1.0,
            volume: 1.0,
            chapter_fraction: 0.0,
            error: None,
            sleep: None,
            source: Default::default(),
            off_text: false,
            chapter_left_ms: None,
            waiting_for_sync: false,
        }
    }

    #[test]
    fn time_spent_playing_is_saved_for_the_book_once_it_pauses() {
        let listening = Listening::new(library_with(&["a"]), Default::default());
        listening.track(&heard("a", PlaybackStatus::Playing));
        std::thread::sleep(Duration::from_millis(40));
        listening.track(&heard("a", PlaybackStatus::Playing));
        assert!(
            listening.book("a").unwrap().is_empty(),
            "not saved while playing"
        );

        listening.track(&heard("a", PlaybackStatus::Paused));

        let days = listening.book("a").unwrap();
        assert_eq!(days.len(), 1);
        assert!(days[0].ms >= 40, "{days:?}");
        let stats = listening.library.lock().unwrap().activity_stats().unwrap();
        assert_eq!(stats.listening_ms, days[0].ms);
    }

    #[test]
    fn time_paused_is_not_counted() {
        let listening = Listening::new(library_with(&["a"]), Default::default());
        listening.track(&heard("a", PlaybackStatus::Paused));
        std::thread::sleep(Duration::from_millis(20));
        listening.track(&heard("a", PlaybackStatus::Paused));
        assert!(listening.book("a").unwrap().is_empty());
    }
}
