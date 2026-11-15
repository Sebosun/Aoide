//! Playback: the book being listened to (its [`Session`]) and the player that plays it. The
//! session and player locks are only taken here, the session's always first. Neither is held
//! while audio is read or decoded. Made speech comes through [`SpeechAudio`]: from the library
//! in the app, from memory in tests.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_audio::{codec, Pcm, PlayerCore, Position, Status};
use aoide_library::{BookId, BookKey, Library, Progress, WordTiming};
use aoide_text::WordEstimate;

use super::align::Alignable;
use super::narration::NarrationPlan;
use super::recording::{Narration, RecordingPlan};
use super::session::Session;
use super::timeline::{self, Timeline, TimelineSession};
use super::{lock, PlaybackStatus, PlayerStateView, Target};
use crate::book::BookModel;

/// A recording being listened to while it syncs takes up the sync's progress at least this
/// often, even when playback is not waiting for it.
const FOLLOW_EVERY: Duration = Duration::from_secs(60);

/// Made speech, decoded.
pub struct MadeSpeech {
    pub pcm: Pcm,
    /// Word timings stored with it, if any.
    pub timings: Option<Vec<WordTiming>>,
}

/// Where made speech is read from.
pub trait SpeechAudio {
    /// The speech with cache key `key`; `None` when it is not there (any more).
    fn read(&self, key: &str) -> Result<Option<MadeSpeech>, String>;
}

/// What one tick of playback did.
pub struct Tick {
    /// The playhead is on another entry than at the last tick (or this session's first tick).
    pub moved: bool,
    /// What was just given to the player without exact word times.
    pub to_align: Vec<Alignable>,
}

/// The playhead, for saving the listener's place. Returned by each change that should be
/// saved at once, for the caller to save.
pub enum Place {
    /// An entry of a book (a segment of its text).
    Entry { book: Arc<BookModel>, index: usize },
    /// A time on an audiobook's timeline, in `part`.
    Time {
        id: String,
        part: usize,
        position_ms: u64,
        total_ms: u64,
    },
}

impl Place {
    pub fn id(&self) -> &str {
        match self {
            Place::Entry { book, .. } => &book.id,
            Place::Time { id, .. } => id,
        }
    }

    /// Whether the place is the item's end, which finishes it.
    pub fn at_end(&self) -> bool {
        match self {
            Place::Entry { book, index } => index + 1 >= book.total_entries(),
            Place::Time {
                position_ms,
                total_ms,
                ..
            } => *position_ms + timeline::AT_END_MS >= *total_ms,
        }
    }

    /// Saves this place as the item's progress.
    pub fn save(self, library: &Mutex<Library>) -> Result<(), String> {
        let (book, index) = match self {
            Place::Entry { book, index } => (book, index),
            Place::Time {
                id,
                part,
                position_ms,
                total_ms,
            } => {
                let id = BookId(id);
                let library = lock(library);
                let fraction = if total_ms == 0 {
                    0.0
                } else {
                    (position_ms as f64 / total_ms as f64).clamp(0.0, 1.0)
                };
                return library
                    .save_progress(
                        &id,
                        &Progress {
                            chapter: part as u32,
                            char_offset: 0,
                            segment_id: None,
                            updated_at: super::now_ms(),
                            position_ms: Some(position_ms),
                        },
                    )
                    .and_then(|()| {
                        library.set_book_value(&id, BookKey::ProgressFraction, &fraction)
                    })
                    .map_err(|e| e.to_string());
            }
        };
        let Some((chapter, local)) = book.locate(index) else {
            return Ok(());
        };
        let segment = &book.chapters[chapter].segments[local];
        let id = BookId(book.id.clone());
        let saved = {
            let library = lock(library);
            library
                .save_progress(
                    &id,
                    &Progress {
                        chapter: chapter as u32,
                        char_offset: segment.range.start as u32,
                        segment_id: Some(segment.id.clone()),
                        updated_at: super::now_ms(),
                        position_ms: None,
                    },
                )
                .and_then(|()| {
                    library.set_book_value(&id, BookKey::ProgressFraction, &book.fraction(index))
                })
        };
        saved.map_err(|e| e.to_string())
    }
}

