//! Playing an audiobook that has no text: its recording straight through, on one timeline in
//! milliseconds. The player plays a list of entries, so the timeline is laid out as stretches of
//! at most [`CHUNK_MS`] (never across a part's boundary), which the feeder decodes a few seconds
//! at a time; only the stretches near the playhead are held in memory.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use aoide_audio::Entry;
use aoide_recording::AudioFile;

use aoide_library::BookId;

use super::feeder::{Fed, Feeder};
use super::playback::Place;
use super::resuming::{self, Away};
use super::{lock, Core};

/// The longest stretch of the timeline one player entry holds.
pub const CHUNK_MS: u64 = 10_000;
/// A place this close to the end is the end: the item is finished, and plays again from the
/// start.
pub const AT_END_MS: u64 = 1_000;
/// Stretches decoded ahead of the playhead.
const AHEAD: usize = 6;
/// Stretches kept behind the playhead (a short skip back plays at once).
const BEHIND: usize = 4;

/// A part of an audiobook: an embedded chapter, else one of its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub title: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// An audiobook laid out for the player.
#[derive(Debug)]
pub struct Timeline {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub files: Vec<AudioFile>,
    pub total_ms: u64,
    pub parts: Vec<Part>,
    /// The player's entries: stretches of the timeline, in order, covering all of it.
    pub chunks: Vec<Range<u64>>,
}

impl Timeline {
    /// `starts`: each part's title and where it starts; the first part starts at 0 whatever is
    /// given, and a part ends where the next starts (the last at `total_ms`). With no parts,
    /// the whole recording is one.
    pub fn new(
        id: String,
        title: String,
        authors: Vec<String>,
        files: Vec<AudioFile>,
        total_ms: u64,
        starts: Vec<(String, u64)>,
    ) -> Self {
        let mut starts: Vec<(String, u64)> = starts
            .into_iter()
            .filter(|(_, start)| *start < total_ms)
            .collect();
        starts.sort_by_key(|(_, start)| *start);
        starts.dedup_by_key(|(_, start)| *start);
        if !starts.first().is_some_and(|(_, start)| *start == 0) {
            let title = if starts.is_empty() {
                title.clone()
            } else {
                String::new()
            };
            starts.insert(0, (title, 0));
        }
        let parts: Vec<Part> = starts
            .iter()
            .enumerate()
            .map(|(n, (title, start))| Part {
                title: if title.trim().is_empty() {
                    format!("Part {}", n + 1)
                } else {
                    title.clone()
                },
                start_ms: *start,
                end_ms: starts.get(n + 1).map_or(total_ms, |(_, next)| *next),
            })
            .collect();
        let chunks = parts
            .iter()
            .flat_map(|part| {
                (part.start_ms..part.end_ms)
                    .step_by(CHUNK_MS as usize)
                    .map(move |from| from..(from + CHUNK_MS).min(part.end_ms))
            })
            .collect();
        Self {
            id,
            title,
            authors,
            files,
            total_ms,
            parts,
            chunks,
        }
    }

    /// The player's entries, one per stretch.
    pub fn entries(&self) -> Vec<Entry> {
        (0..self.chunks.len())
            .map(|n| Entry {
                id: format!("t{n}"),
                speakable: true,
                pause_after_ms: 0,
            })
            .collect()
    }

    /// The stretch holding `at_ms` and how far into it that is; past the end, the end (the
    /// start, for a recording with no length).
    pub fn locate(&self, at_ms: u64) -> (usize, u32) {
        let last = self.chunks.len().saturating_sub(1);
        let index = self
            .chunks
            .partition_point(|chunk| chunk.end <= at_ms)
            .min(last);
        let Some(chunk) = self.chunks.get(index) else {
            return (0, 0);
        };
        (
            index,
            at_ms
                .clamp(chunk.start, chunk.end)
                .saturating_sub(chunk.start) as u32,
        )
    }

