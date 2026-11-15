//! What sits around playback: the sleep timer, the desktop's media keys and now-playing panel,
//! and the player state last sent to the window.

use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{lock, PlayerStateView, SleepView};
pub use crate::media::MediaUpdate;

#[derive(Debug, Clone, Copy)]
enum SleepTimer {
    At(Instant),
    EndOfChapter(usize),
}

/// What the desktop's now-playing panel shows.
pub struct NowPlaying {
    pub title: String,
    pub album: String,
    pub artist: String,
    pub cover: Option<String>,
}

impl NowPlaying {
    /// What shows while `chapter` of `book` plays, with the book's `cover`.
    pub fn of(book: &crate::book::BookModel, chapter: usize, cover: Option<String>) -> Self {
        Self {
            title: book
                .chapters
                .get(chapter)
                .map(|c| c.title.clone())
                .unwrap_or_default(),
            album: book.title.clone(),
            artist: book.authors.join(", "),
            cover,
        }
    }

    /// What shows while `part` of audiobook `timeline` plays, with its `cover`.
    pub fn of_timeline(
        timeline: &super::timeline::Timeline,
        part: usize,
        cover: Option<String>,
    ) -> Self {
        Self {
            title: timeline
                .parts
                .get(part)
                .map(|p| p.title.clone())
                .unwrap_or_default(),
            album: timeline.title.clone(),
            artist: timeline.authors.join(", "),
            cover,
        }
    }
}

#[derive(Default)]
pub(super) struct Controls {
    sleep: Mutex<Option<SleepTimer>>,
    media: Mutex<Option<Sender<MediaUpdate>>>,
    /// The player state last sent to the window.
    sent: Mutex<Option<PlayerStateView>>,
}

impl Controls {
    /// Sends the media keys' service what plays from now on (`None`: there is none).
    pub fn attach_media(&self, media: Option<Sender<MediaUpdate>>) {
        *lock(&self.media) = media;
    }

    /// `minutes`: pause after that long; `end_of_chapter`: pause once `chapter` (the one
    /// playing) ends. Neither cancels the timer.
    pub fn set_sleep(&self, minutes: Option<u32>, end_of_chapter: bool, chapter: Option<usize>) {
        let timer = if end_of_chapter {
            chapter.map(SleepTimer::EndOfChapter)
        } else {
            minutes.map(|m| SleepTimer::At(Instant::now() + Duration::from_secs(u64::from(m) * 60)))
        };
        *lock(&self.sleep) = timer;
    }

    /// Whether playback should pause now, `chapter` playing; a timer that is due is cleared.
    pub fn sleep_due(&self, chapter: usize) -> bool {
        let mut sleep = lock(&self.sleep);
        let due = match *sleep {
            Some(SleepTimer::At(at)) => Instant::now() >= at,
            Some(SleepTimer::EndOfChapter(c)) => chapter != c,
            None => false,
        };
        if due {
            *sleep = None;
        }
        due
    }

    pub fn sleep_view(&self) -> Option<SleepView> {
        match *lock(&self.sleep) {
            Some(SleepTimer::At(at)) => Some(SleepView {
                remaining_ms: Some(at.saturating_duration_since(Instant::now()).as_millis() as u64),
                end_of_chapter: false,
            }),
            Some(SleepTimer::EndOfChapter(_)) => Some(SleepView {
                remaining_ms: None,
                end_of_chapter: true,
            }),
            None => None,
        }
    }

