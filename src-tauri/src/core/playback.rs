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
use aoide_tts::SynthesisRequest;

use super::align::Alignable;
use super::narration::{Fate, NarrationPlan};
use super::recording::{Narration, RecordingPlan};
use super::resuming::{self, Away};
use super::session::Session;
use super::timeline::{self, Timeline, TimelineSession};
use super::{lock, PlaybackStatus, PlayerStateView, SpeedScope, Target};
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

/// What a move of the playhead asks of the rest of the app. Every move of the playhead
/// returns one; [`super::Core::follow_through`] does it, the one place that does, in this
/// order: save the place, tell Generation, then open or close the audio output.
#[must_use = "a move of the playhead is followed through with Core::follow_through"]
#[derive(Default)]
pub struct Moved {
    /// The place to save now.
    pub place: Option<Place>,
    /// What to tell Generation about what is listened to.
    pub listening: Option<Listening>,
    /// What the audio output should do.
    pub output: Option<OutputNeed>,
}

/// A change to what is listened to, for Generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Listening {
    /// Something else is listened to, or the playhead jumped in a book's entries: the speech
    /// to make starts from the playhead at once.
    Jumped,
    /// The playhead moved on, or the narration changed under it: the speech to make follows
    /// once the work under way is done.
    Carried,
    /// Nothing is listened to any more.
    Stopped,
}

/// What the audio output should do after a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputNeed {
    /// Something plays: the output must be running.
    Open,
    /// Nothing plays any more.
    Close,
}

impl Moved {
    #[cfg(test)]
    pub fn parts(&self) -> (bool, Option<Listening>, Option<OutputNeed>) {
        (self.place.is_some(), self.listening, self.output)
    }
}

/// What is being listened to.
enum Playing {
    /// A book's entries (its segments), read by a speech engine or its recording.
    Entries(Session),
    /// An audiobook with no text, straight through on its timeline.
    Timeline(TimelineSession),
}

/// The speeds playback chooses from: the usual one (`[player].speed`) and the playing book's
/// own, when it has one.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Speeds {
    usual: f32,
    own: Option<f32>,
}

/// Where a change of speed went (see [`Playback::set_speed`]).
#[derive(Debug, Clone, PartialEq)]
pub enum SpeedChange {
    /// The usual speed, kept in `[player]`.
    Usual,
    /// Item `id`'s own speed, now `speed`.
    Own { id: String, speed: f32 },
}

/// A playback speed as the core keeps it: 0.5–3.0, to two decimals (so steps of 0.1 do not
/// drift to 1.4000001); 1.0 for a value that is not a number.
pub fn kept_speed(speed: f32) -> f32 {
    if !speed.is_finite() {
        return 1.0;
    }
    (speed.clamp(0.5, 3.0) * 100.0).round() / 100.0
}

pub struct Playback {
    /// Shared with the audio output, which renders from it.
    player: Arc<Mutex<PlayerCore>>,
    /// Locked before `player` when both are.
    speeds: Mutex<Speeds>,
    playing: Mutex<Option<Playing>>,
    error: Mutex<Option<String>>,
    /// The audio output's latency: the word shown is the word heard.
    latency_ms: Mutex<u32>,
    /// Told of each change to what plays or how, for the tick to show it.
    changes: tokio::sync::Notify,
    /// When playback was paused, while it stays paused where it was: how long the listener was
    /// away when it plays again.
    paused_at: Mutex<Option<Instant>>,
}

impl Playback {
    /// Playback by `player`, whose speed is the usual one.
    pub fn new(player: PlayerCore) -> Self {
        let usual = kept_speed(player.speed());
        Self {
            player: Arc::new(Mutex::new(player)),
            speeds: Mutex::new(Speeds { usual, own: None }),
            playing: Mutex::new(None),
            error: Mutex::new(None),
            latency_ms: Mutex::new(0),
            changes: tokio::sync::Notify::new(),
            paused_at: Mutex::new(None),
        }
    }

    /// Resolves at the next change to what plays or how (at once, when one came since the last
    /// call): a session started or stopped, play, pause, a seek, the speed or volume, a failure.
    pub async fn changed(&self) {
        self.changes.notified().await;
    }