/// What is being listened to.
enum Playing {
    /// A book's entries (its segments), read by a speech engine or its recording.
    Entries(Session),
    /// An audiobook with no text, straight through on its timeline.
    Timeline(TimelineSession),
}

pub struct Playback {
    /// Shared with the audio output, which renders from it.
    player: Arc<Mutex<PlayerCore>>,
    playing: Mutex<Option<Playing>>,
    error: Mutex<Option<String>>,
    /// The audio output's latency: the word shown is the word heard.
    latency_ms: Mutex<u32>,
}

impl Playback {
    pub fn new(player: PlayerCore) -> Self {
        Self {
            player: Arc::new(Mutex::new(player)),
            playing: Mutex::new(None),
            error: Mutex::new(None),
            latency_ms: Mutex::new(0),
        }
    }

    /// The player, for the audio output to render from.
    pub fn player(&self) -> &Arc<Mutex<PlayerCore>> {
        &self.player
    }

    pub fn set_latency(&self, latency_ms: u32) {
        *lock(&self.latency_ms) = latency_ms;
    }

    /// Runs `f` on the session when a book's entries are playing.
    fn session<R>(&self, f: impl FnOnce(&mut Session) -> R) -> Option<R> {
        match lock(&self.playing).as_mut()? {
            Playing::Entries(session) => Some(f(session)),
            Playing::Timeline(_) => None,
        }
    }

    // ------------------------------------------------------------ what is playing

    /// The book whose entries are playing (none for an audiobook on its timeline).
    pub fn book(&self) -> Option<Arc<BookModel>> {
        self.session(|s| Arc::clone(&s.target.book))
    }

    /// The audiobook playing on its timeline.
    pub fn timeline(&self) -> Option<Arc<Timeline>> {
        match lock(&self.playing).as_ref()? {
            Playing::Timeline(t) => Some(Arc::clone(&t.timeline)),
            Playing::Entries(_) => None,
        }
    }

    /// The item playing, of either kind.
    pub fn playing_id(&self) -> Option<String> {
        match lock(&self.playing).as_ref()? {
            Playing::Entries(s) => Some(s.target.book.id.clone()),
            Playing::Timeline(t) => Some(t.timeline.id.clone()),
        }
    }

    pub fn is_book(&self, id: &str) -> bool {
        self.playing_id().is_some_and(|playing| playing == id)
    }

    /// The recording of book `id`, when it is being listened to.
    pub fn recording_of(&self, id: &str) -> Option<Arc<RecordingPlan>> {
        self.session(|s| {
            (s.target.book.id == id)
                .then(|| s.recording().cloned())
                .flatten()
        })
        .flatten()
    }

    /// Runs `f` on the narration of the book being listened to, with the book's id.
    pub fn with_narration<R>(&self, f: impl FnOnce(&str, &NarrationPlan) -> R) -> Option<R> {
        self.session(|s| f(&s.target.book.id, &s.target.narration))
    }

    /// Entries in `range` of book `id` that its recording, being listened to, has no audio for.
    pub fn unrecorded(&self, id: &str, range: Range<usize>) -> Option<Vec<usize>> {
        self.session(|s| {
            (s.target.book.id == id && s.recording().is_some())
                .then(|| s.target.narration.unrecorded(range))
        })
        .flatten()
    }

    /// The chapter (an audiobook's part) the playhead is in.
    pub fn chapter(&self) -> Option<usize> {
        let position = self.position();
        match lock(&self.playing).as_ref()? {
            Playing::Entries(s) => s.target.book.locate(position.index).map(|(c, _)| c),
            Playing::Timeline(t) => {
                let timeline = &t.timeline;
                Some(timeline.part_at(timeline.time_at(position.index, position.offset_ms)))
            }
        }
    }

    // ------------------------------------------------------------ transport

    pub fn position(&self) -> Position {
        lock(&self.player).position()
    }

    pub fn is_playing(&self) -> bool {
        matches!(self.position().status, Status::Playing | Status::Buffering)
    }

    pub fn play(&self) {
        lock(&self.player).play();
    }

