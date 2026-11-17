//! What sits around playback: the sleep timer, the desktop's media keys and now-playing panel,
//! and the player state last sent to the window.

use std::sync::mpsc::Sender;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{lock, PlayerStateView, SleepView};
pub use crate::media::MediaUpdate;

/// How long a timed sleep fades before it is due.
const FADE: Duration = Duration::from_secs(20);
/// The gain a fade ends at.
const FADE_FLOOR: f32 = 0.2;
/// How long a due sleep waits for the sentence playing to end before it pauses anyway.
const STOP_WAIT: Duration = Duration::from_secs(15);
/// How long after a sleep rang playing again starts the same timer again.
const AGAIN_WITHIN: Duration = Duration::from_secs(3 * 60);

/// When a running sleep timer is due.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Until {
    At(Instant),
    /// Once chapter (an audiobook's part) `usize` ends.
    EndOfChapter(usize),
}

/// The sleep timer. `minutes` is what was asked for, kept to start the same timer again;
/// `extended`: it was started again by playing soon after it rang.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Sleep {
    #[default]
    Off,
    Running {
        until: Until,
        minutes: Option<u32>,
        extended: bool,
    },
    /// The last seconds of a timed sleep: the gain falls until `ends`.
    Fading {
        ends: Instant,
        minutes: u32,
        extended: bool,
    },
    /// Due: waiting for the player to stop at the end of the sentence playing.
    Stopping {
        since: Instant,
        minutes: u32,
        extended: bool,
    },
    /// Paused by the timer `at`; playing again soon starts it again.
    Rang {
        at: Instant,
        minutes: Option<u32>,
        end_of_chapter: bool,
    },
}

/// What happens to the sleep timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SleepEvent {
    /// The listener set it: `minutes`, or the end of `chapter` (the one playing; `None`
    /// when nothing plays). Neither: it is cancelled.
    Set {
        minutes: Option<u32>,
        end_of_chapter: bool,
        chapter: Option<usize>,
        now: Instant,
    },
    Cancelled,
    /// The tick looked at the player: `chapter` plays, or is paused when not `playing`.
    Tick {
        now: Instant,
        chapter: usize,
        playing: bool,
    },
    /// The listener paused.
    Paused(Instant),
    /// The listener played, `chapter` playing.
    Played {
        now: Instant,
        chapter: usize,
    },
}

/// What the player is to do for the sleep timer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum SleepAction {
    /// Play at this gain (1.0 is full).
    Fade(f32),
    /// Pause where the entry playing ends.
    PauseAtEntryEnd,
    /// Play at full gain, and pause where chapter `usize` ends.
    PauseAtChapterEnd(usize),
    /// Pause now (a pause plays at full gain again and lets go of any stop set up).
    Pause,
    /// Play at full gain, with no stop set up.
    RestoreGain,
}