    /// Takes the player's `state` at a tick: tells the media keys of a new status or chapter
    /// (with `now_playing` for that chapter), and returns the state if the window has yet to
    /// see it.
    pub fn observe(
        &self,
        state: PlayerStateView,
        now_playing: &dyn Fn(usize) -> Option<NowPlaying>,
    ) -> Option<PlayerStateView> {
        let mut sent = lock(&self.sent);
        let previous = sent.as_ref();
        if previous.map(|p| p.status) != Some(state.status) {
            tracing::info!(
                event = "player.status",
                book = state.book_id.as_deref().map(crate::logging::book),
                from = crate::logging::name(previous.map(|p| p.status)),
                to = crate::logging::name(state.status),
                chapter = state.chapter,
                source = crate::logging::name(state.source),
                error = state.error.as_deref(),
                "Player status changed"
            );
            self.send_media(MediaUpdate::Playback(state.status));
        }
        if state.book_id.is_some()
            && previous.map(|p| (&p.book_id, p.chapter)) != Some((&state.book_id, state.chapter))
        {
            if let Some(playing) = now_playing(state.chapter) {
                self.send_media(MediaUpdate::Metadata {
                    title: playing.title,
                    album: playing.album,
                    artist: playing.artist,
                    cover: playing.cover,
                });
            }
        }
        if previous == Some(&state) {
            return None;
        }
        *sent = Some(state.clone());
        Some(state)
    }

    fn send_media(&self, update: MediaUpdate) {
        if let Some(tx) = lock(&self.media).as_ref() {
            let _ = tx.send(update);
        }
    }
}

impl super::Core {
    /// `minutes`: pause after that long; `end_of_chapter`: pause when the chapter ends.
    /// Neither cancels the timer.
    pub fn set_sleep_timer(&self, minutes: Option<u32>, end_of_chapter: bool) {
        let chapter = self.playback.chapter();
        self.controls.set_sleep(minutes, end_of_chapter, chapter);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use super::super::PlaybackStatus;
    use super::*;

    fn state(status: PlaybackStatus, chapter: usize) -> PlayerStateView {
        PlayerStateView {
            book_id: Some("book".into()),
            status,
            chapter,
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
            position_ms: None,
            duration_ms: None,
            part_position_ms: None,
        }
    }

    fn now_playing(chapter: usize) -> Option<NowPlaying> {
        Some(NowPlaying {
            title: format!("Chapter {chapter}"),
            album: "A Book".into(),
            artist: "Someone".into(),
            cover: None,
        })
    }

    #[test]
    fn a_timer_for_the_end_of_the_chapter_is_due_once_the_chapter_changes() {
        let controls = Controls::default();
        controls.set_sleep(None, true, Some(2));
        assert!(!controls.sleep_due(2));
        assert!(controls.sleep_due(3));
        assert!(!controls.sleep_due(4), "it fires once");
        assert_eq!(controls.sleep_view(), None);
    }

    #[test]
    fn a_timer_in_minutes_shows_what_is_left_until_it_is_due() {
        let controls = Controls::default();
        controls.set_sleep(Some(5), false, Some(0));
        let view = controls.sleep_view().expect("set");
        assert!(!view.end_of_chapter);
        assert!(view
            .remaining_ms
            .is_some_and(|ms| ms > 290_000 && ms <= 300_000));
        assert!(!controls.sleep_due(1), "a timed timer ignores chapters");
    }

    #[test]
    fn only_a_changed_state_is_sent_and_the_media_keys_hear_of_a_new_status_or_chapter() {
        let controls = Controls::default();
        let (tx, rx) = channel();
        controls.attach_media(Some(tx));

        assert!(controls
            .observe(state(PlaybackStatus::Playing, 0), &now_playing)
            .is_some());
        assert!(controls
            .observe(state(PlaybackStatus::Playing, 0), &now_playing)
            .is_none());
        assert!(controls
            .observe(state(PlaybackStatus::Paused, 1), &now_playing)
            .is_some());

        let sent: Vec<String> = rx
            .try_iter()
            .map(|update| match update {
                MediaUpdate::Playback(status) => format!("{status:?}"),
                MediaUpdate::Metadata { title, .. } => title,
            })
            .collect();
        assert_eq!(sent, ["Playing", "Chapter 0", "Paused", "Chapter 1"]);
    }
}
