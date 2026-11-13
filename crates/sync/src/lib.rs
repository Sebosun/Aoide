//! Matches what was heard in a recording (a transcript with word times) to a book's text, so each
//! word of the text gets its time in the recording.
//!
//! Everything here is pure and deterministic. Character offsets are Unicode scalar (`char`)
//! offsets into each chapter's display text, as in `aoide-text`. Times are milliseconds on the
//! whole recording's timeline (all of its files laid end to end).

use std::ops::Range;

mod clips;
mod matcher;
mod token;

pub use clips::{clips, Clip, Loudness, Unit};
pub use matcher::match_book;
pub use token::{tokenize, Token};

/// A word a transcriber heard, with its text as reported (punctuation included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeardWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// Where a word of the text is in the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// Matched to a word that was heard.
    Heard { start_ms: u64, end_ms: u64 },
    /// Not heard as written but narrated: misheard, or placed between its neighbours.
    Estimated { start_ms: u64, end_ms: u64 },
    /// Not in the recording.
    Unread,
}

impl Timing {
    pub fn span(&self) -> Option<(u64, u64)> {
        match *self {
            Timing::Heard { start_ms, end_ms } | Timing::Estimated { start_ms, end_ms } => {
                Some((start_ms, end_ms))
            }
            Timing::Unread => None,
        }
    }
}

/// One word of a chapter (as [`tokenize`] splits it) and its timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedWord {
    pub range: Range<usize>,
    pub timing: Timing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterSync {
    pub words: Vec<TimedWord>,
}

/// The result of matching a book to its recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncMap {
    pub chapters: Vec<ChapterSync>,
    pub report: Report,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    /// Words in the text.
    pub words: usize,
    /// Words matched to what was heard.
    pub matched_words: usize,
    /// Words with a time (matched or estimated).
    pub timed_words: usize,
    /// Words heard in the recording.
    pub heard_words: usize,
    /// Heard words matched to the text.
    pub matched_heard_words: usize,
    pub chapters: Vec<ChapterReport>,
    /// Stretches of the recording that are not in the text, longest pauses excluded.
    pub unmatched_audio: Vec<AudioGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChapterReport {
    pub words: usize,
    pub matched_words: usize,
    pub timed_words: usize,
    /// From the first timed word's start to the last one's end.
    pub span: Option<(u64, u64)>,
}

/// Narration that matched no text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioGap {
    pub start_ms: u64,
    pub end_ms: u64,
    /// The first words heard, as reported.
    pub excerpt: String,
}
