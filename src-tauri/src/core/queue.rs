//! The queue: items lined up to listen to next. When the item playing ends, the first queued
//! item that can be listened to plays from its place after a short gap ([`GAP`]), and leaves
//! the queue. The order is kept in the library; the hand-over is one [`Stage`], changed only by
//! [`next`] over named [`Event`]s.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_library::{BookId, Library};
use serde::Serialize;

use super::browsing::Pending;
use super::fetching::Stage as FetchStage;
use super::lock;

/// How long after an item ends the next one starts.
pub const GAP: Duration = Duration::from_secs(2);

/// Where the queue is in handing over from one item to the next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Stage {
    Idle,
    /// Item `from` played to its end at `since`; the next plays [`GAP`] later.
    Between {
        from: String,
        since: Instant,
    },
}

/// What happens to the hand-over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Event {
    /// Item `id`, the one playing, played to its end.
    ItemEnded { id: String, now: Instant },
    /// The tick looked at the player; `at_end`: it still sits at the end of what it played.
    Tick { now: Instant, at_end: bool },
    /// The listener played something, or moved the playhead.
    UserPlayed,
    /// The listener paused or stopped.
    UserStopped,
    /// The queue's items changed.
    Changed,
    /// The listener asked for the next chapter at the last chapter of item `id`.
    NextAsked { id: String },
}

/// What the queue is to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Action {
    /// Play the first queued item that can be listened to, in place of `from`.
    HandOver { from: String },
}

/// The stage after `event`, and what to do; `queued`: the queue has items. Events that do not
/// apply leave the stage as it is.
pub(super) fn next(stage: &Stage, event: &Event, queued: bool) -> (Stage, Option<Action>) {
    use Event::*;
    match (stage, event) {
        (_, NextAsked { id }) if queued => {
            (Stage::Idle, Some(Action::HandOver { from: id.clone() }))
        }
        (Stage::Idle, ItemEnded { id, now }) if queued => (
            Stage::Between {
                from: id.clone(),
                since: *now,
            },
            None,
        ),
        // Something else plays, or the listener moved the playhead back.
        (Stage::Between { .. }, Tick { at_end: false, .. }) => (Stage::Idle, None),
        (Stage::Between { from, since }, Tick { now, .. }) if *now >= *since + GAP => (
            Stage::Idle,
            queued.then(|| Action::HandOver { from: from.clone() }),
        ),
        (Stage::Between { .. }, UserPlayed | UserStopped) => (Stage::Idle, None),
        (Stage::Between { .. }, Changed) if !queued => (Stage::Idle, None),
        (stage, _) => (stage.clone(), None),
    }
}

/// Where an item is put in the queue.
#[allow(dead_code)] // `Next` is asked for from the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePlace {
    /// First: it plays when the item playing ends.
    Next,
    Last,
}

/// Why a queued item cannot be listened to now. It stays queued, and is passed over when its
/// turn comes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum WhyNot {
    /// An article still being fetched.
    Fetching,
    /// An article whose fetch got nothing.
    NotFetched,
    /// Its file is missing or cannot be read, or it is not in the library.
    Missing,
    /// It is read by the speech engine, which has no voice.
    NoVoice,
}

/// An item in the queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct QueueItemView {
    pub book_id: String,
    /// Why it cannot be listened to now; `None` when it can.
    pub why_not: Option<WhyNot>,
}

/// The queue, first to play first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct QueueView {
    pub items: Vec<QueueItemView>,
}

/// What the queue asks of the rest of the app.
pub(super) trait QueueHost: Sync {
    /// Plays item `id` from its saved place, in place of what plays.
    fn play<'a>(&'a self, id: &'a str) -> Pending<'a, Result<(), String>>;
    /// Whether item `id` can be listened to now.
    fn can_listen(&self, id: &str) -> Result<(), WhyNot>;
}

/// The queue: its order in the library, and the hand-over.
pub(super) struct Queue {
    library: Arc<Mutex<Library>>,
    stage: Mutex<Stage>,
}