/// The sleep timer after `event`, and what the player is to do.
pub(super) fn next(sleep: Sleep, event: SleepEvent) -> (Sleep, Option<SleepAction>) {
    use SleepAction::*;
    use SleepEvent::*;
    // Fading, stopping and an end of chapter set up hold the player: letting go restores it.
    let release = || {
        matches!(
            sleep,
            Sleep::Fading { .. }
                | Sleep::Stopping { .. }
                | Sleep::Running {
                    until: Until::EndOfChapter(_),
                    ..
                }
        )
        .then_some(RestoreGain)
    };
    let rang = |at, minutes| Sleep::Rang {
        at,
        minutes: Some(minutes),
        end_of_chapter: false,
    };
    match (sleep, event) {
        (
            _,
            Set {
                end_of_chapter: true,
                chapter: Some(chapter),
                ..
            },
        ) => (
            Sleep::Running {
                until: Until::EndOfChapter(chapter),
                minutes: None,
                extended: false,
            },
            Some(PauseAtChapterEnd(chapter)),
        ),
        (
            _,
            Set {
                minutes: Some(minutes),
                end_of_chapter: false,
                now,
                ..
            },
        ) => (
            Sleep::Running {
                until: Until::At(now + minutes_of(minutes)),
                minutes: Some(minutes),
                extended: false,
            },
            release(),
        ),
        (_, Set { .. } | Cancelled) => (Sleep::Off, release()),

        (
            Sleep::Running {
                until: Until::At(ends),
                minutes: Some(minutes),
                extended,
            },
            Tick { now, playing, .. },
        ) => {
            if !playing {
                // Due while paused: there is nothing to stop.
                return (if now >= ends { Sleep::Off } else { sleep }, None);
            }
            if now + FADE < ends {
                return (sleep, None);
            }
            (
                Sleep::Fading {
                    ends,
                    minutes,
                    extended,
                },
                Some(Fade(fade_at(now, ends))),
            )
        }
        (
            Sleep::Running {
                until: Until::EndOfChapter(ending),
                ..
            },
            Tick { now, chapter, .. },
        ) if chapter != ending => (
            Sleep::Rang {
                at: now,
                minutes: None,
                end_of_chapter: true,
            },
            Some(Pause),
        ),
        (
            Sleep::Fading {
                ends,
                minutes,
                extended,
            },
            Tick { now, playing, .. },
        ) => {
            if !playing {
                (rang(now, minutes), Some(Pause))
            } else if now >= ends {
                (
                    Sleep::Stopping {
                        since: now,
                        minutes,
                        extended,
                    },
                    Some(PauseAtEntryEnd),
                )
            } else {
                (sleep, Some(Fade(fade_at(now, ends))))
            }
        }
        (Sleep::Stopping { since, minutes, .. }, Tick { now, playing, .. })
            if !playing || now >= since + STOP_WAIT =>
        {
            (rang(now, minutes), Some(Pause))
        }

        (Sleep::Fading { minutes, .. } | Sleep::Stopping { minutes, .. }, Paused(now)) => {
            (rang(now, minutes), Some(RestoreGain))
        }

        (
            Sleep::Rang {
                at,
                minutes,
                end_of_chapter,
            },
            Played { now, chapter },
        ) => {
            if now >= at + AGAIN_WITHIN {
                return (Sleep::Off, None);
            }
            if end_of_chapter {
                (
                    Sleep::Running {
                        until: Until::EndOfChapter(chapter),
                        minutes: None,
                        extended: true,
                    },
                    Some(PauseAtChapterEnd(chapter)),
                )
            } else {
                let minutes = minutes.unwrap_or_default();
                (
                    Sleep::Running {
                        until: Until::At(now + minutes_of(minutes)),
                        minutes: Some(minutes),
                        extended: true,
                    },
                    None,
                )
            }
        }
        (Sleep::Fading { minutes, .. } | Sleep::Stopping { minutes, .. }, Played { now, .. }) => (
            Sleep::Running {
                until: Until::At(now + minutes_of(minutes)),
                minutes: Some(minutes),
                extended: true,
            },
            Some(RestoreGain),
        ),
        (
            Sleep::Running {
                until: Until::EndOfChapter(ending),
                ..
            },
            Played { chapter, .. },
        ) if chapter == ending => (sleep, Some(PauseAtChapterEnd(ending))),

        (sleep, _) => (sleep, None),
    }
}

fn minutes_of(minutes: u32) -> Duration {
    Duration::from_secs(u64::from(minutes) * 60)
}

/// The gain at `now` of a fade that ends at `ends`: from full down to [`FADE_FLOOR`], in
/// hundredths.
fn fade_at(now: Instant, ends: Instant) -> f32 {
    let left = ends.saturating_duration_since(now).as_secs_f32() / FADE.as_secs_f32();
    let gain = FADE_FLOOR + (1.0 - FADE_FLOOR) * left.min(1.0);
    (gain * 100.0).round() / 100.0
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
    sleep: Mutex<Sleep>,
    media: Mutex<Option<Sender<MediaUpdate>>>,
    /// The player state last sent to the window.
    sent: Mutex<Option<PlayerStateView>>,
}

impl Controls {
    /// Sends the media keys' service what plays from now on (`None`: there is none).
    pub fn attach_media(&self, media: Option<Sender<MediaUpdate>>) {
        *lock(&self.media) = media;
    }

