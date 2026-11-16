//! Reading without listening: whether an item is read or listened to ([`ReadingMode`]), what the
//! reader screen is for it ([`Shape`]), whether it is unread, in progress or finished
//! ([`ReadState`]), and the time spent reading it.
//!
//! Mode and read state each change only through their transition function (`next_mode`,
//! `next_read`) over named events. The reading-time rule (`ReadingClock`) takes the time as an
//! argument, so it is tested with a supplied clock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use aoide_library::{BookId, Library};
use chrono::{NaiveDateTime, NaiveTime, TimeDelta};
use serde::{Deserialize, Serialize};

use super::error::Reporter;
use super::facets::AudioFacts;
use super::{lock, BookView, CoreEvent};

/// How the user takes in a text item, chosen per item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ReadingMode {
    /// Silently: no player.
    Read,
    /// With its narration playing.
    Listen,
}

/// What the reader screen is for an item. The reader switches on this and nothing else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Shape {
    /// Text, no player.
    Read,
    /// Text with the player.
    ReadAlong,
    /// Audio only (an item with no text).
    Listen,
}

/// What the user can do to an item's mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeEvent {
    ListenChosen,
    ReadChosen,
}

pub fn next_mode(_mode: ReadingMode, event: ModeEvent) -> ReadingMode {
    match event {
        ModeEvent::ListenChosen => ReadingMode::Listen,
        ModeEvent::ReadChosen => ReadingMode::Read,
    }
}

/// The mode of an item the user has not chosen one for: listening when it has been set up or
/// used for listening, so books listened to before keep opening as they did.
pub fn default_mode(
    has_recording: bool,
    has_cached_speech: bool,
    has_listening_history: bool,
) -> ReadingMode {
    if has_recording || has_cached_speech || has_listening_history {
        ReadingMode::Listen
    } else {
        ReadingMode::Read
    }
}

/// The mode the user chose, else the default for what the item is set up with.
pub fn mode_for(chosen: Option<ReadingMode>, facts: &AudioFacts) -> ReadingMode {
    chosen.unwrap_or_else(|| {
        default_mode(
            facts.narrates_recording,
            facts.cached_speech,
            facts.listened,
        )
    })
}

pub fn shape(has_text: bool, mode: ReadingMode) -> Shape {
    match (has_text, mode) {
        (false, _) => Shape::Listen,
        (true, ReadingMode::Read) => Shape::Read,
        (true, ReadingMode::Listen) => Shape::ReadAlong,
    }
}

/// How far the user is with an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ReadState {
    /// No place saved and not finished.
    Unread,
    /// A place saved, not finished.
    InProgress,
    /// Finished (`books.finished_at` set).
    Finished,
}

