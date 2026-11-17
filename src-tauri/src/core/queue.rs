//! The queue: items lined up to listen to next. When the item playing ends, the first queued
//! item that can be listened to plays from its place after a short gap ([`GAP`]), and leaves
//! the queue. The order is kept in the library; the hand-over is one [`Stage`], changed only by
//! [`next`] over named [`Event`]s.

use std::time::{Duration, Instant};

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
    /// The tick looked at the player.
    Tick(Instant),
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
        (Stage::Between { from, since }, Tick(now)) if *now >= *since + GAP => (
            Stage::Idle,
            queued.then(|| Action::HandOver { from: from.clone() }),
        ),
        (Stage::Between { .. }, UserPlayed | UserStopped) => (Stage::Idle, None),
        (Stage::Between { .. }, Changed) if !queued => (Stage::Idle, None),
        (stage, _) => (stage.clone(), None),
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
        let early = Event::Tick(t0 + GAP - Duration::from_millis(1));
        let due = Event::Tick(t0 + GAP);
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
