//! Playback: the book being listened to (its [`Session`]) and the player that plays it. The
//! session and player locks are only taken here, the session's always first. Neither is held
//! while audio is read or decoded. Made speech comes through [`SpeechAudio`]: from the library
//! in the app, from memory in tests.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_audio::{codec, Pcm, PlayerCore, Position, Status};
use aoide_library::{Library, WordTiming};
use aoide_text::WordEstimate;

use super::align::Alignable;
use super::narration::NarrationPlan;
use super::recording::{Narration, RecordingPlan};
use super::session::Session;
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
pub struct Place {
    pub book: Arc<BookModel>,
    pub index: usize,
}

pub struct Playback {
    /// Shared with the audio output, which renders from it.
    player: Arc<Mutex<PlayerCore>>,
    session: Mutex<Option<Session>>,
    error: Mutex<Option<String>>,
    /// The audio output's latency: the word shown is the word heard.
    latency_ms: Mutex<u32>,
}

impl Playback {
    pub fn new(player: PlayerCore) -> Self {
        Self {
            player: Arc::new(Mutex::new(player)),
            session: Mutex::new(None),
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

    // ------------------------------------------------------------ what is playing

    pub fn book(&self) -> Option<Arc<BookModel>> {
        lock(&self.session)
            .as_ref()
            .map(|s| Arc::clone(&s.target.book))
    }

    pub fn is_book(&self, id: &str) -> bool {
        lock(&self.session)
            .as_ref()
            .is_some_and(|s| s.target.book.id == id)
    }

    /// The recording of book `id`, when it is being listened to.
    pub fn recording_of(&self, id: &str) -> Option<Arc<RecordingPlan>> {
        lock(&self.session)
            .as_ref()
            .filter(|s| s.target.book.id == id)
            .and_then(|s| s.recording().cloned())
    }

    /// Runs `f` on the narration of the book being listened to, with the book's id.
    pub fn with_narration<R>(&self, f: impl FnOnce(&str, &NarrationPlan) -> R) -> Option<R> {
        lock(&self.session)
            .as_ref()
            .map(|s| f(&s.target.book.id, &s.target.narration))
    }

    /// Entries in `range` of book `id` that its recording, being listened to, has no audio for.
    pub fn unrecorded(&self, id: &str, range: Range<usize>) -> Option<Vec<usize>> {
        lock(&self.session)
            .as_ref()
            .filter(|s| s.target.book.id == id && s.recording().is_some())
            .map(|s| s.target.narration.unrecorded(range))
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
        let rate = {
            let mut player = lock(&self.player);
            player.load(entries, start);
            player.seek_to(start, offset);
            player.play();
            player.output_rate()
        };
        *lock(&self.session) = Some(Session::new(target, rate));
        *lock(&self.error) = None;
    }

    /// Ends the session; returns where it was.
    #[must_use = "the place is lost unless it is saved"]
    pub fn stop(&self) -> Option<Place> {
        let place = self.place_to_save(true);
        lock(&self.player).load(Vec::new(), 0);
        *lock(&self.session) = None;
        place
    }

    /// Moves the playhead `offset_ms` into entry `index`; `None` starts where the entry's text
    /// does (for a recording, just before its first word).
    #[must_use = "the place is lost unless it is saved"]
    pub fn seek(&self, index: usize, offset_ms: Option<u32>) -> Option<Place> {
        let (offset, released) = match lock(&self.session).as_mut() {
            Some(session) => {
                let offset = offset_ms.unwrap_or_else(|| {
                    session
                        .recording()
                        .map_or(0, |plan| plan.start_offset(index))
                });
                (offset, session.restart_feed())
            }
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

    // ------------------------------------------------------------ audio

    /// Gives the player the audio near the playhead it does not have yet (the recording's
    /// clips as they are decoded, made speech read from `audio`), and lets it release audio far
    /// behind. `None` when nothing is playing.
    pub fn tick(&self, audio: &dyn SpeechAudio) -> Option<Tick> {
        let playhead = self.position().index;
        let (moved, pieces, wanted, mut to_align) = {
            let mut session = lock(&self.session);
            let session = session.as_mut()?;
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
            if let Some(session) = lock(&self.session).as_mut() {
                if session.provided(index, timings.as_deref(), &pcm) {
                    to_align.push(Alignable::Speech {
                        book: session.target.book.id.clone(),
                        index,
                        key,
                    });
                }
            }
        }
        let release = lock(&self.session).as_mut()?.release_behind(playhead);
        if let Some(before) = release {
            lock(&self.player).release_before(before);
        }
        Some(Tick { moved, to_align })
    }

    #[cfg(test)]
    pub fn timings_of(&self, index: usize) -> Option<Vec<WordEstimate>> {
        lock(&self.session).as_ref()?.timings_of(index)
    }

    /// Exact word times arrived for entry `index` of book `book`.
    pub fn aligned(&self, book: &str, index: usize, timings: Vec<WordEstimate>) {
        if let Some(session) = lock(&self.session)
            .as_mut()
            .filter(|s| s.target.book.id == book)
        {
            session.aligned(index, timings);
        }
    }

    /// The speech with cache key `key` is made.
    pub fn segment_made(&self, key: &str, duration_ms: u32) {
        if let Some(session) = lock(&self.session).as_mut() {
            session.segment_made(key, duration_ms);
        }
    }

    /// The speech with these cache keys was deleted.
    pub fn audio_lost(&self, keys: &HashSet<String>) {
        if let Some(session) = lock(&self.session).as_mut() {
            session.audio_lost(keys);
        }
    }

    // ------------------------------------------------------------ following a sync

    /// Book `id`'s recording, being listened to while it syncs, should take up the sync's
    /// progress: playback waits for it, it has not for a while, or `force`.
    pub fn should_follow(&self, id: &str, force: bool) -> Option<Arc<BookModel>> {
        let playhead = self.position().index;
        let session = lock(&self.session);
        let session = session.as_ref()?;
        (session.target.book.id == id
            && session.recording().is_some()
            && (force
                || session.target.narration.waits_for_sync(playhead)
                || session.followed.elapsed() >= FOLLOW_EVERY))
            .then(|| Arc::clone(&session.target.book))
    }

    /// Book `id` plays by `target`, its recording synced further, from now on. Audio the player
    /// has stays; what turned out not to be in the recording is passed over. Returns whether the
    /// speech engine has other entries to say now; `None` when the book is not playing.
    pub fn follow(&self, id: &str, target: Target) -> Option<bool> {
        let followed = lock(&self.session)
            .as_mut()
            .filter(|s| s.target.book.id == id)?
            .follow(target);
        let mut player = lock(&self.player);
        for index in followed.passed_over {
            player.finish(index);
        }
        Some(followed.speech_changed)
    }

    // ------------------------------------------------------------ views

    /// The playhead, when `force` or it was last saved a while ago.
    #[must_use = "the place is lost unless it is saved"]
    pub fn place_to_save(&self, force: bool) -> Option<Place> {
        let mut session = lock(&self.session);
        let session = session.as_mut()?;
        if !force && session.last_saved.elapsed() < Duration::from_secs(3) {
            return None;
        }
        session.last_saved = Instant::now();
        let book = Arc::clone(&session.target.book);
        let index = self
            .position()
            .index
            .min(book.total_entries().saturating_sub(1));
        Some(Place { book, index })
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
        let session = lock(&self.session);
        let Some(session) = session.as_ref() else {
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
            };
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
        let place = playback.stop().expect("where it stopped");
        assert_eq!((place.book.id.as_str(), place.index), ("b", 0));
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
        let place = playback.seek(2, None).expect("the place sought");
        assert_eq!(place.index, 2);
        let tick = playback.tick(&memory(&["k0", "k1", "k2", "k3"])).unwrap();
        assert_eq!((playback.position().index, tick.moved), (2, true));
        assert_eq!(playback.state().segment_id, Some("1:0:0".into()));
    }
}
