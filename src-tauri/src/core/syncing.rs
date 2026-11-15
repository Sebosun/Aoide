//! The sync of a book's recording: one explicit [`Stage`], changed only by [`next`] in response
//! to an [`Event`]. Progress numbers sit beside the stage and never decide it. A recording plays
//! fully once it is [`Stage::Ready`] (accepting a poor match makes it ready), and what is synced
//! so far plays while it syncs (see [`Stage::listenable`]).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "SyncStage"))]
pub enum Stage {
    /// Finding out where it stands: files present and unchanged, the quick sample check,
    /// what is stored.
    Checking,
    /// In line to be transcribed.
    Waiting,
    /// The speech-recognition model is not on this computer yet.
    NeedsModel,
    /// Being transcribed (one recording at a time).
    Transcribing,
    /// Transcribed; being matched to the text.
    Matching,
    /// Matched well, or accepted: it plays.
    Ready,
    /// The quick check (a few samples transcribed before the rest) did not find the samples in
    /// the book's text.
    SampleMismatch,
    /// Matched, but too little of the text was found to trust it without the user's word.
    PoorMatch,
    /// Kept in line, but not transcribed until resumed.
    Paused,
    /// Its audio files are gone or changed.
    Missing,
    Failed,
    /// An audiobook on its own: there is no text to sync with, and it plays as it is. Only
    /// attaching text moves it on.
    Standalone,
}

/// What Checking found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// No transcript yet, or part of one: transcription carries on from where it stopped.
    Untranscribed,
    /// A whole transcript, but no map for the current text.
    Transcribed,
    /// A map for the current text. `review`: poor and not accepted.
    Matched { review: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Verified(Found),
    SampleFailed,
    /// First in line, and the model is there.
    Promote,
    ModelMissing,
    ModelReady,
    TranscriptDone,
    /// `review`: poor and not accepted.
    Matched {
        review: bool,
    },
    /// The user keeps a doubtful sample check or a poor match.
    Accept,
    Pause,
    Resume,
    /// The book's text (or the matcher) changed: the map no longer fits.
    TextChanged,
    FilesLost,
    FilesFound,
    Fail,
    Retry,
    /// Transcribe and match again from scratch (the caller clears what is stored).
    Resync,
    /// An audiobook on its own was given its text: it syncs like any recording from now on.
    #[allow(dead_code)] // Sent by the merge with an EPUB (phase 3.8).
    TextAttached,
}

/// The whole state machine. Events that do not apply leave the stage as it is.
pub fn next(stage: Stage, event: Event) -> Stage {
    use Event::*;
    use Stage::*;
    match (stage, event) {
        (Standalone, TextAttached) => Checking,
        // Its files going missing shows on the item itself, like a missing EPUB.
        (Standalone, _) => Standalone,
        (_, FilesLost) => Missing,
        (Missing, FilesFound) => Checking,
        (Checking, Verified(Found::Untranscribed)) => Waiting,
        (Checking, Verified(Found::Transcribed)) => Matching,
        (Checking, Verified(Found::Matched { review: false })) => Ready,
        (Checking, Verified(Found::Matched { review: true })) => PoorMatch,
        (Transcribing, SampleFailed) => SampleMismatch,
        (Waiting, Promote) => Transcribing,
        (Waiting, ModelMissing) => NeedsModel,
        (NeedsModel, ModelReady) => Waiting,
        (Transcribing, TranscriptDone) => Matching,
        (Matching, Matched { review: false }) => Ready,
        (Matching, Matched { review: true }) => PoorMatch,
        (SampleMismatch, Accept) => Waiting,
        (PoorMatch, Accept) => Ready,
        (Waiting | NeedsModel | Transcribing, Pause) => Paused,
        (Paused, Resume) => Waiting,
        (Ready | PoorMatch, TextChanged) => Matching,
        (Checking | Transcribing | Matching, Fail) => Failed,
        (Failed, Retry) => Checking,
        (Ready | PoorMatch | SampleMismatch | Failed | Paused, Resync) => Checking,
        (stage, _) => stage,
    }
}