    #[must_use = "the place is lost unless it is saved"]
    pub fn pause(&self) -> Option<Place> {
        lock(&self.player).pause();
        self.place_to_save(true)
    }

    pub fn speed(&self) -> f32 {
        lock(&self.player).speed()
    }

    pub fn volume(&self) -> f32 {
        lock(&self.player).volume()
    }

    pub fn set_speed(&self, speed: f32) {
        lock(&self.player).set_speed(speed);
    }

    pub fn set_volume(&self, volume: f32) {
        lock(&self.player).set_volume(volume);
    }

    /// The audio output failed: playback pauses with `error` shown.
    pub fn fail(&self, error: String) {
        lock(&self.player).pause();
        *lock(&self.error) = Some(error);
    }

    /// Plays `target` from entry `start`, in place of anything playing.
    pub fn start(&self, target: Target, start: usize) {
        let entries = target.narration.player_entries(&target.book);
        let offset = target
            .narration
            .recording_plan()
            .map_or(0, |plan| plan.start_offset(start));
        let rate = self.load(entries, start, offset);
        *lock(&self.playing) = Some(Playing::Entries(Session::new(target, rate)));
        *lock(&self.error) = None;
    }

    /// Plays `timeline` from `at_ms`, in place of anything playing.
    pub fn start_timeline(&self, timeline: Arc<Timeline>, at_ms: u64) {
        let (start, offset) = timeline.locate(at_ms);
        let rate = self.load(timeline.entries(), start, offset);
        *lock(&self.playing) = Some(Playing::Timeline(TimelineSession::new(timeline, rate)));
        *lock(&self.error) = None;
    }

    /// Gives the player `entries` and plays from `offset` ms into entry `start`; returns the
    /// rate it plays at.
    fn load(&self, entries: Vec<aoide_audio::Entry>, start: usize, offset: u32) -> u32 {
        let mut player = lock(&self.player);
        player.load(entries, start);
        player.seek_to(start, offset);
        player.play();
        player.output_rate()
    }

    /// Ends the session; returns where it was.
    #[must_use = "the place is lost unless it is saved"]
    pub fn stop(&self) -> Option<Place> {
        let place = self.place_to_save(true);
        lock(&self.player).load(Vec::new(), 0);
        *lock(&self.playing) = None;
        place
    }

    /// Moves the playhead `offset_ms` into entry `index`; `None` starts where the entry's text
    /// does (for a recording, just before its first word; on a timeline, the stretch's start).
    #[must_use = "the place is lost unless it is saved"]
    pub fn seek(&self, index: usize, offset_ms: Option<u32>) -> Option<Place> {
        let (offset, released) = match lock(&self.playing).as_mut() {
            Some(Playing::Entries(session)) => {
                let offset = offset_ms.unwrap_or_else(|| {
                    session
                        .recording()
                        .map_or(0, |plan| plan.start_offset(index))
                });
                (offset, session.restart_feed())
            }
            Some(Playing::Timeline(session)) => (offset_ms.unwrap_or(0), session.restart_feed()),
            None => (0, Vec::new()),
        };
        {
            let mut player = lock(&self.player);
            for entry in released {
                player.release(entry);
            }
            player.seek_to(index, offset);
        }
        self.place_to_save(true)
    }

    /// Moves the playhead to `at_ms` on the timeline of book `id`'s recording.
    #[must_use = "the place is lost unless it is saved"]
    pub fn seek_recording(&self, id: &str, at_ms: u64) -> Result<Option<Place>, String> {
        let plan = self
            .recording_of(id)
            .ok_or("the recording is not playing")?;
        let index = plan
            .clips
            .iter()
            .position(|c| c.audio.contains(&at_ms))
            .ok_or("that time is not in the recording")?;
        let offset = (at_ms - plan.clips[index].audio.start) as u32;
        Ok(self.seek(index, Some(offset)))
    }

    /// The playhead on the timeline of audiobook `id`, when it is playing.
    pub fn time_of(&self, id: &str) -> Option<u64> {
        let timeline = self.timeline().filter(|t| t.id == id)?;
        let position = self.position();
        Some(timeline.time_at(position.index, position.offset_ms))
    }