impl Queue {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        Self {
            library,
            stage: Mutex::new(Stage::Idle),
        }
    }

    /// The items queued, first to play first.
    pub fn ids(&self) -> Result<Vec<String>, String> {
        let queued = lock(&self.library).queue().map_err(|e| e.to_string())?;
        Ok(queued.into_iter().map(|id| id.0).collect())
    }

    /// Puts item `id` at place `at` (0 is next; past the end is last), moving it there when it
    /// is queued already.
    pub fn put(&self, id: &str, at: usize) -> Result<(), String> {
        lock(&self.library)
            .queue_put(&BookId(id.into()), at)
            .map_err(|e| e.to_string())?;
        self.changed();
        Ok(())
    }

    /// Takes item `id` off the queue; false when it was not on it.
    pub fn remove(&self, id: &str) -> Result<bool, String> {
        let removed = lock(&self.library)
            .queue_remove(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        if removed {
            self.changed();
        }
        Ok(removed)
    }

    #[allow(dead_code)] // Asked for from the window.
    pub fn clear(&self) -> Result<(), String> {
        lock(&self.library)
            .queue_clear()
            .map_err(|e| e.to_string())?;
        self.changed();
        Ok(())
    }

    fn changed(&self) {
        let size = self.ids().map_or(0, |ids| ids.len());
        tracing::info!(event = "queue.changed", size, "The queue changed");
        self.on(Event::Changed);
    }

    /// Item `id` started playing: it leaves the queue. True when it was queued.
    pub fn started(&self, id: &str) -> bool {
        self.remove(id).unwrap_or(false)
    }

    /// Tells the hand-over of `event`; returns what to do.
    pub fn on(&self, event: Event) -> Option<Action> {
        let mut stage = lock(&self.stage);
        // Idle, only an item ending or Next asks whether anything is queued.
        let asks = !matches!(*stage, Stage::Idle)
            || matches!(event, Event::ItemEnded { .. } | Event::NextAsked { .. });
        let queued = asks && self.ids().is_ok_and(|ids| !ids.is_empty());
        let (after, action) = next(&stage, &event, queued);
        log_stage(&stage, &event, &after);
        *stage = after;
        action
    }

    /// The queue, each item with why it cannot be listened to now, if it cannot.
    pub fn view(&self, host: &impl QueueHost) -> Result<QueueView, String> {
        let items = self
            .ids()?
            .into_iter()
            .map(|id| QueueItemView {
                why_not: host.can_listen(&id).err(),
                book_id: id,
            })
            .collect();
        Ok(QueueView { items })
    }

    /// Plays the first queued item that can be listened to, in place of `from`, and takes it
    /// off the queue; those that cannot are passed over and stay. Returns the item played.
    pub async fn hand_over(&self, host: &impl QueueHost, from: &str) -> Option<String> {
        let ids = self.ids().unwrap_or_default();
        for id in ids {
            if let Err(why) = host.can_listen(&id) {
                tracing::info!(
                    event = "queue.passed_over",
                    book = crate::logging::book(&id),
                    why = crate::logging::name(why),
                    "Passed over a queued item that cannot be listened to now"
                );
                continue;
            }
            match host.play(&id).await {
                Ok(()) => {
                    self.started(&id);
                    log_hand_over(from, Some(&id), "played");
                    return Some(id);
                }
                Err(error) => {
                    tracing::warn!(
                        event = "queue.handover",
                        from = crate::logging::book(from),
                        to = crate::logging::book(&id),
                        outcome = "failed",
                        %error,
                        "Could not play the next item in the queue"
                    );
                }
            }
        }
        log_hand_over(from, None, "nothing_to_play");
        None
    }
}

impl QueueHost for super::Core {
    fn play<'a>(&'a self, id: &'a str) -> Pending<'a, Result<(), String>> {
        Box::pin(super::Core::play(self, id, None, None))
    }

    fn can_listen(&self, id: &str) -> Result<(), WhyNot> {
        let record = lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .ok_or(WhyNot::Missing)?;
        if record.status != aoide_library::BookStatus::Present {
            return Err(WhyNot::Missing);
        }
        if record.kind == aoide_library::ItemKind::Article {
            match self.articles.summary(id).map(|a| a.stage) {
                Some(FetchStage::Queued | FetchStage::Fetching) => return Err(WhyNot::Fetching),
                Some(FetchStage::Failed) => return Err(WhyNot::NotFetched),
                _ => {}
            }
        }
        let recorded = record.kind == aoide_library::ItemKind::Audiobook
            || self.plays_recording(id, &self.book_settings(id));
        if !recorded && self.engine.default_voice().is_none() {
            return Err(WhyNot::NoVoice);
        }
        Ok(())
    }
}

impl super::Core {
    /// Puts item `id` in the queue, next or last (moving it when it is queued already).
    #[allow(dead_code)] // Asked for from the window.
    pub fn queue_add(&self, id: &str, place: QueuePlace) -> Result<(), String> {
        let at = match place {
            QueuePlace::Next => 0,
            QueuePlace::Last => usize::MAX,
        };
        self.queue.put(id, at)
    }

