//! Where an article's fetch stands. One transition function over named events; the item's
//! page and card render the stage.

use aoide_article::{Completeness, WallKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "reason", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "FetchStage"))]
pub enum Stage {
    /// In line to be fetched (one fetch at a time).
    Queued,
    Fetching,
    /// The article was got whole.
    Ready,
    /// Something was got, but not the whole article; what was captured is readable.
    NeedsYou(Reason),
    /// Nothing was got; no snapshot is stored.
    Failed,
}

/// Why a fetch needs the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "FetchReason"))]
pub enum Reason {
    SignIn,
    Paywall,
    /// A "checking your browser" page.
    Check,
    CutShort,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The worker took it up.
    Started,
    /// A snapshot is stored; this is how complete it is.
    Got(Completeness),
    /// Nothing was got and nothing is stored.
    FetchFailed,
    /// The user asked to fetch it again.
    Retried,
    /// Aoide stopped while it was being fetched.
    Interrupted,
}

pub fn next(stage: Stage, event: Event) -> Stage {
    use Event::*;
    use Stage::*;
    match (stage, event) {
        (Queued, Started) => Fetching,
        (Fetching, Got(completeness)) => match completeness {
            Completeness::Complete => Ready,
            Completeness::CutShort { .. } => NeedsYou(Reason::CutShort),
            Completeness::Wall(WallKind::SignIn) => NeedsYou(Reason::SignIn),
            Completeness::Wall(WallKind::Paywall) => NeedsYou(Reason::Paywall),
            Completeness::Wall(WallKind::Check) => NeedsYou(Reason::Check),
            Completeness::Empty => NeedsYou(Reason::Empty),
        },
        (Fetching, FetchFailed) => Failed,
        (Ready | NeedsYou(_) | Failed, Retried) => Queued,
        (Fetching, Interrupted) => Queued,
        (stage, _) => stage,
    }
}

impl Stage {
    /// The name stored in the library, and the reason's.
    pub fn names(self) -> (&'static str, Option<&'static str>) {
        match self {
            Stage::Queued => ("queued", None),
            Stage::Fetching => ("fetching", None),
            Stage::Ready => ("ready", None),
            Stage::NeedsYou(reason) => ("needs_you", Some(reason.name())),
            Stage::Failed => ("failed", None),
        }
    }

    /// A stored stage. A fetch under way when Aoide stopped is queued again.
    pub fn from_stored(name: &str, reason: Option<&str>) -> Stage {
        match name {
            "queued" => Stage::Queued,
            "fetching" => next(Stage::Fetching, Event::Interrupted),
            "ready" => Stage::Ready,
            "needs_you" => {
                Stage::NeedsYou(reason.and_then(Reason::from_name).unwrap_or(Reason::Empty))
            }
            _ => Stage::Failed,
        }
    }
}

impl Reason {
    fn name(self) -> &'static str {
        match self {
            Reason::SignIn => "sign_in",
            Reason::Paywall => "paywall",
            Reason::Check => "check",
            Reason::CutShort => "cut_short",
            Reason::Empty => "empty",
        }
    }

    fn from_name(name: &str) -> Option<Reason> {
        [
            Reason::SignIn,
            Reason::Paywall,
            Reason::Check,
            Reason::CutShort,
            Reason::Empty,
        ]
        .into_iter()
        .find(|r| r.name() == name)
    }
}

#[cfg(test)]
mod tests {
    use super::Event::*;
    use super::Stage::*;
    use super::*;

    const ALL: [Stage; 5] = [Queued, Fetching, Ready, NeedsYou(Reason::Paywall), Failed];

    #[test]
    fn the_stage_table() {
        let cut = Completeness::CutShort {
            captured: 10,
            declared: 900,
        };
        let table = [
            (Queued, Started, Fetching),
            (Fetching, Got(Completeness::Complete), Ready),
            (Fetching, Got(cut), NeedsYou(Reason::CutShort)),
            (
                Fetching,
                Got(Completeness::Wall(WallKind::SignIn)),
                NeedsYou(Reason::SignIn),
            ),
            (
                Fetching,
                Got(Completeness::Wall(WallKind::Paywall)),
                NeedsYou(Reason::Paywall),
            ),
            (
                Fetching,
                Got(Completeness::Wall(WallKind::Check)),
                NeedsYou(Reason::Check),
            ),
            (Fetching, Got(Completeness::Empty), NeedsYou(Reason::Empty)),
            (Fetching, FetchFailed, Failed),
            (Ready, Retried, Queued),
            (NeedsYou(Reason::CutShort), Retried, Queued),
            (Failed, Retried, Queued),
            (Fetching, Interrupted, Queued),
        ];
        for (stage, event, to) in table {
            assert_eq!(next(stage, event), to, "{stage:?} + {event:?}");
        }
    }

    #[test]
    fn events_that_do_not_apply_change_nothing() {
        for stage in [Queued, Ready, NeedsYou(Reason::Check), Failed] {
            for event in [Got(Completeness::Complete), FetchFailed, Interrupted] {
                assert_eq!(next(stage, event), stage, "{stage:?} + {event:?}");
            }
        }
        assert_eq!(next(Fetching, Retried), Fetching);
        assert_eq!(next(Fetching, Started), Fetching);
    }

    #[test]
    fn stages_are_stored_by_name_and_an_interrupted_fetch_is_queued_again() {
        for stage in ALL {
            let (name, reason) = stage.names();
            let back = Stage::from_stored(name, reason);
            let expected = if stage == Fetching { Queued } else { stage };
            assert_eq!(back, expected, "{stage:?}");
        }
        for reason in [
            Reason::SignIn,
            Reason::Paywall,
            Reason::Check,
            Reason::CutShort,
            Reason::Empty,
        ] {
            let (name, stored) = NeedsYou(reason).names();
            assert_eq!(Stage::from_stored(name, stored), NeedsYou(reason));
        }
    }
}