    /// Moves the playhead to `at_ms` on audiobook `id`'s timeline.
    #[must_use = "the place is lost unless it is saved"]
    pub fn seek_time(&self, id: &str, at_ms: u64) -> Result<Option<Place>, String> {
        let timeline = self
            .timeline()
            .filter(|t| t.id == id)
            .ok_or("the audiobook is not playing")?;
        let (index, offset) = timeline.locate(at_ms);
        Ok(self.seek(index, Some(offset)))
    }

    // ------------------------------------------------------------ audio

    /// Gives the player the audio near the playhead it does not have yet (the recording's
    /// clips as they are decoded, made speech read from `audio`), and lets it release audio far
    /// behind. `None` when nothing is playing.
    pub fn tick(&self, audio: &dyn SpeechAudio) -> Option<Tick> {
        let playhead = self.position().index;
        let (moved, pieces, wanted, mut to_align) = {
            let mut playing = lock(&self.playing);
            match playing.as_mut()? {
                Playing::Entries(session) => {
                    let moved = session.ticked.replace(playhead) != Some(playhead);
                    let pieces = session.feed(playhead);
                    let book = session.target.book.id.clone();
                    let clips: Vec<Alignable> = match session.recording().cloned() {
                        Some(plan) => session
                            .take_to_align()
                            .into_iter()
                            .map(|index| Alignable::Clip {
                                book: book.clone(),
                                index,
                                plan: Arc::clone(&plan),
                            })
                            .collect(),
                        None => Vec::new(),
                    };
                    (moved, pieces, session.wanted_speech(playhead), clips)
                }
                Playing::Timeline(session) => {
                    let moved = session.ticked.replace(playhead) != Some(playhead);
                    (moved, session.feed(playhead), Vec::new(), Vec::new())
                }
            }
        };
        if !pieces.is_empty() {
            let mut player = lock(&self.player);
            let rate = player.output_rate();
            for piece in pieces {
                player.append(
                    piece.index,
                    &Pcm {
                        sample_rate: rate,
                        samples: piece.samples,
                    },
                );
                if piece.last {
                    player.finish(piece.index);
                }
            }
        }
        for (index, key) in wanted {
            let MadeSpeech { pcm, timings } = match audio.read(&key) {
                Ok(Some(read)) => read,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!(event = "playback.cache", %key, %error, "Cannot read cached audio");
                    continue;
                }
            };
            lock(&self.player).provide(index, &pcm);
            let align = self.session(|session| {
                session
                    .provided(index, timings.as_deref(), &pcm)
                    .then(|| session.target.book.id.clone())
            });
            if let Some(Some(book)) = align {
                to_align.push(Alignable::Speech { book, index, key });
            }
        }
        let release = match lock(&self.playing).as_mut()? {
            Playing::Entries(session) => session.release_behind(playhead),
            Playing::Timeline(session) => session.release_behind(playhead),
        };
        if let Some(before) = release {
            lock(&self.player).release_before(before);
        }
        Some(Tick { moved, to_align })
    }

    #[cfg(test)]
    pub fn timings_of(&self, index: usize) -> Option<Vec<WordEstimate>> {
        self.session(|s| s.timings_of(index)).flatten()
    }

    /// Exact word times arrived for entry `index` of book `book`.
    pub fn aligned(&self, book: &str, index: usize, timings: Vec<WordEstimate>) {
        self.session(|session| {
            if session.target.book.id == book {
                session.aligned(index, timings);
            }
        });
    }

    /// The speech with cache key `key` is made.
    pub fn segment_made(&self, key: &str, duration_ms: u32) {
        self.session(|session| session.segment_made(key, duration_ms));
    }

    /// The speech with these cache keys was deleted.
    pub fn audio_lost(&self, keys: &HashSet<String>) {
        self.session(|session| session.audio_lost(keys));
    }

    // ------------------------------------------------------------ following a sync

    /// Book `id`'s recording, being listened to while it syncs, should take up the sync's
    /// progress: playback waits for it, it has not for a while, or `force`.
    pub fn should_follow(&self, id: &str, force: bool) -> Option<Arc<BookModel>> {
        let playhead = self.position().index;
        self.session(|session| {
            (session.target.book.id == id
                && session.recording().is_some()
                && (force
                    || session.target.narration.waits_for_sync(playhead)
                    || session.followed.elapsed() >= FOLLOW_EVERY))
                .then(|| Arc::clone(&session.target.book))
        })
        .flatten()
    }

    /// Book `id` plays by `target`, its recording synced further, from now on. Audio the player
    /// has stays; what turned out not to be in the recording is passed over. Returns whether the
    /// speech engine has other entries to say now; `None` when the book is not playing.
    pub fn follow(&self, id: &str, target: Target) -> Option<bool> {
        let followed = self
            .session(|session| (session.target.book.id == id).then(|| session.follow(target)))
            .flatten()?;
        let mut player = lock(&self.player);
        for index in followed.passed_over {
            player.finish(index);
        }
        Some(followed.speech_changed)
    }

    // ------------------------------------------------------------ views

    /// The playhead, when `force`, it was last saved a while ago, or it is at the item's end
    /// (which finishes it).
    #[must_use = "the place is lost unless it is saved"]
    pub fn place_to_save(&self, force: bool) -> Option<Place> {
        let position = self.position();
        let mut playing = lock(&self.playing);
        let (place, last_saved) = match playing.as_mut()? {
            Playing::Entries(session) => {
                let book = Arc::clone(&session.target.book);
                let last = book.total_entries().saturating_sub(1);
                let index = position.index.min(last);
                (Place::Entry { book, index }, &mut session.last_saved)
            }
            Playing::Timeline(session) => {
                let timeline = &session.timeline;
                let position_ms = timeline.time_at(position.index, position.offset_ms);
                let place = Place::Time {
                    id: timeline.id.clone(),
                    part: timeline.part_at(position_ms),
                    position_ms,
                    total_ms: timeline.total_ms,
                };
                (place, &mut session.last_saved)
            }
        };
        if !force && !place.at_end() && last_saved.elapsed() < Duration::from_secs(3) {
            return None;
        }
        *last_saved = Instant::now();
        Some(place)
    }

    /// The player's state (without the sleep timer, which is not playback's).
    pub fn state(&self) -> PlayerStateView {
        let (position, speed, volume) = {
            let player = lock(&self.player);
            (player.position(), player.speed(), player.volume())
        };
        let error = lock(&self.error).clone();
        let status = match position.status {
            Status::Stopped | Status::Finished => PlaybackStatus::Stopped,
            Status::Paused => PlaybackStatus::Paused,
            Status::Playing => PlaybackStatus::Playing,
            Status::Buffering => PlaybackStatus::Buffering,
        };
        let playing = lock(&self.playing);
        let session = match playing.as_ref() {
            None => {
                return PlayerStateView {
                    book_id: None,
                    status: PlaybackStatus::Stopped,
                    chapter: 0,
                    segment_id: None,
                    word_index: None,
                    speed,
                    volume,
                    chapter_fraction: 0.0,
                    error,
                    sleep: None,
                    source: Narration::Speech,
                    off_text: false,
                    chapter_left_ms: None,
                    waiting_for_sync: false,
                    position_ms: None,
                    duration_ms: None,
                    part_position_ms: None,
                }
            }
            Some(Playing::Timeline(session)) => {
                let timeline = &session.timeline;
                let at = timeline.time_at(position.index, position.offset_ms);
                let part_index = timeline.part_at(at);
                let part = &timeline.parts[part_index];
                let into = at.saturating_sub(part.start_ms);
                let length = part.end_ms.saturating_sub(part.start_ms);
                let left = part.end_ms.saturating_sub(at);
                return PlayerStateView {
                    book_id: Some(timeline.id.clone()),
                    status,
                    chapter: part_index,
                    segment_id: None,
                    word_index: None,
                    speed,
                    volume,
                    chapter_fraction: if length == 0 {
                        0.0
                    } else {
                        into as f64 / length as f64
                    },
                    error,
                    sleep: None,
                    source: Narration::Recording,
                    off_text: false,
                    chapter_left_ms: Some((left as f64 / f64::from(speed.max(0.1))) as u64),
                    waiting_for_sync: false,
                    position_ms: Some(at),
                    duration_ms: Some(timeline.total_ms),
                    part_position_ms: Some(into),
                };
            }
            Some(Playing::Entries(session)) => session,
        };
        let book = &session.target.book;
        let narration = &session.target.narration;
        let index = position.index.min(book.total_entries().saturating_sub(1));
        let (chapter, _) = book.locate(index).unwrap_or((0, 0));
        let latency = (*lock(&self.latency_ms) as f32 * speed) as u32;
        let heard_at = position.offset_ms.saturating_sub(latency);
        let recording = session.recording();
        let off_text = recording.is_some_and(|plan| plan.off_text(index, heard_at));
        let word = (position.status == Status::Playing && !off_text)
            .then(|| session.word_at(index, heard_at))
            .flatten();
        PlayerStateView {
            book_id: Some(book.id.clone()),
            status,
            chapter,
            segment_id: book.segment(index).map(|s| s.id.clone()),
            word_index: word,
            speed,
            volume,
            chapter_fraction: book.chapter_fraction(index),
            error,
            sleep: None,
            source: if recording.is_some() {
                Narration::Recording
            } else {
                Narration::Speech
            },
            off_text,
            chapter_left_ms: recording.and_then(|_| {
                let entries = book
                    .chapters
                    .get(chapter)
                    .map_or(index..index, |c| index..c.first_entry + c.segments.len());
                let audio = narration.playing_ms(entries)?;
                let left = audio.saturating_sub(u64::from(position.offset_ms));
                Some((left as f64 / f64::from(speed.max(0.1))) as u64)
            }),
            waiting_for_sync: narration.waits_for_sync(index),
            position_ms: None,
            duration_ms: None,
            part_position_ms: None,
        }
    }
}