    /// The time `offset_ms` into stretch `index`.
    pub fn time_at(&self, index: usize, offset_ms: u32) -> u64 {
        self.chunks.get(index).map_or(self.total_ms, |chunk| {
            (chunk.start + u64::from(offset_ms)).min(chunk.end)
        })
    }

    /// The part holding `at_ms`.
    pub fn part_at(&self, at_ms: u64) -> usize {
        self.parts
            .partition_point(|part| part.end_ms <= at_ms)
            .min(self.parts.len().saturating_sub(1))
    }
}

/// An audiobook being listened to: its timeline, and which stretches the player was given.
pub struct TimelineSession {
    pub timeline: Arc<Timeline>,
    feeder: Feeder,
    /// Pieces of earlier generations (before a jump) are dropped.
    generation: u64,
    /// Stretches asked of the feeder.
    provided: HashSet<usize>,
    /// Stretches whose audio has arrived whole.
    finished: HashSet<usize>,
    /// The playhead at the last tick; `None` before the first.
    pub ticked: Option<usize>,
    pub last_saved: Instant,
}

impl TimelineSession {
    /// Decodes `timeline`'s files at `rate` on a feeder thread.
    pub fn new(timeline: Arc<Timeline>, rate: u32) -> Self {
        Self {
            feeder: Feeder::spawn(timeline.files.clone(), rate),
            timeline,
            generation: 0,
            provided: HashSet::new(),
            finished: HashSet::new(),
            ticked: None,
            last_saved: Instant::now(),
        }
    }

    /// Asks the feeder for the stretches near the playhead it was not asked for, and returns
    /// the pieces decoded since the last call that the player should get.
    pub fn feed(&mut self, playhead: usize) -> Vec<Fed> {
        let ahead = (playhead + AHEAD).min(self.timeline.chunks.len());
        for index in playhead..ahead {
            if self.provided.insert(index) {
                self.feeder.push(index, self.timeline.chunks[index].clone());
            }
        }
        let mut pieces = self.feeder.take();
        pieces.retain(|p| p.generation == self.generation);
        for piece in pieces.iter().filter(|p| p.last) {
            self.finished.insert(piece.index);
        }
        pieces
    }

    /// After a jump: drops what the feeder had not delivered. Returns the stretches whose
    /// partly delivered audio the player should release.
    pub fn restart_feed(&mut self) -> Vec<usize> {
        self.generation = self.feeder.reset();
        let unfinished: Vec<usize> = self
            .provided
            .iter()
            .copied()
            .filter(|i| !self.finished.contains(i))
            .collect();
        for index in &unfinished {
            self.provided.remove(index);
        }
        unfinished
    }

    /// Forgets stretches far behind the playhead. Returns the stretch before which the player
    /// may release audio.
    pub fn release_behind(&mut self, playhead: usize) -> Option<usize> {
        let before = playhead.checked_sub(BEHIND).filter(|b| *b > 0)?;
        self.provided.retain(|i| *i >= before);
        self.finished.retain(|i| *i >= before);
        Some(before)
    }
}

/// Where `timeline` plays from, stepping back from `at_ms` for a listener `away` (see
/// [`resuming::step_back_ms`]), never before the part's start. A step back is logged with
/// `from`, what `away` was measured from. `None` plays on.
pub(super) fn step_back_on(
    timeline: &Timeline,
    at_ms: u64,
    away: Away,
    from: &'static str,
) -> Option<u64> {
    let ms = resuming::step_back_ms(away);
    if ms == 0 {
        return None;
    }
    let part_start = timeline.parts.get(timeline.part_at(at_ms))?.start_ms;
    let back = at_ms.saturating_sub(ms).max(part_start.min(at_ms));
    tracing::info!(
        event = "playback.resumed",
        book = crate::logging::book(&timeline.id),
        away_s = away.0.as_secs(),
        stepped_back = at_ms - back,
        unit = "ms",
        from,
        "Stepped back on resume"
    );
    Some(back)
}