impl ReadState {
    /// The state the stored item is in: its finish time and whether a place is saved.
    pub fn stored(finished: bool, has_progress: bool) -> Self {
        match (finished, has_progress) {
            (true, _) => Self::Finished,
            (false, true) => Self::InProgress,
            (false, false) => Self::Unread,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::InProgress => "inProgress",
            Self::Finished => "finished",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadEvent {
    /// A place was saved, by reading or listening; `at_end` when it is the item's last
    /// segment.
    Progressed {
        at_end: bool,
    },
    MarkedFinished,
    MarkedUnread,
}

pub fn next_read(state: ReadState, event: ReadEvent) -> ReadState {
    use ReadEvent::*;
    use ReadState::*;
    match (state, event) {
        (_, MarkedUnread) => Unread,
        (_, MarkedFinished) | (_, Progressed { at_end: true }) => Finished,
        (Finished, Progressed { at_end: false }) => Finished,
        (_, Progressed { at_end: false }) => InProgress,
    }
}

/// Time read since the previous report is credited up to this much: a longer gap means the
/// reader stepped away.
const CAP: TimeDelta = TimeDelta::seconds(30);

/// When each item last reported reading in this run.
#[derive(Default)]
pub struct ReadingClock {
    last: HashMap<String, NaiveDateTime>,
}

impl ReadingClock {
    /// Item `id` is being read at local time `now`: the time since its previous report (at most
    /// [`CAP`]), split by local day as `(YYYY-MM-DD, ms)`. The first report credits nothing.
    pub fn credit(&mut self, id: &str, now: NaiveDateTime) -> Vec<(String, u64)> {
        let Some(previous) = self.last.insert(id.to_owned(), now) else {
            return Vec::new();
        };
        let start = now - (now - previous).clamp(TimeDelta::zero(), CAP);
        let mut parts = Vec::new();
        let mut from = start;
        while from < now {
            let midnight = (from.date() + TimeDelta::days(1)).and_time(NaiveTime::MIN);
            let to = midnight.min(now);
            parts.push((
                from.date().format("%Y-%m-%d").to_string(),
                (to - from).num_milliseconds() as u64,
            ));
            from = to;
        }
        parts
    }
}

/// Read state and reading time, kept in the library.
pub(super) struct Reading {
    library: Arc<Mutex<Library>>,
    reporter: Reporter,
    clock: Mutex<ReadingClock>,
}

impl Reading {
    pub fn new(library: Arc<Mutex<Library>>, reporter: Reporter) -> Self {
        Self {
            library,
            reporter,
            clock: Mutex::default(),
        }
    }

    /// Item `id`'s read state as stored.
    pub fn state(&self, id: &str) -> Result<ReadState, String> {
        let library = lock(&self.library);
        let book = BookId(id.into());
        let record = library
            .book(&book)
            .map_err(|e| e.to_string())?
            .ok_or("no such book")?;
        let progress = library.progress(&book).map_err(|e| e.to_string())?;
        Ok(ReadState::stored(
            record.finished_at.is_some(),
            progress.is_some(),
        ))
    }

    /// Applies `event` to item `id` and stores the state it leads to; returns the state before
    /// and after.
    pub fn apply(&self, id: &str, event: ReadEvent) -> Result<(ReadState, ReadState), String> {
        let from = self.state(id)?;
        let to = next_read(from, event);
        let book = BookId(id.into());
        let library = lock(&self.library);
        let stored = match (from, to) {
            (_, ReadState::Unread) => library
                .set_finished_at(&book, None)
                .and_then(|()| library.delete_progress(&book)),
            (ReadState::Finished, ReadState::Finished) => Ok(()),
            (_, ReadState::Finished) => library.set_finished_at(&book, Some(super::now_ms())),
            (_, ReadState::InProgress) => Ok(()),
        };
        stored.map_err(|e| e.to_string())?;
        if from != to {
            tracing::info!(
                event = "reading.state",
                book = crate::logging::book(id),
                from = from.name(),
                to = to.name(),
                "Read state"
            );
        }
        Ok((from, to))
    }

    /// Item `id` is being read now: credits the time since its last report to today.
    pub fn activity(&self, id: &str, now: NaiveDateTime) {
        let parts = lock(&self.clock).credit(id, now);
        let book = BookId(id.into());
        for (day, ms) in parts.into_iter().filter(|(_, ms)| *ms > 0) {
            if let Err(error) = lock(&self.library).record_reading(&book, &day, ms) {
                self.reporter
                    .report("reading.save", "Could not save your reading time", error);
            }
        }
    }
}

impl ReadingMode {
    fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Listen => "listen",
        }
    }
}

impl super::Core {
    /// What item `id` plays with, for its default mode.
    pub(super) fn audio_facts(&self, id: &str) -> AudioFacts {
        let book = BookId(id.into());
        let (cached_speech, listened) = {
            let library = lock(&self.library);
            (
                library.book_audio_bytes(&book).unwrap_or(0) > 0,
                library
                    .book_listening(&book)
                    .is_ok_and(|days| days.iter().any(|d| d.ms > 0)),
            )
        };
        AudioFacts {
            narrates_recording: self.plays_recording(id, &self.book_settings(id)),
            cached_speech,
            prepared: self.prepare_view(id).is_some(),
            listened,
        }
    }

    /// Item `id`'s mode: the user's choice, else its default.
    pub(super) fn mode_of(&self, id: &str) -> ReadingMode {
        mode_for(self.book_settings(id).mode, &self.audio_facts(id))
    }

    /// Applies a mode choice to item `id` and stores it when it changed.
    pub(super) fn choose_mode(&self, id: &str, event: ModeEvent) -> Result<ReadingMode, String> {
        let chosen = self.book_settings(id).mode;
        let from = self.mode_of(id);
        let to = next_mode(from, event);
        if chosen != Some(to) {
            self.shelf.save_mode(id, to)?;
            if from != to {
                tracing::info!(
                    event = "reading.mode",
                    book = crate::logging::book(id),
                    from = from.name(),
                    to = to.name(),
                    "Mode"
                );
            }
            self.emit(CoreEvent::LibraryChanged);
        }
        Ok(to)
    }

    /// The user chose to read item `id` silently or to listen to it. Reading the item that is
    /// playing pauses it; choosing to listen does not start playback by itself.
    pub fn set_mode(&self, id: &str, mode: ReadingMode) -> Result<BookView, String> {
        if mode == ReadingMode::Read {
            self.require(id, super::item::Needs::Text)?;
        }
        let event = match mode {
            ReadingMode::Read => ModeEvent::ReadChosen,
            ReadingMode::Listen => ModeEvent::ListenChosen,
        };
        if event == ModeEvent::ReadChosen && self.playback.is_book(id) && self.playback.is_playing()
        {
            self.pause();
        }
        self.choose_mode(id, event)?;
        self.book_overview(id)
    }