/// Made speech, read from the library's audio files.
pub struct LibraryAudio<'a> {
    pub library: &'a Mutex<Library>,
    pub audio_dir: &'a std::path::Path,
}

impl SpeechAudio for LibraryAudio<'_> {
    fn read(&self, key: &str) -> Result<Option<MadeSpeech>, String> {
        let segment = lock(self.library).segment(key).map_err(|e| e.to_string())?;
        let Some(segment) = segment else {
            return Ok(None);
        };
        let bytes = std::fs::read(self.audio_dir.join(&segment.file)).map_err(|e| e.to_string())?;
        Ok(Some(MadeSpeech {
            pcm: codec::decode(&bytes).map_err(|e| e.to_string())?,
            timings: segment.word_timings,
        }))
    }
}

impl super::Core {
    /// Saves the listener's place; the one way it is saved.
    pub(super) fn save_place(&self, place: Option<Place>) {
        let Some(place) = place else { return };
        if let Err(error) = self.store_place(place) {
            self.report("progress.save", "Could not save your place", error);
        }
    }

    /// Stores `place` as its book's progress and moves the book's read state on; the one way a
    /// place is stored, by listening or by reading.
    pub(super) fn store_place(&self, place: Place) -> Result<(), String> {
        let id = place.id().to_owned();
        let at_end = place.at_end();
        place.save(&self.library)?;
        self.read_event(&id, super::reading::ReadEvent::Progressed { at_end })
            .map(drop)
    }