impl Core {
    /// Audiobook `id` laid out for the player.
    pub(super) fn timeline_of(&self, id: &str) -> Result<Timeline, String> {
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("book not found")?;
        let source = self.recordings.timeline_source(id)?;
        if source.total_ms == 0 {
            return Err("The recording has no audio to play.".into());
        }
        Ok(Timeline::new(
            id.to_owned(),
            record.title.unwrap_or_default(),
            record.authors,
            source.files,
            source.total_ms,
            source.parts,
        ))
    }

    /// Plays audiobook `id` from the start of `part`, else from where the listener was (from
    /// the start once it was finished).
    pub(super) fn play_timeline(&self, id: &str, part: Option<usize>) -> Result<(), String> {
        self.choose_mode(id, super::reading::ModeEvent::ListenChosen)?;
        if self.playback.is_book(id) {
            let Some(part) = part else {
                return self.resume_playing();
            };
            self.seek_part(id, part)?;
            return self.follow_through(self.playback.play());
        }
        let timeline = self.timeline_of(id)?;
        let at = match part {
            Some(part) => {
                timeline
                    .parts
                    .get(part)
                    .ok_or("that part is not in the audiobook")?
                    .start_ms
            }
            // From the saved place, a little earlier the longer ago it was saved.
            None => self
                .saved_progress(id)
                .and_then(|p| Some((p.position_ms?, p.updated_at)))
                .filter(|(at, _)| at + AT_END_MS < timeline.total_ms)
                .map_or(0, |(at, saved_at)| {
                    let away = Away::since(saved_at, super::now_ms());
                    self.steps_back()
                        .then(|| step_back_on(&timeline, at, away, "saved"))
                        .flatten()
                        .unwrap_or(at)
                }),
        };
        self.playback
            .use_own_speed(self.book_settings(id).playback_speed);
        self.follow_through(self.playback.start_timeline(Arc::new(timeline), at))
    }

    /// Moves to `position_ms` on audiobook `id`'s timeline: the playhead when it plays, else
    /// its saved place.
    pub fn seek_to(&self, id: &str, position_ms: u64) -> Result<(), String> {
        if self.playback.time_of(id).is_some() {
            return self.follow_through(self.playback.seek_time(id, position_ms)?);
        }
        let timeline = self.timeline_of(id)?;
        let at = position_ms.min(timeline.total_ms);
        self.store_place(Place::Time {
            id: id.to_owned(),
            part: timeline.part_at(at),
            position_ms: at,
            total_ms: timeline.total_ms,
        })
    }

    /// Moves `delta_ms` back or forward on audiobook `id`'s timeline (the ±30 s buttons).
    pub fn skip(&self, id: &str, delta_ms: i64) -> Result<(), String> {
        let at = self.time_in(id);
        self.seek_to(id, at.saturating_add_signed(delta_ms))
    }

    /// Audiobook `id`'s saved place on its timeline.
    fn saved_time(&self, id: &str) -> Option<u64> {
        self.saved_progress(id).and_then(|p| p.position_ms)
    }

    fn saved_progress(&self, id: &str) -> Option<aoide_library::Progress> {
        lock(&self.library)
            .progress(&BookId(id.into()))
            .ok()
            .flatten()
    }

    /// Where the listener is in audiobook `id`: the playhead while it plays, else its saved
    /// place.
    pub(super) fn time_in(&self, id: &str) -> u64 {
        self.playback
            .time_of(id)
            .or_else(|| self.saved_time(id))
            .unwrap_or(0)
    }

    /// Where the listener is in audiobook `id`: the part, the time, and the part's title.
    pub(super) fn audiobook_place(&self, id: &str) -> Result<(usize, u64, String), String> {
        let timeline = self.timeline_of(id)?;
        let at = self.time_in(id).min(timeline.total_ms);
        let part = timeline.part_at(at);
        Ok((part, at, timeline.parts[part].title.clone()))
    }