    /// The queue, each item with why it cannot be listened to now, if it cannot.
    #[allow(dead_code)] // Asked for from the window.
    pub fn queue(&self) -> Result<QueueView, String> {
        self.queue.view(self)
    }

    /// Next was asked for at the end of item `from`: the first queued item that can be
    /// listened to plays now. False when none did.
    pub(super) async fn next_in_queue(&self, from: &str) -> bool {
        let asked = Event::NextAsked { id: from.into() };
        match self.queue.on(asked) {
            Some(Action::HandOver { from }) => self.queue.hand_over(self, &from).await.is_some(),
            None => false,
        }
    }

    /// Tells the queue of a tick of the player (`ended`: the item that just played to its
    /// end), and hands over when it is time.
    pub(super) fn queue_tick(self: &Arc<Self>, ended: Option<String>) {
        let now = Instant::now();
        // With the sleep timer set for the end of the chapter, the listener sleeps there.
        let sleeping = || self.controls.sleep_view().is_some_and(|s| s.end_of_chapter);
        if let Some(id) = ended.filter(|_| !sleeping()) {
            self.queue.on(Event::ItemEnded { id, now });
        }
        let at_end = self.playback.position().status == aoide_audio::Status::Finished;
        if let Some(Action::HandOver { from }) = self.queue.on(Event::Tick { now, at_end }) {
            let core = Arc::clone(self);
            tokio::spawn(async move { core.queue.hand_over(&*core, &from).await });
        }
    }
}

fn log_hand_over(from: &str, to: Option<&str>, outcome: &str) {
    tracing::info!(
        event = "queue.handover",
        from = crate::logging::book(from),
        to = to.map(crate::logging::book),
        outcome,
        "The queue handed over to the next item"
    );
}