    pub(super) fn seek(&self, index: usize) {
        self.seek_at(index, None);
    }

    /// Moves the playhead `offset_ms` into entry `index`; `None` starts where the entry's text
    /// does (for a recording, just before its first word).
    pub(super) fn seek_at(&self, index: usize, offset_ms: Option<u32>) {
        self.save_place(self.playback.seek(index, offset_ms));
        self.plan(super::PlanEvent::ListeningChanged { jump: true });
    }

    pub fn pause(&self) {
        self.save_place(self.playback.pause());
    }

    pub fn resume(&self) -> Result<(), String> {
        self.playback.play();
        self.ensure_output()
    }

    pub fn stop(&self) {
        self.save_place(self.playback.stop());
        self.output.close();
        self.plan(super::PlanEvent::ListeningStopped);
    }

    pub fn toggle(&self) {
        if self.playback.is_playing() {
            self.pause();
        } else {
            let _ = self.resume();
        }
    }

    /// Plays from the start of the chapter `delta` chapters away from the current one.
    pub async fn skip_chapter(&self, delta: i64) -> Result<(), String> {
        if let Some(timeline) = self.playback.timeline() {
            return self.skip_part(&timeline, delta);
        }
        let book = self.playback.book().ok_or("nothing is playing")?;
        let (chapter, _) = book
            .locate(self.playback.position().index)
            .unwrap_or((0, 0));
        let last = book.chapters.len().saturating_sub(1) as i64;
        let target = (chapter as i64 + delta).clamp(0, last) as usize;
        self.play(&book.id, Some(target), None).await
    }