    /// Something shown with playback changed.
    pub fn stir(&self) {
        self.changes.notify_one();
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

    /// Plays (or carries on playing) what is loaded.
    pub fn play(&self) -> Moved {
        *lock(&self.paused_at) = None;
        lock(&self.player).play();
        self.stir();
        Moved {
            output: Some(OutputNeed::Open),
            ..Moved::default()
        }
    }

    /// Plays on after a pause, `now`: after a break, a little earlier (see
    /// [`resuming::step_back`]).
    pub fn resume_at(&self, now: Instant) -> Moved {
        let paused_at = lock(&self.paused_at).take();
        let stepped = paused_at
            .and_then(|at| self.step_back(Away(now.saturating_duration_since(at)), "pause"));
        let played = self.play();
        match stepped {
            Some(moved) => Moved {
                output: played.output,
                ..moved
            },
            None => played,
        }
    }

    /// Moves the playhead back for a listener `away` (see [`resuming`]), and says so; `None`
    /// when it stays. `from` is what `away` was measured from, for the log.
    fn step_back(&self, away: Away, from: &'static str) -> Option<Moved> {
        let position = self.position();
        let (index, offset) = match lock(&self.playing).as_ref()? {
            Playing::Entries(session) => (
                step_back_in(&session.target, position.index, away, from)?,
                None,
            ),
            Playing::Timeline(session) => {
                let timeline = &session.timeline;
                let at = timeline.time_at(position.index, position.offset_ms);
                let (index, offset) =
                    timeline.locate(timeline::step_back_on(timeline, at, away, from)?);
                (index, Some(offset))
            }
        };
        Some(self.seek(index, offset))
    }

    /// Pauses; the place is saved.
    pub fn pause(&self) -> Moved {
        self.pause_since(None)
    }

    /// When playback was paused, while it stays paused where it was.
    pub fn paused_since(&self) -> Option<Instant> {
        *lock(&self.paused_at)
    }

    /// Pauses as [`Playback::pause`] does, the pause counted from `since` (when `None`, from
    /// the pause already on, or now): a session started again in place of one paused stays
    /// paused since then, and pausing what is paused keeps the time it was first paused.
    pub fn pause_since(&self, since: Option<Instant>) -> Moved {
        {
            let mut paused_at = lock(&self.paused_at);
            *paused_at = since.or(*paused_at).or_else(|| Some(Instant::now()));
        }
        lock(&self.player).pause();
        self.stir();
        Moved {
            place: self.place_to_save(true),
            ..Moved::default()
        }
    }

    pub fn volume(&self) -> f32 {
        lock(&self.player).volume()
    }

    /// Plays at `speed`, which becomes the playing item's own speed when it has one, else the
    /// usual speed; says which, for it to be saved there.
    pub fn set_speed(&self, speed: f32) -> SpeedChange {
        let speed = kept_speed(speed);
        let playing = self.playing_id();
        let mut speeds = lock(&self.speeds);
        let change = match (speeds.own.as_mut(), playing) {
            (Some(own), Some(id)) => {
                *own = speed;
                SpeedChange::Own { id, speed }
            }
            _ => {
                speeds.usual = speed;
                SpeedChange::Usual
            }
        };
        lock(&self.player).set_speed(speed);
        drop(speeds);
        self.stir();
        change
    }

    /// The speed the player plays at.
    pub fn speed(&self) -> f32 {
        lock(&self.player).speed()
    }

    /// The usual speed, which items without their own play at.
    pub fn usual_speed(&self) -> f32 {
        lock(&self.speeds).usual
    }

    /// Makes `speed` the usual one; the player takes it unless what plays has its own.
    pub fn set_usual_speed(&self, speed: f32) {
        let mut speeds = lock(&self.speeds);
        speeds.usual = kept_speed(speed);
        if speeds.own.is_none() {
            lock(&self.player).set_speed(speeds.usual);
        }
        drop(speeds);
        self.stir();
    }

    /// Plays at `own`, the speed of the item about to play, or at the usual speed when it has
    /// none of its own.
    pub fn use_own_speed(&self, own: Option<f32>) {
        let mut speeds = lock(&self.speeds);
        speeds.own = own.map(kept_speed);
        lock(&self.player).set_speed(speeds.own.unwrap_or(speeds.usual));
        drop(speeds);
        self.stir();
    }

    pub fn set_volume(&self, volume: f32) {
        lock(&self.player).set_volume(volume);
        self.stir();
    }

    /// The audio output failed: playback pauses with `error` shown.
    pub fn fail(&self, error: String) {
        lock(&self.player).pause();
        *lock(&self.error) = Some(error);
        self.stir();
    }

    /// Plays `target` from entry `start`, in place of anything playing.
    pub fn start(&self, target: Target, start: usize) -> Moved {
        let entries = target.narration.player_entries(&target.book);
        let offset = target
            .narration
            .recording_plan()
            .map_or(0, |plan| plan.start_offset(start));
        let rate = self.load(entries, start, offset);
        *lock(&self.paused_at) = None;
        *lock(&self.playing) = Some(Playing::Entries(Session::new(target, rate)));
        *lock(&self.error) = None;
        self.stir();
        Self::started()
    }

    /// What follows a start: something else is listened to, and it plays.
    fn started() -> Moved {
        Moved {
            place: None,
            listening: Some(Listening::Jumped),
            output: Some(OutputNeed::Open),
        }
    }

    /// Plays `timeline` from `at_ms`, in place of anything playing.
    pub fn start_timeline(&self, timeline: Arc<Timeline>, at_ms: u64) -> Moved {
        let (start, offset) = timeline.locate(at_ms);
        let rate = self.load(timeline.entries(), start, offset);
        *lock(&self.paused_at) = None;
        *lock(&self.playing) = Some(Playing::Timeline(TimelineSession::new(timeline, rate)));
        *lock(&self.error) = None;
        self.stir();
        Self::started()
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

    /// Ends the session; its place is saved.
    pub fn stop(&self) -> Moved {
        let place = self.place_to_save(true);
        lock(&self.player).load(Vec::new(), 0);
        // With nothing playing, the usual speed applies.
        self.use_own_speed(None);
        *lock(&self.paused_at) = None;
        *lock(&self.playing) = None;
        self.stir();
        Moved {
            place,
            listening: Some(Listening::Stopped),
            output: Some(OutputNeed::Close),
        }
    }

    /// Moves the playhead `offset_ms` into entry `index`; `None` starts where the entry's text
    /// does (for a recording, just before its first word; on a timeline, the stretch's start).
    /// The place is saved; a jump in a book's entries is a jump for Generation too, while the
    /// speech it makes does not depend on where an audiobook's timeline is.
    pub fn seek(&self, index: usize, offset_ms: Option<u32>) -> Moved {
        // A place chosen while paused is played from as chosen.
        *lock(&self.paused_at) = None;
        let (offset, released, listening) = match lock(&self.playing).as_mut() {
            Some(Playing::Entries(session)) => {
                // Sent into a chapter left out, it is played.
                let admitted = super::leaving_out::admit(&mut session.target, index);
                if !admitted.is_empty() {
                    let entries = session
                        .target
                        .narration
                        .player_entries(&session.target.book);
                    let mut player = lock(&self.player);
                    for index in admitted {
                        player.update(index, entries[index].clone());
                    }
                }
                let offset = offset_ms.unwrap_or_else(|| {
                    session
                        .recording()
                        .map_or(0, |plan| plan.start_offset(index))
                });
                (offset, session.restart_feed(), Some(Listening::Jumped))
            }
            Some(Playing::Timeline(session)) => {
                (offset_ms.unwrap_or(0), session.restart_feed(), None)
            }
            None => (0, Vec::new(), None),
        };
        {
            let mut player = lock(&self.player);
            for entry in released {
                player.release(entry);
            }
            player.seek_to(index, offset);
        }
        self.stir();
        Moved {
            place: self.place_to_save(true),
            listening,
            output: None,
        }
    }

    /// Moves the playhead to `at_ms` on the timeline of book `id`'s recording.
    pub fn seek_recording(&self, id: &str, at_ms: u64) -> Result<Moved, String> {
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
    pub fn seek_time(&self, id: &str, at_ms: u64) -> Result<Moved, String> {
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
                    let before = session.ticked.replace(playhead);
                    let moved = before != Some(playhead);
                    if let Some(before) = before {
                        log_skipped_left_out(&session.target, before, playhead);
                    }
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

    /// Entry `index` of book `id` is said again by `request` under `key`: the player lets go of
    /// its audio and waits for the new take. Returns false when that book is not playing or the
    /// speech engine does not say the entry.
    pub fn retake(&self, id: &str, index: usize, request: SynthesisRequest, key: String) -> bool {
        let retaken = self
            .session(|session| session.target.book.id == id && session.retake(index, request, key))
            .unwrap_or(false);
        if retaken {
            lock(&self.player).release(index);
        }
        retaken
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
    /// has stays; what turned out not to be in the recording is passed over. Generation is
    /// told when the speech engine has other entries to say now. Nothing follows when the book
    /// is not playing.
    pub fn follow(&self, id: &str, target: Target) -> Moved {
        let Some(followed) = self
            .session(|session| (session.target.book.id == id).then(|| session.follow(target)))
            .flatten()
        else {
            return Moved::default();
        };
        let mut player = lock(&self.player);
        for index in followed.passed_over {
            player.finish(index);
        }
        Moved {
            listening: followed.speech_changed.then_some(Listening::Carried),
            ..Moved::default()
        }
    }

    /// One tick of playback: gives the player audio near the playhead (see [`Playback::tick`]).
    /// When the playhead reached another entry, Generation follows it and the place is saved
    /// every few seconds. `aligned` gets what was given to the player without exact word times.
    pub fn advance(&self, audio: &dyn SpeechAudio, aligned: impl FnOnce(Vec<Alignable>)) -> Moved {
        let Some(tick) = self.tick(audio) else {
            return Moved::default();
        };
        aligned(tick.to_align);
        if !tick.moved {
            return Moved::default();
        }
        Moved {
            listening: Some(Listening::Carried),
            place: self.place_to_save(false),
            output: None,
        }
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
        let speed_scope = if lock(&self.speeds).own.is_some() {
            SpeedScope::Book
        } else {
            SpeedScope::Usual
        };
        let stopped = PlayerStateView {
            book_id: None,
            status: PlaybackStatus::Stopped,
            chapter: 0,
            segment_id: None,
            word_index: None,
            speed,
            speed_scope,
            volume,
            chapter_fraction: 0.0,
            error: lock(&self.error).clone(),
            sleep: None,
            source: Narration::Speech,
            off_text: false,
            chapter_left_ms: None,
            book_left_ms: None,
            chapter_ms: None,
            waiting_for_sync: false,
            position_ms: None,
            duration_ms: None,
            part_position_ms: None,
        };
        let playing = lock(&self.playing);
        match playing.as_ref() {
            None => stopped,
            Some(Playing::Timeline(session)) => timeline_state(session, &position, stopped),
            Some(Playing::Entries(session)) => self.entries_state(session, &position, stopped),
        }
    }

    /// Where a book's entries are being narrated (by speech or a synced recording).
    fn entries_state(
        &self,
        session: &Session,
        position: &Position,
        base: PlayerStateView,
    ) -> PlayerStateView {
        let speed = base.speed;
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
        let chapter_entries = book.chapters.get(chapter).map_or(index..index, |c| {
            c.first_entry..c.first_entry + c.segments.len()
        });
        let left = |end: usize| {
            let audio = narration.playing_ms(index..end)?;
            let left = audio.saturating_sub(u64::from(position.offset_ms));
            Some(seconds(at_speed(left, speed)))
        };
        let chapter_left_ms = left(chapter_entries.end);
        let book_left_ms = left(book.total_entries());
        let chapter_ms = narration.playing_ms(chapter_entries);
        PlayerStateView {
            book_id: Some(book.id.clone()),
            status: status_of(position.status),
            chapter,
            segment_id: book.segment(index).map(|s| s.id.clone()),
            word_index: word,
            chapter_fraction: book.chapter_fraction(index),
            source: if recording.is_some() {
                Narration::Recording
            } else {
                Narration::Speech
            },
            off_text,
            chapter_left_ms,
            book_left_ms,
            chapter_ms,
            waiting_for_sync: narration.waits_for_sync(index),
            ..base
        }
    }
}

/// The entry `target` plays from, stepping back from entry `index` for a listener `away` (see
/// [`resuming`]): never before the chapter's start, counting only entries said or recorded. A
/// step back is logged with `from`, what `away` was measured from. `None` plays on.
pub(super) fn step_back_in(
    target: &Target,
    index: usize,
    away: Away,
    from: &'static str,
) -> Option<usize> {
    let book = &target.book;
    let (chapter, _) = book.locate(index)?;
    let narration = &target.narration;
    let spoken = |i| matches!(narration.fate(i), Some(Fate::Speech { .. } | Fate::Clip));
    let first = book.chapters[chapter].first_entry;
    let back = resuming::entry_back(index, resuming::step_back(away), first, spoken)?;
    tracing::info!(
        event = "playback.resumed",
        book = crate::logging::book(&book.id),
        away_s = away.0.as_secs(),
        stepped_back = index - back,
        unit = "entries",
        from,
        "Stepped back on resume"
    );
    Some(back)
}

/// Where an audiobook with no text is: a time in its recording.
fn timeline_state(
    session: &TimelineSession,
    position: &Position,
    base: PlayerStateView,
) -> PlayerStateView {
    let timeline = &session.timeline;
    let at = timeline.time_at(position.index, position.offset_ms);
    let part_index = timeline.part_at(at);
    let part = &timeline.parts[part_index];
    let into = at.saturating_sub(part.start_ms);
    let length = part.end_ms.saturating_sub(part.start_ms);
    PlayerStateView {
        book_id: Some(timeline.id.clone()),
        status: status_of(position.status),
        chapter: part_index,
        chapter_fraction: if length == 0 {
            0.0
        } else {
            into as f64 / length as f64
        },
        source: Narration::Recording,
        chapter_left_ms: Some(seconds(at_speed(
            part.end_ms.saturating_sub(at),
            base.speed,
        ))),
        book_left_ms: Some(seconds(at_speed(
            timeline.total_ms.saturating_sub(at),
            base.speed,
        ))),
        chapter_ms: Some(length),
        position_ms: Some(at),
        duration_ms: Some(timeline.total_ms),
        part_position_ms: Some(into),
        ..base
    }
}

/// Logs playback passing over chapters left out on its way from entry `from` to `to`.
fn log_skipped_left_out(target: &Target, from: usize, to: usize) {
    if !target.narration.passed_left_out(from, to) {
        return;
    }
    let chapter = |index: usize| {
        target
            .book
            .locate(index)
            .map_or(target.book.chapters.len(), |(c, _)| c)
    };
    tracing::info!(
        event = "playback.skipped_left_out",
        book = crate::logging::book(&target.book.id),
        from = chapter(from),
        to = chapter(to),
        "Passed over chapters left out of the narration"
    );
}

fn status_of(status: Status) -> PlaybackStatus {
    match status {
        Status::Stopped | Status::Finished => PlaybackStatus::Stopped,
        Status::Paused => PlaybackStatus::Paused,
        Status::Playing => PlaybackStatus::Playing,
        Status::Buffering => PlaybackStatus::Buffering,
    }
}

/// How long `ms` of audio takes to play at `speed`.
fn at_speed(ms: u64, speed: f32) -> u64 {
    (ms as f64 / f64::from(speed.max(0.1))) as u64
}

/// `ms` in whole seconds, rounded down: a time left that falls by the millisecond would change
/// the player's state, and so send it, on every tick.
fn seconds(ms: u64) -> u64 {
    ms / 1_000 * 1_000
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
        // Read or played before its sync reached the audiobook's place: that place is moot.
        if matches!(place, Place::Entry { .. }) {
            self.drop_pending_place(&id, "dropped");
        }
        place.save(&self.library)?;
        self.read_event(&id, super::reading::ReadEvent::Progressed { at_end })
            .map(drop)
    }

    /// Does what a move of the playhead asks for (see [`Moved`]): saves the place, tells
    /// Generation, then opens or closes the audio output. The one place that does; an `Err`
    /// says the output could not be opened.
    pub(super) fn follow_through(&self, moved: Moved) -> Result<(), String> {
        let Moved {
            place,
            listening,
            output,
        } = moved;
        self.save_place(place);
        match listening {
            Some(Listening::Jumped) => self.plan(super::PlanEvent::ListeningChanged { jump: true }),
            Some(Listening::Carried) => {
                self.plan(super::PlanEvent::ListeningChanged { jump: false });
            }
            Some(Listening::Stopped) => self.plan(super::PlanEvent::ListeningStopped),
            None => {}
        }
        match output {
            Some(OutputNeed::Open) => self.ensure_output(),
            Some(OutputNeed::Close) => {
                self.output.close();
                Ok(())
            }
            None => Ok(()),
        }
    }

    pub fn pause(&self) {
        // A pause leaves the output as it is: nothing in it can fail.
        let _ = self.follow_through(self.playback.pause());
    }

    /// Plays on after a pause, a little earlier after a break unless that is turned off.
    pub fn resume(&self) -> Result<(), String> {
        let moved = if self.steps_back() {
            self.playback.resume_at(Instant::now())
        } else {
            self.playback.play()
        };
        self.follow_through(moved)
    }

    /// Whether playback steps back a little after a break (a setting).
    pub(super) fn steps_back(&self) -> bool {
        self.app_settings().step_back_on_resume
    }

    pub fn stop(&self) {
        // Closing the output cannot fail.
        let _ = self.follow_through(self.playback.stop());
    }

    pub fn toggle(&self) {
        if self.playback.is_playing() {
            self.pause();
        } else {
            let _ = self.resume();
        }
    }

    /// Plays from the start of the chapter `delta` chapters away from the current one,
    /// counting only those narrated.
    pub async fn skip_chapter(&self, delta: i64) -> Result<(), String> {
        if let Some(timeline) = self.playback.timeline() {
            return self.skip_part(&timeline, delta);
        }
        let book = self.playback.book().ok_or("nothing is playing")?;
        let (chapter, _) = book
            .locate(self.playback.position().index)
            .unwrap_or((0, 0));
        let narrated = |c: usize| {
            let first = book.chapters[c].first_entry;
            !self
                .playback
                .with_narration(|_, n| n.is_left_out(first))
                .unwrap_or(false)
        };
        let target = super::leaving_out::skip(chapter, delta, book.chapters.len(), narrated);
        self.play(&book.id, Some(target), None).await
    }

    /// Plays at `speed`, saved as the playing book's own speed when it has one, else as the
    /// usual speed.
    pub fn set_speed(&self, speed: f32) {
        match self.playback.set_speed(speed) {
            SpeedChange::Usual => self.save_player_settings(),
            SpeedChange::Own { id, speed } => {
                if let Err(error) = self.shelf.set_playback_speed(&id, Some(speed)) {
                    self.report("library.save", "Could not save the book's speed", error);
                }
            }
        }
    }

    pub fn set_volume(&self, volume: f32) {
        self.playback.set_volume(volume);
        self.save_player_settings();
    }

    /// Gives book `id` its own speed (`on`), the speed it plays at when it is playing, else
    /// the usual one; or takes it away, and the book plays at the usual speed.
    pub fn set_book_speed(&self, id: &str, on: bool) -> Result<(), String> {
        let playing = self.playback.is_book(id);
        let speed = on.then(|| {
            if playing {
                self.playback.speed()
            } else {
                self.playback.usual_speed()
            }
        });
        self.shelf.set_playback_speed(id, speed)?;
        tracing::info!(
            event = "playback.book_speed",
            book = crate::logging::book(id),
            on,
            "A book's own speed turned on or off"
        );
        if playing {
            self.playback.use_own_speed(speed);
        }
        Ok(())
    }

    pub(super) fn save_player_settings(&self) {
        let settings = super::config::PlayerSettings {
            // A book's own speed is the book's, not the usual one.
            speed: self.playback.usual_speed(),
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
        speech_of(&crate::core::tests::epub(), made, &[])
    }

    /// The book in `epub` read by the speech engine, with entries `made` made and entries
    /// `silent` not said.
    fn speech_of(epub: &[u8], made: &[usize], silent: &[usize]) -> Target {
        let book = Arc::new(BookModel::parse("b", epub, &Default::default()).unwrap());
        let requests: Vec<Option<SynthesisRequest>> = (0..book.total_entries())
            .map(|i| {
                (!silent.contains(&i)).then(|| SynthesisRequest {
                    text: format!("entry {i}"),
                    voice_id: "v".into(),
                    language: None,
                    params: Default::default(),
                    take: 0,
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
        let _ = playback.start(speech(&[0, 1]), 0);
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
        let _ = playback.start(speech(&[]), 0);
        playback.tick(&memory(&[])).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Buffering);
        playback.segment_made("k0", 1_000);
        playback.tick(&memory(&["k0"])).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Playing);
    }

    #[test]
    fn the_time_left_in_a_book_read_by_speech_counts_made_and_estimated_speech_at_the_speed() {
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(speech(&[1]), 1);
        playback.segment_made("k2", 400);
        playback.tick(&memory(&["k1", "k2"])).unwrap();
        listen(&playback, 300);
        let offset = u64::from(playback.position().offset_ms);
        assert!(offset > 0, "into entry 1");
        playback.set_speed(1.5);

        // Entry 1 made (1 s), entry 2 made shorter (0.4 s), entry 3 estimated (1 s).
        let pause = |i| u64::from(playback.book().unwrap().segment(i).unwrap().pause_after_ms);
        let rest = 1_000 + pause(1) + 400 + pause(2) + 1_000 + pause(3) - offset;
        let state = playback.state();
        assert_eq!(state.book_left_ms, Some(whole_seconds(rest as f64 / 1.5)));
        // The playhead is in the first chapter's second (and last) entry.
        let chapter_rest = 1_000 + pause(1) - offset;
        assert_eq!(
            state.chapter_left_ms,
            Some(whole_seconds(chapter_rest as f64 / 1.5))
        );
        assert_eq!(
            state.chapter_ms,
            Some(1_000 + pause(0) + 1_000 + pause(1)),
            "the whole chapter at 1x"
        );
    }

    /// `ms` in whole seconds, rounded down, as the player's state gives a time left.
    fn whole_seconds(ms: f64) -> u64 {
        ms as u64 / 1_000 * 1_000
    }

    #[test]
    fn the_time_left_falls_by_whole_seconds_so_the_state_changes_once_a_second() {
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(speech(&[0, 1, 2, 3]), 0);
        let audio = memory(&["k0", "k1", "k2", "k3"]);
        playback.tick(&audio).unwrap();
        let before = playback.state();
        listen(&playback, 20);
        playback.tick(&audio).unwrap();
        listen(&playback, 20);
        playback.tick(&audio).unwrap();
        let after = playback.state();
        assert!(after.book_left_ms.unwrap() % 1_000 == 0);
        assert!(after.chapter_left_ms.unwrap() % 1_000 == 0);
        assert_eq!(before, after, "nothing to send within the second");
    }

    #[test]
    fn deleted_speech_is_not_given_until_made_again() {
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(speech(&[0]), 0);
        playback.audio_lost(&HashSet::from(["k0".to_string()]));
        playback.tick(&memory(&["k0"])).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Buffering);
    }

    #[test]
    fn a_sentence_said_again_plays_its_new_take_now_and_ahead() {
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(speech(&[0, 1, 2, 3]), 0);
        let old = memory(&["k0", "k1", "k2", "k3"]);
        playback.tick(&old).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Playing);
        let take = |index: usize| SynthesisRequest {
            text: format!("entry {index}"),
            voice_id: "v".into(),
            language: None,
            params: Default::default(),
            take: 1,
        };

        // The sentence playing now, and one the player holds ahead.
        assert!(playback.retake("b", 0, take(0), "k0-take".into()));
        assert!(playback.retake("b", 2, take(2), "k2-take".into()));
        assert!(!playback.retake("other", 1, take(1), "x".into()));
        playback.tick(&old).unwrap();
        assert_eq!(
            playback.state().status,
            PlaybackStatus::Buffering,
            "the old take is let go and not given again"
        );

        // The new takes are shorter than the old ones: the playhead shows which plays.
        let mut new = memory(&["k1", "k3"]);
        new.made.insert("k0-take".into(), 400);
        new.made.insert("k2-take".into(), 300);
        playback.segment_made("k0-take", 400);
        playback.segment_made("k2-take", 300);
        playback.tick(&new).unwrap();
        assert_eq!(playback.state().status, PlaybackStatus::Playing);
        let pause = |i| playback.book().unwrap().segment(i).unwrap().pause_after_ms as usize;
        listen(&playback, 400 + pause(0) + 50);
        playback.tick(&new).unwrap();
        assert_eq!(playback.position().index, 1);
        listen(&playback, 950 + pause(1) + 300 + pause(2) + 50);
        playback.tick(&new).unwrap();
        assert_eq!(
            playback.position().index,
            3,
            "entry 2 played its 300 ms take"
        );
    }

    /// Whether a change was told within a moment.
    async fn told(playback: &Playback) -> bool {
        tokio::time::timeout(Duration::from_millis(20), playback.changed())
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn each_change_to_playback_is_told_to_whoever_waits_for_it() {
        let playback = Playback::new(PlayerCore::new(RATE));
        assert!(!told(&playback).await, "nothing changed yet");
        let _ = playback.start(speech(&[0]), 0);
        assert!(told(&playback).await);
        assert!(!told(&playback).await, "told once");
        let _ = playback.pause();
        assert!(told(&playback).await);
        playback.set_speed(1.5);
        assert!(told(&playback).await);
        let _ = playback.seek(0, Some(10));
        assert!(told(&playback).await);
        let _ = playback.stop();
        assert!(told(&playback).await);
    }

    #[test]
    fn nothing_plays_after_stopping() {
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(speech(&[0]), 0);
        assert!(playback.is_book("b") && playback.book().is_some());
        let Some(Place::Entry { book, index }) = playback.stop().place else {
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

    /// The test book read from its recording (entry 1 not in it), synced up to entry `synced`.
    fn recorded(synced: usize) -> Target {
        let book = Arc::new(
            BookModel::parse("b", &crate::core::tests::epub(), &Default::default()).unwrap(),
        );
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
        Target {
            narration: NarrationPlan::recording(
                &book,
                Arc::new(RecordingPlan {
                    files: vec![],
                    clips,
                }),
                synced,
                || None,
            ),
            book,
        }
    }

    #[test]
    fn the_time_left_in_a_book_read_from_its_recording_is_known_once_the_sync_reaches_the_end() {
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(recorded(3), 0);
        assert_eq!(
            playback.state().book_left_ms,
            None,
            "entry 3 awaits the sync"
        );

        let _ = playback.follow("b", recorded(4));
        playback.set_speed(2.0);
        // Three clips of a second (entry 1 is not in the recording), their pauses in them.
        assert_eq!(
            playback.state().book_left_ms,
            Some(1_000),
            "1.5 s, in whole seconds"
        );
    }

    #[test]
    fn the_time_left_in_an_audiobook_alone_runs_to_the_end_of_its_recording() {
        let timeline = Timeline::new(
            "a".into(),
            "A".into(),
            vec![],
            vec![],
            65_000,
            vec![("One".into(), 0), ("Two".into(), 40_000)],
        );
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start_timeline(Arc::new(timeline), 30_000);
        playback.set_speed(1.25);
        let state = playback.state();
        assert_eq!(state.position_ms, Some(30_000));
        assert_eq!(state.chapter_left_ms, Some(8_000));
        assert_eq!(state.book_left_ms, Some(28_000));
    }

    #[test]
    fn playback_waiting_for_the_sync_passes_over_what_it_finds_missing() {
        let recording = recorded;
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start(recording(1), 1);
        let state = playback.state();
        assert!(state.waiting_for_sync);
        assert_eq!(state.status, PlaybackStatus::Buffering);
        assert!(playback.should_follow("b", false).is_some(), "it waits");
        assert!(playback.should_follow("other", true).is_none());

        assert_eq!(
            playback.follow("b", recording(4)).listening,
            None,
            "the speech engine has nothing new to say"
        );
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
        let _ = playback.start(speech(&[0, 1, 2, 3]), 0);
        playback.tick(&memory(&["k0", "k1", "k2", "k3"])).unwrap();
        let Some(Place::Entry { index, .. }) = playback.seek(2, None).place else {
            panic!("the place sought")
        };
        assert_eq!(index, 2);
        let tick = playback.tick(&memory(&["k0", "k1", "k2", "k3"])).unwrap();
        assert_eq!((playback.position().index, tick.moved), (2, true));
        assert_eq!(playback.state().segment_id, Some("1:0:0".into()));
    }

    #[test]
    fn each_move_of_the_playhead_says_what_follows_it() {
        use Listening::*;
        use OutputNeed::*;
        let playback = Playback::new(PlayerCore::new(RATE));
        assert_eq!(
            playback.start(speech(&[0, 1, 2, 3]), 0).parts(),
            (false, Some(Jumped), Some(Open)),
            "something else is listened to, and plays"
        );
        assert_eq!(
            playback.seek(2, None).parts(),
            (true, Some(Jumped), None),
            "a jump in a book's entries"
        );
        assert_eq!(playback.pause().parts(), (true, None, None));
        assert_eq!(playback.play().parts(), (false, None, Some(Open)));
        assert_eq!(
            playback.stop().parts(),
            (true, Some(Stopped), Some(Close)),
            "nothing is listened to"
        );
        assert_eq!(playback.stop().parts(), (false, Some(Stopped), Some(Close)));
    }

    /// Plays the test book read by speech, all made, into entry `index`, and pauses there.
    fn paused_in(index: usize) -> Playback {
        let playback = Playback::new(PlayerCore::new(RATE));
        drop(playback.start(speech(&[0, 1, 2, 3]), index));
        playback.tick(&memory(&["k0", "k1", "k2", "k3"])).unwrap();
        listen(&playback, 500);
        drop(playback.pause());
        assert_eq!(playback.position().index, index);
        playback
    }

    #[test]
    fn resuming_after_ten_minutes_starts_a_sentence_earlier_and_saves_that_place() {
        let playback = paused_in(1);
        let back = Instant::now() + Duration::from_secs(10 * 60);
        let moved = playback.resume_at(back);
        let Some(Place::Entry { index, .. }) = moved.place else {
            panic!("the place stepped back to is saved")
        };
        assert_eq!(index, 0);
        assert_eq!(
            (moved.listening, moved.output),
            (Some(Listening::Jumped), Some(OutputNeed::Open))
        );
        assert_eq!(playback.position().index, 0);
        assert_eq!(playback.position().offset_ms, 0);
        assert!(playback.is_playing());
    }

    #[test]
    fn resuming_after_a_moment_plays_on_from_the_playhead() {
        let playback = paused_in(1);
        let offset = playback.position().offset_ms;
        assert!(offset > 0);
        assert_eq!(
            playback.resume_at(Instant::now()).parts(),
            (false, None, Some(OutputNeed::Open))
        );
        assert_eq!(
            (playback.position().index, playback.position().offset_ms),
            (1, offset)
        );
    }

    #[test]
    fn resuming_after_a_minute_starts_the_sentence_again() {
        let playback = paused_in(1);
        drop(playback.resume_at(Instant::now() + Duration::from_secs(60)));
        assert_eq!(
            (playback.position().index, playback.position().offset_ms),
            (1, 0)
        );
    }

    #[test]
    fn resuming_never_steps_back_over_a_chapter_start() {
        let two_hours = Duration::from_secs(2 * 60 * 60);
        let first = paused_in(2);
        drop(first.resume_at(Instant::now() + two_hours));
        assert_eq!(first.position().index, 2, "the chapter's first sentence");
        let second = paused_in(3);
        drop(second.resume_at(Instant::now() + two_hours));
        assert_eq!(second.position().index, 2, "one sentence back at most");
    }

    #[test]
    fn a_place_chosen_while_paused_is_played_from_as_chosen() {
        let playback = paused_in(1);
        drop(playback.seek(1, Some(200)));
        drop(playback.resume_at(Instant::now() + Duration::from_secs(10 * 60)));
        let position = playback.position();
        assert_eq!((position.index, position.offset_ms), (1, 200));

        // Playing and pausing again a moment later: away a moment.
        drop(playback.pause());
        drop(playback.play());
        drop(playback.pause());
        drop(playback.resume_at(Instant::now() + Duration::from_secs(1)));
        assert_eq!(playback.position().index, 1);
    }

    #[test]
    fn sentences_not_said_are_not_counted_when_stepping_back() {
        let tests = crate::core::tests::epub_with;
        let chapter = |body: &str| {
            format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>"#)
        };
        let epub = tests(
            chapter("<p>The nets dried.</p><p>A gull cried.</p><p>The tide turned.</p><p>A boat came in.</p>"),
            chapter("<p>Night fell.</p>"),
        );
        let playback = Playback::new(PlayerCore::new(RATE));
        drop(playback.start(speech_of(&epub, &[0, 1, 2, 3], &[2]), 3));
        playback.tick(&memory(&["k0", "k1", "k3"])).unwrap();
        listen(&playback, 500);
        drop(playback.pause());
        assert_eq!(playback.position().index, 3);
        drop(playback.resume_at(Instant::now() + Duration::from_secs(10 * 60)));
        assert_eq!(playback.position().index, 1, "entry 2 says nothing");
    }

    #[test]
    fn pausing_again_while_paused_still_counts_from_the_first_pause() {
        // A sleep timer that runs out, or a media key, can pause what is already paused.
        let playback = paused_in(1);
        let first = playback.paused_since().unwrap();
        drop(playback.pause());
        drop(playback.resume_at(first + Duration::from_secs(5 * 60)));
        assert_eq!(
            playback.position().index,
            0,
            "five minutes since the first pause"
        );
    }

    #[test]
    fn a_pause_carried_over_a_restart_still_counts_from_when_it_began() {
        let playback = paused_in(1);
        let since = playback.paused_since();
        drop(playback.stop());
        drop(playback.start(speech(&[0, 1, 2, 3]), 1));
        drop(playback.pause_since(since));
        // Five minutes from the first pause, a moment less from the second.
        drop(playback.resume_at(since.unwrap() + Duration::from_secs(5 * 60)));
        assert_eq!(playback.position().index, 0);
    }

    #[test]
    fn advancing_to_another_entry_carries_generation_along_and_saves_the_place_now_and_then() {
        let playback = Playback::new(PlayerCore::new(RATE));
        drop(playback.start(speech(&[0, 1]), 0));
        let audio = memory(&["k0", "k1"]);
        let mut aligned = 0;
        let first = playback.advance(&audio, |to_align| aligned += to_align.len());
        assert_eq!(aligned, 2);
        assert_eq!(
            first.listening,
            Some(Listening::Carried),
            "a session's first tick"
        );
        assert!(first.place.is_none(), "saved a moment ago, at the start");
        let still = playback.advance(&audio, |_| {});
        assert_eq!(still.parts(), (false, None, None), "the playhead stayed");
    }
}