    /// Applies `event` to item `id`'s read state; tells the window when the state changed.
    pub(super) fn read_event(&self, id: &str, event: ReadEvent) -> Result<ReadState, String> {
        let (from, to) = self.reading.apply(id, event)?;
        if from != to {
            self.emit(CoreEvent::LibraryChanged);
        }
        Ok(to)
    }

    pub fn mark_finished(&self, id: &str) -> Result<(), String> {
        self.read_event(id, ReadEvent::MarkedFinished).map(drop)
    }

    /// Forgets that item `id` was finished, and where the user was in it.
    pub fn mark_unread(&self, id: &str) -> Result<(), String> {
        self.read_event(id, ReadEvent::MarkedUnread).map(drop)
    }

    /// The user is reading item `id` silently at `segment_id` in `chapter`: stores that place
    /// as its progress, as playback would, and returns it. Ignored (`None`) while the item is
    /// playing, since playback owns the place then.
    pub fn set_reading_place(
        &self,
        id: &str,
        chapter: usize,
        segment_id: &str,
    ) -> Result<Option<super::BookProgressView>, String> {
        if self.playback.is_book(id) && self.playback.is_playing() {
            return Ok(None);
        }
        let book = self.load_book(id)?;
        let index = book
            .index_of(segment_id)
            .filter(|&index| book.locate(index).is_some_and(|(c, _)| c == chapter))
            .ok_or("that place is not in the book")?;
        self.store_place(super::playback::Place::Entry { book, index })?;
        Ok(self.shelf.progress(&BookId(id.into())))
    }

    /// The user is reading item `id` silently now; the window says so every few seconds.
    pub fn reading_activity(&self, id: &str) {
        self.reading
            .activity(id, chrono::Local::now().naive_local());
    }

    /// Reading and listening time, per day and per item.
    pub fn activity_stats(&self) -> Result<aoide_library::ActivityStats, String> {
        lock(&self.library)
            .activity_stats()
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_follows_the_last_choice() {
        for mode in [ReadingMode::Read, ReadingMode::Listen] {
            assert_eq!(
                next_mode(mode, ModeEvent::ListenChosen),
                ReadingMode::Listen
            );
            assert_eq!(next_mode(mode, ModeEvent::ReadChosen), ReadingMode::Read);
        }
    }

    #[test]
    fn an_item_used_for_listening_opens_for_listening() {
        assert_eq!(default_mode(false, false, false), ReadingMode::Read);
        for facts in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            assert_eq!(
                default_mode(facts.0, facts.1, facts.2),
                ReadingMode::Listen,
                "{facts:?}"
            );
        }
    }

    #[test]
    fn shape_is_the_mode_for_text_and_listening_without_it() {
        assert_eq!(shape(true, ReadingMode::Read), Shape::Read);
        assert_eq!(shape(true, ReadingMode::Listen), Shape::ReadAlong);
        assert_eq!(shape(false, ReadingMode::Read), Shape::Listen);
        assert_eq!(shape(false, ReadingMode::Listen), Shape::Listen);
    }

    #[test]
    fn read_state_transitions() {
        use ReadEvent::*;
        use ReadState::*;
        let table = [
            (Unread, Progressed { at_end: false }, InProgress),
            (Unread, Progressed { at_end: true }, Finished),
            (InProgress, Progressed { at_end: false }, InProgress),
            (InProgress, Progressed { at_end: true }, Finished),
            (Finished, Progressed { at_end: false }, Finished),
            (Finished, Progressed { at_end: true }, Finished),
            (Unread, MarkedFinished, Finished),
            (InProgress, MarkedFinished, Finished),
            (Finished, MarkedFinished, Finished),
            (Unread, MarkedUnread, Unread),
            (InProgress, MarkedUnread, Unread),
            (Finished, MarkedUnread, Unread),
        ];
        for (from, event, to) in table {
            assert_eq!(next_read(from, event), to, "{from:?} + {event:?}");
        }
    }

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn reading_time_is_credited_since_the_last_report_up_to_a_cap() {
        let mut clock = ReadingClock::default();
        assert_eq!(clock.credit("a", at("2026-11-15 10:00:00")), []);
        assert_eq!(
            clock.credit("a", at("2026-11-15 10:00:10")),
            [("2026-11-15".to_owned(), 10_000)]
        );
        assert_eq!(
            clock.credit("a", at("2026-11-15 10:05:10")),
            [("2026-11-15".to_owned(), 30_000)]
        );
        // Another item has its own first report.
        assert_eq!(clock.credit("b", at("2026-11-15 10:05:20")), []);
    }

    #[test]
    fn reading_across_midnight_is_split_between_the_days() {
        let mut clock = ReadingClock::default();
        clock.credit("a", at("2026-11-15 23:59:50"));
        assert_eq!(
            clock.credit("a", at("2026-11-16 00:00:10")),
            [
                ("2026-11-15".to_owned(), 10_000),
                ("2026-11-16".to_owned(), 10_000)
            ]
        );
    }
}