    /// The item view of audiobook `id`: its parts as chapters; a file that is gone as a
    /// warning.
    pub(super) fn audiobook_overview(&self, id: &str) -> Result<super::BookView, String> {
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("book not found")?;
        let (chapters, warnings) = match self.timeline_of(id) {
            Ok(timeline) => (
                timeline
                    .parts
                    .iter()
                    .enumerate()
                    .map(|(index, part)| crate::book::ChapterSummary {
                        index,
                        title: part.title.clone(),
                        duration_ms: Some(part.end_ms - part.start_ms),
                        left_out: false,
                        suggested: false,
                        word_count: 0,
                    })
                    .collect(),
                Vec::new(),
            ),
            Err(error) => (Vec::new(), vec![error]),
        };
        let mode = self.mode_of(id);
        Ok(super::BookView {
            id: id.to_owned(),
            kind: aoide_library::ItemKind::Audiobook,
            title: record.title.unwrap_or_default(),
            authors: record.authors,
            language: record.language,
            chapters,
            warnings,
            excluded: Vec::new(),
            language_warning: None,
            voice_warning: None,
            progress: self.shelf.progress(&BookId(id.into())),
            word_count: 0,
            narrated_word_count: 0,
            chapters_left_out: 0,
            mode,
            shape: super::reading::shape(false, mode),
        })
    }

    /// Plays the start of the part `delta` parts away from the one playing.
    pub(super) fn skip_part(&self, timeline: &Timeline, delta: i64) -> Result<(), String> {
        let at = self.playback.time_of(&timeline.id).unwrap_or(0);
        let last = timeline.parts.len().saturating_sub(1) as i64;
        let part = (timeline.part_at(at) as i64 + delta).clamp(0, last) as usize;
        self.seek_part(&timeline.id, part)?;
        self.follow_through(self.playback.play())
    }