    /// Moves the sleep timer on by `event` (see [`next`]); says what the player is to do.
    pub fn sleep(&self, event: SleepEvent) -> Option<SleepAction> {
        let mut sleep = lock(&self.sleep);
        let before = *sleep;
        let (after, action) = next(before, event);
        *sleep = after;
        drop(sleep);
        log_sleep(before, event, after);
        action
    }

    /// The sleep timer as the player shows it: none once it has rung (fading is not shown).
    pub fn sleep_view(&self) -> Option<SleepView> {
        let now = Instant::now();
        let left = |ends: Instant| Some(ends.saturating_duration_since(now).as_millis() as u64);
        match *lock(&self.sleep) {
            Sleep::Running {
                until: Until::At(ends),
                ..
            }
            | Sleep::Fading { ends, .. } => Some(SleepView {
                remaining_ms: left(ends),
                end_of_chapter: false,
            }),
            Sleep::Stopping { .. } => Some(SleepView {
                remaining_ms: Some(0),
                end_of_chapter: false,
            }),
            Sleep::Running {
                until: Until::EndOfChapter(_),
                ..
            } => Some(SleepView {
                remaining_ms: None,
                end_of_chapter: true,
            }),
            Sleep::Off | Sleep::Rang { .. } => None,
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

/// Logs a change of the sleep timer worth a line: set (or cancelled), rung, or started again.
fn log_sleep(before: Sleep, event: SleepEvent, after: Sleep) {
    let (minutes, end_of_chapter) = match after {
        Sleep::Running { until, minutes, .. } => (minutes, matches!(until, Until::EndOfChapter(_))),
        Sleep::Rang {
            minutes,
            end_of_chapter,
            ..
        } => (minutes, end_of_chapter),
        _ => (None, false),
    };
    if let SleepEvent::Set { .. } | SleepEvent::Cancelled = event {
        tracing::info!(
            event = "sleep.set",
            minutes,
            end_of_chapter,
            "Sleep timer set"
        );
    } else if matches!(after, Sleep::Rang { .. }) && !matches!(before, Sleep::Rang { .. }) {
        tracing::info!(
            event = "sleep.rang",
            minutes,
            end_of_chapter,
            "Sleep timer paused playback"
        );
    } else if matches!(after, Sleep::Running { extended: true, .. })
        && !matches!(before, Sleep::Running { .. })
    {
        tracing::info!(
            event = "sleep.extended",
            minutes,
            end_of_chapter,
            "Sleep timer started again by playing soon after it rang"
        );
    }
}

impl super::Core {
    /// `minutes`: pause after that long; `end_of_chapter`: pause when the chapter ends.
    /// Neither cancels the timer.
    pub fn set_sleep_timer(&self, minutes: Option<u32>, end_of_chapter: bool) {
        if minutes.is_none() && !end_of_chapter {
            return self.sleep(SleepEvent::Cancelled);
        }
        self.sleep(SleepEvent::Set {
            minutes,
            end_of_chapter,
            chapter: self.playback.chapter(),
            now: Instant::now(),
        });
    }

    /// Moves the sleep timer on by `event`, and has the player do what it asks.
    pub(super) fn sleep(&self, event: SleepEvent) {
        let action = self.controls.sleep(event);
        match action {
            None => {}
            Some(SleepAction::Fade(gain)) => self.playback.set_fade(gain),
            Some(SleepAction::PauseAtEntryEnd) => {
                // An audiobook alone has no sentences: it pauses now.
                if !self.playback.pause_at_entry_end() {
                    self.pause();
                }
            }
            Some(SleepAction::PauseAtChapterEnd(chapter)) => {
                self.playback.pause_at_chapter_end(chapter);
            }
            Some(SleepAction::Pause) => {
                self.pause();
                self.playback.restore_gain();
            }
            Some(SleepAction::RestoreGain) => self.playback.restore_gain(),
        }
        if matches!(event, SleepEvent::Set { .. } | SleepEvent::Cancelled) || action.is_some() {
            self.playback.stir();
        }
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
            speed_scope: Default::default(),
            volume: 1.0,
            chapter_fraction: 0.0,
            error: None,
            sleep: None,
            source: Default::default(),
            off_text: false,
            chapter_left_ms: None,
            book_left_ms: None,
            chapter_ms: None,
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
    fn the_sleep_timer_moves_through_its_states_on_named_events() {
        use SleepAction::*;
        use SleepEvent::*;
        let t0 = Instant::now();
        let s = |secs: u64| t0 + Duration::from_secs(secs);
        let min = |m: u64| s(m * 60);
        let running = |until, minutes| Sleep::Running {
            until,
            minutes,
            extended: false,
        };
        let at15 = running(Until::At(min(15)), Some(15));
        let chapter2 = running(Until::EndOfChapter(2), None);
        let fading = Sleep::Fading {
            ends: min(15),
            minutes: 15,
            extended: false,
        };
        let stopping = Sleep::Stopping {
            since: min(15),
            minutes: 15,
            extended: false,
        };
        let rang = |at, minutes, end_of_chapter| Sleep::Rang {
            at,
            minutes,
            end_of_chapter,
        };
        let tick = |now, chapter, playing| Tick {
            now,
            chapter,
            playing,
        };
        let set = |minutes, end_of_chapter, chapter| Set {
            minutes,
            end_of_chapter,
            chapter,
            now: t0,
        };
        let table: Vec<(Sleep, SleepEvent, Sleep, Option<SleepAction>)> = vec![
            // Setting: timed, or until the chapter playing ends (the player is told at once).
            (Sleep::Off, set(Some(15), false, Some(0)), at15, None),
            (
                Sleep::Off,
                set(None, true, Some(2)),
                chapter2,
                Some(PauseAtChapterEnd(2)),
            ),
            (
                Sleep::Off,
                set(Some(15), true, Some(2)),
                chapter2,
                Some(PauseAtChapterEnd(2)),
            ),
            (Sleep::Off, set(None, true, None), Sleep::Off, None),
            (Sleep::Off, set(None, false, Some(0)), Sleep::Off, None),
            (
                fading,
                set(Some(15), false, Some(0)),
                at15,
                Some(RestoreGain),
            ),
            (
                fading,
                set(None, false, Some(0)),
                Sleep::Off,
                Some(RestoreGain),
            ),
            // Cancelling lets go of the player.
            (at15, Cancelled, Sleep::Off, None),
            (chapter2, Cancelled, Sleep::Off, Some(RestoreGain)),
            (fading, Cancelled, Sleep::Off, Some(RestoreGain)),
            (stopping, Cancelled, Sleep::Off, Some(RestoreGain)),
            (rang(s(0), Some(15), false), Cancelled, Sleep::Off, None),
            // A timed timer runs until its last 20 seconds, then fades to a fifth.
            (at15, tick(min(14), 0, true), at15, None),
            (
                at15,
                tick(s(14 * 60 + 40), 0, true),
                fading,
                Some(Fade(1.0)),
            ),
            (
                fading,
                tick(s(14 * 60 + 50), 0, true),
                fading,
                Some(Fade(0.6)),
            ),
            (
                fading,
                tick(s(14 * 60 + 59), 3, true),
                fading,
                Some(Fade(0.24)),
            ),
            // Due: it waits for the sentence to end, at most 15 seconds.
            (
                fading,
                tick(min(15), 0, true),
                stopping,
                Some(PauseAtEntryEnd),
            ),
            (stopping, tick(s(15 * 60 + 5), 0, true), stopping, None),
            (
                stopping,
                tick(s(15 * 60 + 5), 0, false),
                rang(s(15 * 60 + 5), Some(15), false),
                Some(Pause),
            ),
            (
                stopping,
                tick(s(15 * 60 + 15), 0, true),
                rang(s(15 * 60 + 15), Some(15), false),
                Some(Pause),
            ),
            // Paused before it was due: nothing to stop.
            (at15, tick(min(14), 0, false), at15, None),
            (at15, tick(min(15), 0, false), Sleep::Off, None),
            (
                fading,
                tick(s(14 * 60 + 50), 0, false),
                rang(s(14 * 60 + 50), Some(15), false),
                Some(Pause),
            ),
            // The end of the chapter: rings once another chapter plays.
            (chapter2, tick(min(30), 2, true), chapter2, None),
            (
                chapter2,
                tick(min(30), 3, false),
                rang(min(30), None, true),
                Some(Pause),
            ),
            // Pausing: a timer running keeps running; one fading or stopping has rung.
            (at15, Paused(min(1)), at15, None),
            (chapter2, Paused(min(1)), chapter2, None),
            (
                fading,
                Paused(s(14 * 60 + 50)),
                rang(s(14 * 60 + 50), Some(15), false),
                Some(RestoreGain),
            ),
            (
                stopping,
                Paused(s(15 * 60 + 2)),
                rang(s(15 * 60 + 2), Some(15), false),
                Some(RestoreGain),
            ),
            // Playing within three minutes of a ring starts the same timer again.
            (
                rang(min(15), Some(15), false),
                Played {
                    now: min(17),
                    chapter: 4,
                },
                Sleep::Running {
                    until: Until::At(min(32)),
                    minutes: Some(15),
                    extended: true,
                },
                None,
            ),
            (
                rang(min(15), None, true),
                Played {
                    now: min(17),
                    chapter: 3,
                },
                Sleep::Running {
                    until: Until::EndOfChapter(3),
                    minutes: None,
                    extended: true,
                },
                Some(PauseAtChapterEnd(3)),
            ),
            (
                rang(min(15), Some(15), false),
                Played {
                    now: min(18),
                    chapter: 4,
                },
                Sleep::Off,
                None,
            ),
            // Playing while it fades: still awake.
            (
                fading,
                Played {
                    now: s(14 * 60 + 50),
                    chapter: 0,
                },
                Sleep::Running {
                    until: Until::At(s(29 * 60 + 50)),
                    minutes: Some(15),
                    extended: true,
                },
                Some(RestoreGain),
            ),
            // Playing on in the chapter: the player is told its end again.
            (
                chapter2,
                Played {
                    now: min(1),
                    chapter: 2,
                },
                chapter2,
                Some(PauseAtChapterEnd(2)),
            ),
            (
                chapter2,
                Played {
                    now: min(1),
                    chapter: 5,
                },
                chapter2,
                None,
            ),
            (
                at15,
                Played {
                    now: min(1),
                    chapter: 0,
                },
                at15,
                None,
            ),
            (
                Sleep::Off,
                Played {
                    now: min(1),
                    chapter: 0,
                },
                Sleep::Off,
                None,
            ),
            (
                rang(s(0), Some(15), false),
                tick(min(5), 0, true),
                rang(s(0), Some(15), false),
                None,
            ),
        ];
        for (from, event, to, action) in table {
            let label = format!("{from:?} {event:?}");
            assert_eq!(next(from, event), (to, action), "{label}");
        }
    }

    #[test]
    fn the_view_shows_what_is_left_of_a_timer_until_it_rings() {
        let controls = Controls::default();
        let now = Instant::now();
        controls.sleep(SleepEvent::Set {
            minutes: Some(5),
            end_of_chapter: false,
            chapter: Some(0),
            now,
        });
        let view = controls.sleep_view().expect("set");
        assert!(!view.end_of_chapter);
        assert!(view
            .remaining_ms
            .is_some_and(|ms| ms > 290_000 && ms <= 300_000));

        let action = controls.sleep(SleepEvent::Set {
            minutes: None,
            end_of_chapter: true,
            chapter: Some(2),
            now,
        });
        assert_eq!(action, Some(SleepAction::PauseAtChapterEnd(2)));
        let view = controls.sleep_view().expect("set");
        assert_eq!((view.remaining_ms, view.end_of_chapter), (None, true));
        let tick = |chapter| SleepEvent::Tick {
            now,
            chapter,
            playing: true,
        };
        assert_eq!(controls.sleep(tick(2)), None);
        assert_eq!(controls.sleep(tick(3)), Some(SleepAction::Pause));
        assert_eq!(controls.sleep(tick(4)), None, "it rings once");
        assert_eq!(controls.sleep_view(), None);
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