    pub fn set_speed(&self, speed: f32) {
        self.playback.set_speed(speed);
        self.save_player_settings();
    }

    pub fn set_volume(&self, volume: f32) {
        self.playback.set_volume(volume);
        self.save_player_settings();
    }

    pub(super) fn save_player_settings(&self) {
        let settings = super::config::PlayerSettings {
            speed: self.playback.speed(),
            volume: self.playback.volume(),
        };
        let saved = self.settings.save(super::config::PLAYER, &settings);
        if let Err(error) = saved {
            self.report("config.save", "Could not save the player settings", error);
        }
    }

    pub(crate) fn player_state(&self) -> super::PlayerStateView {
        super::PlayerStateView {
            sleep: self.controls.sleep_view(),
            ..self.playback.state()
        }
    }

    /// Case-insensitive search of the book's text; at most 200 hits in reading order.
    pub fn search_book(
        &self,
        id: &str,
        query: &str,
    ) -> Result<Vec<crate::book::SearchHit>, String> {
        Ok(self.load_book(id)?.search(query, 200))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use aoide_generation::PlanItem;
    use aoide_tts::SynthesisRequest;

    use super::*;
    use crate::core::preparation::Plan;

    const RATE: u32 = 1_000;

    /// Made speech kept in memory: one second of sound per key.
    #[derive(Default)]
    struct Memory {
        made: HashMap<String, u32>,
    }

    impl SpeechAudio for Memory {
        fn read(&self, key: &str) -> Result<Option<MadeSpeech>, String> {
            Ok(self.made.get(key).map(|ms| MadeSpeech {
                pcm: Pcm {
                    sample_rate: RATE,
                    samples: vec![0.5; (*ms * RATE / 1_000) as usize],
                },
                timings: None,
            }))
        }
    }

    /// The test book (four entries) read by the speech engine, with entries `made` made.
    fn speech(made: &[usize]) -> Target {
        let book = Arc::new(BookModel::parse("b", &crate::core::tests::epub(), false).unwrap());
        let requests: Vec<Option<SynthesisRequest>> = (0..book.total_entries())
            .map(|i| {
                Some(SynthesisRequest {
                    text: format!("entry {i}"),
                    voice_id: "v".into(),
                    language: None,
                    params: Default::default(),
                })
            })
            .collect();
        let items = (0..book.total_entries())
            .map(|i| PlanItem {
                key: format!("k{i}"),
                speakable: true,
                cached: made.contains(&i),
                duration_ms: 1_000,
            })
            .collect();
        Target {
            narration: NarrationPlan::speech(&book, Plan { items, requests }),
            book,
        }
    }

    fn memory(keys: &[&str]) -> Memory {
        Memory {
            made: keys.iter().map(|k| (k.to_string(), 1_000)).collect(),
        }
    }

    /// Plays `ms` of audio.
    fn listen(playback: &Playback, ms: usize) {
        let mut out = vec![0.0; ms * RATE as usize / 1_000];
        lock(playback.player()).render(&mut out);
    }

    #[test]
    fn made_speech_reaches_the_player_and_the_playhead_moving_is_told_once() {
        let playback = Playback::new(PlayerCore::new(RATE));
        playback.start(speech(&[0, 1]), 0);
        let audio = memory(&["k0", "k1"]);
        let first = playback.tick(&audio).unwrap();
        assert!(first.moved, "a session's first tick");
        assert_eq!(playback.position().index, 0);
        assert_eq!(first.to_align.len(), 2, "no stored word timings");
        assert!(!playback.tick(&audio).unwrap().moved);
        assert_eq!(playback.state().status, PlaybackStatus::Playing);

        // Through entry 0 and its pause, into entry 1.
        let pause = playback.book().unwrap().segment(0).unwrap().pause_after_ms as usize;
        listen(&playback, 1_000 + pause + 100);
        let tick = playback.tick(&audio).unwrap();
        assert_eq!((playback.position().index, tick.moved), (1, true));
        assert!(tick.to_align.is_empty(), "given once");
        assert!(!playback.tick(&audio).unwrap().moved);
    }

    #[test]
    fn speech_made_later_is_given_on_the_next_tick() {
        let playback = Playback::new(PlayerCore::new(RATE));
        playback.start(speech(&[]), 0);
        playback.tick(&memory(&[])).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Buffering);
        playback.segment_made("k0", 1_000);
        playback.tick(&memory(&["k0"])).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Playing);
    }

    #[test]
    fn deleted_speech_is_not_given_until_made_again() {
        let playback = Playback::new(PlayerCore::new(RATE));
        playback.start(speech(&[0]), 0);
        playback.audio_lost(&HashSet::from(["k0".to_string()]));
        playback.tick(&memory(&["k0"])).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Buffering);
    }

    #[test]
    fn nothing_plays_after_stopping() {
        let playback = Playback::new(PlayerCore::new(RATE));
        playback.start(speech(&[0]), 0);
        assert!(playback.is_book("b") && playback.book().is_some());
        let Some(Place::Entry { book, index }) = playback.stop() else {
            panic!("where it stopped")
        };
        assert_eq!((book.id.as_str(), index), ("b", 0));
        assert!(playback.tick(&memory(&["k0"])).is_none());
        let state = playback.state();
        assert_eq!(
            (state.book_id, state.status),
            (None, PlaybackStatus::Stopped)
        );
    }

    #[test]
    fn playback_waiting_for_the_sync_passes_over_what_it_finds_missing() {
        let book = Arc::new(BookModel::parse("b", &crate::core::tests::epub(), false).unwrap());
        let clip = |audio: std::ops::Range<u64>| aoide_sync::Clip {
            text: (!audio.is_empty()).then(|| audio.clone()),
            audio,
            words: vec![],
            off_text: vec![],
        };
        let clips = vec![
            clip(0..1_000),
            clip(1_000..1_000),
            clip(1_000..2_000),
            clip(2_000..3_000),
        ];
        let recording = |synced: usize| Target {
            narration: NarrationPlan::recording(
                &book,
                Arc::new(RecordingPlan {
                    files: vec![],
                    clips: clips.clone(),
                }),
                synced,
                || None,
            ),
            book: Arc::clone(&book),
        };
        let playback = Playback::new(PlayerCore::new(RATE));
        playback.start(recording(1), 1);
        let state = playback.state();
        assert!(state.waiting_for_sync);
        assert_eq!(state.status, PlaybackStatus::Buffering);
        assert!(playback.should_follow("b", false).is_some(), "it waits");
        assert!(playback.should_follow("other", true).is_none());

        assert_eq!(playback.follow("b", recording(4)), Some(false));
        assert!(
            playback.should_follow("b", false).is_none(),
            "followed just now"
        );
        listen(&playback, 10);
        let state = playback.state();
        assert!(!state.waiting_for_sync);
        assert_eq!(
            playback.position().index,
            2,
            "entry 1 is not in the recording"
        );
    }

    #[test]
    fn a_seek_plays_from_the_entry_sought() {
        let playback = Playback::new(PlayerCore::new(RATE));
        playback.start(speech(&[0, 1, 2, 3]), 0);
        playback.tick(&memory(&["k0", "k1", "k2", "k3"])).unwrap();
        let Some(Place::Entry { index, .. }) = playback.seek(2, None) else {
            panic!("the place sought")
        };
        assert_eq!(index, 2);
        let tick = playback.tick(&memory(&["k0", "k1", "k2", "k3"])).unwrap();
        assert_eq!((playback.position().index, tick.moved), (2, true));
        assert_eq!(playback.state().segment_id, Some("1:0:0".into()));
    }
}