    fn seek_part(&self, id: &str, part: usize) -> Result<(), String> {
        let timeline = self.playback.timeline().ok_or("nothing is playing")?;
        let start = timeline
            .parts
            .get(part)
            .ok_or("that part is not in the audiobook")?
            .start_ms;
        self.follow_through(self.playback.seek_time(id, start)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timeline(total_ms: u64, starts: &[(&str, u64)]) -> Timeline {
        Timeline::new(
            "a".into(),
            "Item".into(),
            vec![],
            vec![],
            total_ms,
            starts.iter().map(|(t, s)| (t.to_string(), *s)).collect(),
        )
    }

    #[test]
    fn parts_cover_the_timeline_and_stretches_never_cross_them() {
        let t = timeline(25_000, &[("One", 0), ("Two", 12_000)]);
        let parts: Vec<_> = t
            .parts
            .iter()
            .map(|p| (p.title.as_str(), p.start_ms, p.end_ms))
            .collect();
        assert_eq!(parts, [("One", 0, 12_000), ("Two", 12_000, 25_000)]);
        assert_eq!(
            t.chunks,
            [0..10_000, 10_000..12_000, 12_000..22_000, 22_000..25_000]
        );
        assert_eq!(t.entries().len(), 4);
    }

    #[test]
    fn missing_or_odd_parts_still_cover_it() {
        let whole = timeline(5_000, &[]);
        assert_eq!(whole.parts.len(), 1);
        assert_eq!(whole.parts[0].title, "Item");
        // A first part starting late gets a part before it; one past the end is dropped.
        let late = timeline(5_000, &[("", 2_000), ("After", 9_000)]);
        let parts: Vec<_> = late
            .parts
            .iter()
            .map(|p| (p.title.as_str(), p.start_ms, p.end_ms))
            .collect();
        assert_eq!(parts, [("Part 1", 0, 2_000), ("Part 2", 2_000, 5_000)]);
    }

    #[test]
    fn a_time_is_found_in_its_stretch_and_part() {
        let t = timeline(25_000, &[("One", 0), ("Two", 12_000)]);
        assert_eq!(t.locate(0), (0, 0));
        assert_eq!(t.locate(10_500), (1, 500));
        assert_eq!(t.locate(12_000), (2, 0));
        assert_eq!(t.locate(99_000), (3, 3_000), "past the end: the end");
        assert_eq!(t.time_at(2, 1_500), 13_500);
        assert_eq!(t.part_at(11_999), 0);
        assert_eq!(t.part_at(12_000), 1);
    }

    #[test]
    fn a_recording_with_no_length_has_no_stretches_and_finds_every_time_at_its_start() {
        let t = timeline(0, &[("One", 0)]);
        assert!(t.chunks.is_empty());
        assert_eq!(t.locate(5_000), (0, 0));
        assert_eq!(t.part_at(5_000), 0);
    }

    // ---------------------------------------------------------------- playing it

    use std::path::Path;
    use std::time::Duration;

    use aoide_audio::PlayerCore;
    use aoide_tts::testing::FakeEngine;

    use super::super::playback::{MadeSpeech, Playback, SpeechAudio};
    use super::super::recording_tests::{write_recording_part, RATE, SCALE};
    use super::super::tests::{library_folder, start_with, until};
    use super::super::{PlaybackStatus, ReadState};

    /// No made speech: a timeline has none.
    struct NoSpeech;

    impl SpeechAudio for NoSpeech {
        fn read(&self, _key: &str) -> Result<Option<MadeSpeech>, String> {
            Ok(None)
        }
    }

    /// Lets the feeder decode what the player asked for.
    fn feed(playback: &Playback) {
        for _ in 0..20 {
            playback.tick(&NoSpeech);
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Plays `ms` and checks each sample heard is the time the timeline says, from `from_ms`.
    fn hear(playback: &Playback, from_ms: u64, ms: u64) {
        let mut heard = Vec::new();
        for _ in 0..ms / 500 {
            feed(playback);
            let mut out = vec![0.0; (500 * RATE / 1_000) as usize];
            lock(playback.player()).render(&mut out);
            heard.extend(out);
        }
        let per_ms = f64::from(RATE) / 1_000.0;
        for (n, sample) in heard.iter().enumerate() {
            let expected = from_ms as f64 + n as f64 / per_ms;
            let got = f64::from(*sample) * f64::from(SCALE);
            assert!(
                (got - expected).abs() < 2.0,
                "{n} samples from {from_ms} ms: heard {got} ms, not {expected} ms"
            );
        }
    }

    #[test]
    fn the_player_hears_the_timeline_across_stretches_and_files_and_after_a_seek() {
        let dir = tempfile::tempdir().unwrap();
        write_recording_part(&dir.path().join("01.wav"), 0, 12_000);
        write_recording_part(&dir.path().join("02.wav"), 12_000, 8_000);
        let found = aoide_recording::inspect(&[dir.path().to_path_buf()]);
        let timeline = Arc::new(Timeline::new(
            "a".into(),
            "Item".into(),
            vec![],
            found.files.clone(),
            found.total_ms(),
            vec![],
        ));
        assert_eq!(timeline.total_ms, 20_000);
        let playback = Playback::new(PlayerCore::new(RATE));
        let _ = playback.start_timeline(timeline, 9_000);
        // Over a stretch's end (10 s) and a file's (12 s).
        hear(&playback, 9_000, 4_000);
        assert_eq!(playback.time_of("a"), Some(13_000));

        let moved = playback.seek_time("a", 3_000).unwrap();
        assert!(
            moved.listening.is_none(),
            "the speech Generation makes does not depend on a timeline's playhead"
        );
        let Some(Place::Time { position_ms, .. }) = moved.place else {
            panic!("a place on the timeline")
        };
        assert_eq!(position_ms, 3_000);
        hear(&playback, 3_000, 1_000);
        let state = playback.state();
        assert_eq!(
            (state.position_ms, state.duration_ms, state.part_position_ms),
            (Some(4_000), Some(20_000), Some(4_000))
        );
    }

    #[test]
    fn an_audiobook_alone_steps_back_in_seconds_but_not_over_a_part_start() {
        let t = Arc::new(timeline(60_000, &[("One", 0), ("Two", 30_000)]));
        let playback = Playback::new(PlayerCore::new(RATE));
        drop(playback.start_timeline(t, 45_000));
        let resumed_after = |secs: u64| {
            drop(playback.pause());
            drop(playback.resume_at(Instant::now() + Duration::from_secs(secs)));
            playback.time_of("a").unwrap()
        };
        assert_eq!(resumed_after(1), 45_000);
        assert_eq!(resumed_after(60), 42_000);
        assert_eq!(resumed_after(10 * 60), 32_000);
        assert_eq!(resumed_after(2 * 60 * 60), 30_000, "the part's start");
    }

    /// Three files: "01 Dusk" 40 s, "02 Midnight" 20 s, "03 Dawn" 5 s.
    fn three_parts(root: &Path) -> std::path::PathBuf {
        let folder = root.join("Audio/Long Night");
        write_recording_part(&folder.join("01 Dusk.wav"), 0, 40_000);
        write_recording_part(&folder.join("02 Midnight.wav"), 40_000, 20_000);
        write_recording_part(&folder.join("03 Dawn.wav"), 60_000, 5_000);
        folder
    }

    async fn core(root: &Path) -> Arc<Core> {
        library_folder(root, "");
        let (core, _events) = start_with(
            root,
            &FakeEngine::new(),
            Arc::new(|_| Err("no speech recognition in this test".into())),
        );
        until("the library scan", || {
            core.list_books().is_ok_and(|b| b.len() == 1)
        })
        .await;
        core
    }

    fn saved_ms(core: &Core, id: &str) -> Option<u64> {
        lock(&core.library)
            .progress(&BookId(id.into()))
            .unwrap()?
            .position_ms
    }

    /// Waits for the player's state to hold `check`.
    async fn state_until(
        core: &Core,
        what: &str,
        check: impl Fn(&super::super::PlayerStateView) -> bool,
    ) {
        until(what, || check(&core.player_state())).await;
    }

    #[tokio::test]
    async fn an_audiobook_with_its_own_speed_plays_at_it_and_the_book_after_it_at_the_usual_one() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        let book = core.list_books().unwrap()[0].id.clone();
        let folder = three_parts(dir.path());
        let id = core
            .add_audiobook(vec![folder.to_string_lossy().into()])
            .book_id
            .unwrap();
        let own = super::super::BookSettings {
            playback_speed: Some(0.9),
            ..Default::default()
        };
        core.shelf.save_settings(&id, &own).unwrap();

        core.play(&id, None, None).await.unwrap();
        assert_eq!(core.player_state().speed, 0.9);
        core.play(&book, Some(0), None).await.unwrap();
        assert_eq!(core.player_state().speed, 1.0);
    }

    #[tokio::test]
    async fn an_audiobook_plays_seeks_skips_and_resumes_on_its_timeline() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        let folder = three_parts(dir.path());
        let id = core
            .add_audiobook(vec![folder.to_string_lossy().into()])
            .book_id
            .unwrap();
        let parts: Vec<_> = core
            .timeline_of(&id)
            .unwrap()
            .parts
            .into_iter()
            .map(|p| (p.title, p.start_ms, p.end_ms))
            .collect();
        assert_eq!(
            parts,
            [
                ("01 Dusk".to_string(), 0, 40_000),
                ("02 Midnight".to_string(), 40_000, 60_000),
                ("03 Dawn".to_string(), 60_000, 65_000)
            ]
        );

        core.play(&id, None, None).await.unwrap();
        state_until(&core, "playing", |s| {
            s.status == PlaybackStatus::Playing && s.position_ms > Some(0)
        })
        .await;
        let state = core.player_state();
        assert_eq!(state.book_id.as_deref(), Some(id.as_str()));
        assert_eq!((state.chapter, state.duration_ms), (0, Some(65_000)));
        assert_eq!(state.part_position_ms, state.position_ms);
        assert!(state.chapter_left_ms.is_some_and(|ms| ms < 40_000));

        // Pausing saves the place; playing again goes on from it, also after stopping.
        core.pause();
        let paused = core.player_state().position_ms.unwrap();
        assert_eq!(saved_ms(&core, &id), Some(paused));
        core.stop();
        core.play(&id, None, None).await.unwrap();
        let resumed = core.player_state().position_ms.unwrap();
        assert!(
            (paused..paused + 500).contains(&resumed),
            "{resumed} after {paused}"
        );

        // Seeking and skipping, while it plays.
        core.seek_to(&id, 1_000).unwrap();
        assert_eq!(saved_ms(&core, &id), Some(1_000));
        core.skip(&id, 30_000).unwrap();
        assert_eq!(saved_ms(&core, &id), Some(31_000));
        core.skip(&id, -30_000).unwrap();
        core.skip(&id, -30_000).unwrap();
        assert_eq!(saved_ms(&core, &id), Some(0), "not before the start");
        core.skip_chapter(1).await.unwrap();
        assert_eq!(saved_ms(&core, &id), Some(40_000));
        assert_eq!(core.player_state().chapter, 1);
        core.skip_chapter(5).await.unwrap();
        assert_eq!(saved_ms(&core, &id), Some(60_000), "the last part at most");
        core.skip_chapter(-1).await.unwrap();
        assert_eq!(saved_ms(&core, &id), Some(40_000));

        // Asleep at the end of the part: stopped where the next one starts.
        core.seek_to(&id, 59_000).unwrap();
        core.set_sleep_timer(None, true);
        state_until(&core, "the sleep timer", |s| {
            s.status == PlaybackStatus::Paused
        })
        .await;
        let slept = core.player_state();
        assert_eq!(slept.chapter, 2);
        assert!(
            (60_000..61_500).contains(&slept.position_ms.unwrap()),
            "{:?}",
            slept.position_ms
        );

        // Playing to the end finishes it.
        core.seek_to(&id, 63_000).unwrap();
        let read_state = |core: &Core| {
            let books = core.list_books().unwrap();
            books.into_iter().find(|b| b.id == id).unwrap().read_state
        };
        assert_eq!(read_state(&core), ReadState::InProgress);
        core.resume().unwrap();
        until("the end", || saved_ms(&core, &id) == Some(65_000)).await;
        assert_eq!(read_state(&core), ReadState::Finished);
        assert!(core
            .book_listening(&id)
            .unwrap()
            .iter()
            .any(|day| day.ms > 0));

        // Played again once finished: from the start.
        core.stop();
        core.play(&id, None, None).await.unwrap();
        assert!(core.player_state().position_ms < Some(500));
        core.stop();

        // Played from a place saved a while ago: a little earlier, not before its part.
        let saved_ago = |position_ms: u64, ago_ms: i64| {
            lock(&core.library)
                .save_progress(
                    &BookId(id.clone()),
                    &aoide_library::Progress {
                        chapter: 1,
                        char_offset: 0,
                        segment_id: None,
                        updated_at: super::super::now_ms() - ago_ms,
                        position_ms: Some(position_ms),
                    },
                )
                .unwrap();
        };
        saved_ago(55_000, 10 * 60 * 1_000);
        core.play(&id, None, None).await.unwrap();
        let at = core.player_state().position_ms.unwrap();
        assert!((45_000..45_500).contains(&at), "{at}");
        core.stop();
        saved_ago(50_000, 2 * 60 * 60 * 1_000);
        core.play(&id, None, None).await.unwrap();
        let at = core.player_state().position_ms.unwrap();
        assert!((40_000..40_500).contains(&at), "{at}");
        core.stop();

        // Not playing, a seek moves the saved place.
        core.seek_to(&id, 45_000).unwrap();
        assert_eq!(saved_ms(&core, &id), Some(45_000));
        core.skip(&id, -10_000).unwrap();
        assert_eq!(saved_ms(&core, &id), Some(35_000));
    }
}