impl Stage {
    /// The name stored in the library.
    pub fn name(self) -> &'static str {
        match self {
            Stage::Checking => "checking",
            Stage::Waiting => "waiting",
            Stage::NeedsModel => "needsModel",
            Stage::Transcribing => "transcribing",
            Stage::Matching => "matching",
            Stage::Ready => "ready",
            Stage::SampleMismatch => "sampleMismatch",
            Stage::PoorMatch => "poorMatch",
            Stage::Paused => "paused",
            Stage::Missing => "missing",
            Stage::Failed => "failed",
            Stage::Standalone => "standalone",
        }
    }

    /// A stored name. Work that was under way when Aoide stopped is checked again, and so is a
    /// ready recording (its files may have moved, or the book's text changed).
    pub fn from_stored(name: &str) -> Stage {
        match name {
            "waiting" | "transcribing" | "matching" | "checking" | "ready" => Stage::Checking,
            "needsModel" => Stage::NeedsModel,
            "sampleMismatch" => Stage::SampleMismatch,
            "poorMatch" => Stage::PoorMatch,
            "paused" => Stage::Paused,
            "missing" => Stage::Missing,
            "standalone" => Stage::Standalone,
            _ => Stage::Failed,
        }
    }

    /// Whether what has been synced so far can be listened to: once synced, or while the sync
    /// is under way (the part not reached yet waits for it).
    pub fn listenable(self) -> bool {
        matches!(
            self,
            Stage::Ready
                | Stage::Waiting
                | Stage::Transcribing
                | Stage::Paused
                | Stage::Matching
                | Stage::Standalone
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Event::*;
    use super::Stage::*;
    use super::*;

    const ALL: [Stage; 12] = [
        Checking,
        Waiting,
        NeedsModel,
        Transcribing,
        Matching,
        Ready,
        SampleMismatch,
        PoorMatch,
        Paused,
        Missing,
        Failed,
        Standalone,
    ];

    #[test]
    fn the_stage_table() {
        let table = [
            (Checking, Verified(Found::Untranscribed), Waiting),
            (Checking, Verified(Found::Transcribed), Matching),
            (Checking, Verified(Found::Matched { review: false }), Ready),
            (
                Checking,
                Verified(Found::Matched { review: true }),
                PoorMatch,
            ),
            (Transcribing, SampleFailed, SampleMismatch),
            (Waiting, Promote, Transcribing),
            (Waiting, ModelMissing, NeedsModel),
            (NeedsModel, ModelReady, Waiting),
            (Transcribing, TranscriptDone, Matching),
            (Matching, Matched { review: false }, Ready),
            (Matching, Matched { review: true }, PoorMatch),
            (SampleMismatch, Accept, Waiting),
            (PoorMatch, Accept, Ready),
            (Waiting, Pause, Paused),
            (NeedsModel, Pause, Paused),
            (Transcribing, Pause, Paused),
            (Paused, Resume, Waiting),
            (Ready, TextChanged, Matching),
            (PoorMatch, TextChanged, Matching),
            (Missing, FilesFound, Checking),
            (Checking, Fail, Failed),
            (Transcribing, Fail, Failed),
            (Matching, Fail, Failed),
            (Failed, Retry, Checking),
            (Ready, Resync, Checking),
            (PoorMatch, Resync, Checking),
            (SampleMismatch, Resync, Checking),
            (Failed, Resync, Checking),
            (Paused, Resync, Checking),
            (Standalone, TextAttached, Checking),
        ];
        for (from, event, to) in table {
            assert_eq!(next(from, event), to, "{from:?} + {event:?}");
        }
    }

    #[test]
    fn losing_the_files_stops_anything() {
        for stage in ALL.into_iter().filter(|s| *s != Standalone) {
            assert_eq!(next(stage, FilesLost), Missing, "{stage:?}");
        }
    }

    #[test]
    fn events_that_do_not_apply_change_nothing() {
        for stage in [Ready, Paused, Missing, Failed, SampleMismatch] {
            for event in [Promote, TranscriptDone, Matched { review: false }, Fail] {
                assert_eq!(next(stage, event), stage, "{stage:?} + {event:?}");
            }
        }
        assert_eq!(next(Ready, Accept), Ready);
        assert_eq!(next(Transcribing, Resync), Transcribing);
        for stage in ALL {
            assert_eq!(
                next(stage, TextAttached),
                if stage == Standalone { Checking } else { stage }
            );
        }
    }

    #[test]
    fn an_audiobook_on_its_own_stays_so_until_it_is_given_text() {
        for event in [
            Verified(Found::Untranscribed),
            SampleFailed,
            Promote,
            ModelMissing,
            ModelReady,
            TranscriptDone,
            Matched { review: false },
            Accept,
            Pause,
            Resume,
            TextChanged,
            FilesLost,
            FilesFound,
            Fail,
            Retry,
            Resync,
        ] {
            assert_eq!(next(Standalone, event), Standalone, "{event:?}");
        }
    }

    #[test]
    fn stored_names_come_back_and_unfinished_work_is_checked_again() {
        for stage in [
            NeedsModel,
            SampleMismatch,
            PoorMatch,
            Paused,
            Missing,
            Failed,
            Standalone,
        ] {
            assert_eq!(Stage::from_stored(stage.name()), stage);
        }
        for stage in [Checking, Waiting, Transcribing, Matching, Ready] {
            assert_eq!(Stage::from_stored(stage.name()), Checking);
        }
    }

    #[test]
    fn a_recording_can_be_listened_to_once_synced_or_while_it_syncs() {
        for stage in ALL {
            let listenable = matches!(
                stage,
                Ready | Waiting | Transcribing | Paused | Matching | Standalone
            );
            assert_eq!(stage.listenable(), listenable, "{stage:?}");
        }
    }
}