/// Logs the hand-over waiting for the gap, or let go of.
fn log_stage(before: &Stage, event: &Event, after: &Stage) {
    match (before, after) {
        (Stage::Idle, Stage::Between { from, .. }) => tracing::info!(
            event = "queue.handover",
            from = crate::logging::book(from),
            outcome = "waiting",
            "An item ended: the next in the queue plays shortly"
        ),
        (Stage::Between { from, .. }, Stage::Idle)
            if !matches!(
                event,
                Event::Tick { at_end: true, .. } | Event::NextAsked { .. }
            ) =>
        {
            tracing::info!(
                event = "queue.handover",
                from = crate::logging::book(from),
                outcome = "cancelled",
                "The listener did something before the next item in the queue played"
            )
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn how_the_hand_over_moves_on_each_event() {
        let t0 = Instant::now();
        let between = Stage::Between {
            from: "a".into(),
            since: t0,
        };
        let ended = Event::ItemEnded {
            id: "a".into(),
            now: t0,
        };
        let hand_over = Some(Action::HandOver { from: "a".into() });
        let next_asked = Event::NextAsked { id: "a".into() };
        let early = Event::Tick {
            now: t0 + GAP - Duration::from_millis(1),
            at_end: true,
        };
        let due = Event::Tick {
            now: t0 + GAP,
            at_end: true,
        };
        let moved_on = Event::Tick {
            now: t0,
            at_end: false,
        };
        #[rustfmt::skip]
        let table: Vec<(&Stage, &Event, bool, Stage, Option<Action>)> = vec![
            // An item ends: with something queued, the gap begins.
            (&Stage::Idle, &ended, true, between.clone(), None),
            (&Stage::Idle, &ended, false, Stage::Idle, None),
            (&between, &ended, true, between.clone(), None),
            // After the gap, the next plays.
            (&between, &early, true, between.clone(), None),
            (&between, &due, true, Stage::Idle, hand_over.clone()),
            (&between, &due, false, Stage::Idle, None),
            (&Stage::Idle, &due, true, Stage::Idle, None),
            // Anything the listener does in the gap cancels the hand-over.
            (&between, &Event::UserPlayed, true, Stage::Idle, None),
            (&between, &Event::UserStopped, true, Stage::Idle, None),
            (&between, &moved_on, true, Stage::Idle, None),
            // Emptying the queue in the gap leaves nothing to hand over to.
            (&between, &Event::Changed, false, Stage::Idle, None),
            (&between, &Event::Changed, true, between.clone(), None),
            (&Stage::Idle, &Event::Changed, true, Stage::Idle, None),
            // Next at the last chapter hands over at once.
            (&Stage::Idle, &next_asked, true, Stage::Idle, hand_over.clone()),
            (&between, &next_asked, true, Stage::Idle, hand_over.clone()),
            (&Stage::Idle, &next_asked, false, Stage::Idle, None),
        ];
        for (stage, event, queued, after, action) in table {
            assert_eq!(
                next(stage, event, queued),
                (after, action),
                "{stage:?} on {event:?}, queued: {queued}"
            );
        }
    }
}

#[cfg(test)]
mod component_tests {
    use super::super::tests::library_with;
    use super::*;

    /// Plays what it is asked to, and remembers it; the items in `cannot` cannot be listened
    /// to.
    #[derive(Default)]
    struct Host {
        played: Mutex<Vec<String>>,
        cannot: Vec<(&'static str, WhyNot)>,
    }

    impl QueueHost for Host {
        fn play<'a>(&'a self, id: &'a str) -> Pending<'a, Result<(), String>> {
            lock(&self.played).push(id.into());
            Box::pin(async { Ok(()) })
        }

        fn can_listen(&self, id: &str) -> Result<(), WhyNot> {
            match self.cannot.iter().find(|(c, _)| *c == id) {
                Some((_, why)) => Err(*why),
                None => Ok(()),
            }
        }
    }

    fn queue(ids: &[&str]) -> Queue {
        let queue = Queue::new(library_with(&["a", "b", "c"]));
        for id in ids {
            queue.put(id, usize::MAX).unwrap();
        }
        queue
    }

    fn tick(now: Instant) -> Event {
        Event::Tick { now, at_end: true }
    }

    #[tokio::test]
    async fn an_item_that_ends_hands_over_to_the_first_queued_which_leaves_the_queue() {
        let queue = queue(&["b", "c"]);
        let host = Host::default();
        let t0 = Instant::now();
        let ended = Event::ItemEnded {
            id: "a".into(),
            now: t0,
        };
        assert_eq!(queue.on(ended), None);
        assert_eq!(queue.on(tick(t0 + Duration::from_secs(1))), None);
        let Some(Action::HandOver { from }) = queue.on(tick(t0 + GAP)) else {
            panic!("no hand-over after the gap");
        };
        assert_eq!(queue.hand_over(&host, &from).await.as_deref(), Some("b"));
        assert_eq!(*lock(&host.played), ["b"]);
        assert_eq!(queue.ids().unwrap(), ["c"]);
    }

    #[tokio::test]
    async fn the_listener_playing_in_the_gap_keeps_the_queue_as_it_is() {
        let queue = queue(&["b"]);
        let t0 = Instant::now();
        queue.on(Event::ItemEnded {
            id: "a".into(),
            now: t0,
        });
        queue.on(Event::UserPlayed);
        assert_eq!(queue.on(tick(t0 + GAP)), None);
        assert_eq!(queue.ids().unwrap(), ["b"]);
    }

    #[test]
    fn an_item_queued_that_starts_by_any_route_leaves_the_queue() {
        let queue = queue(&["b", "c"]);
        assert!(queue.started("c"));
        assert!(!queue.started("a"));
        assert_eq!(queue.ids().unwrap(), ["b"]);
    }

    #[tokio::test]
    async fn items_that_cannot_be_listened_to_are_passed_over_and_stay_queued_with_why() {
        let queue = queue(&["a", "b", "c"]);
        let host = Host {
            cannot: vec![("a", WhyNot::Fetching), ("b", WhyNot::NoVoice)],
            ..Host::default()
        };
        assert_eq!(queue.hand_over(&host, "x").await.as_deref(), Some("c"));
        assert_eq!(*lock(&host.played), ["c"]);
        let view = queue.view(&host).unwrap();
        let items: Vec<_> = view
            .items
            .iter()
            .map(|i| (i.book_id.as_str(), i.why_not))
            .collect();
        assert_eq!(
            items,
            [("a", Some(WhyNot::Fetching)), ("b", Some(WhyNot::NoVoice))]
        );
    }

    #[tokio::test]
    async fn with_nothing_that_can_be_listened_to_nothing_plays() {
        let queue = queue(&["a"]);
        let host = Host {
            cannot: vec![("a", WhyNot::Missing)],
            ..Host::default()
        };
        assert_eq!(queue.hand_over(&host, "x").await, None);
        assert!(lock(&host.played).is_empty());
        assert_eq!(queue.ids().unwrap(), ["a"]);
    }
}
