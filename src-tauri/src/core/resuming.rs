//! Stepping back a little on resume: after a break, playback starts a little earlier so the
//! listener catches the thread. How far depends only on how long they were away.

use std::time::Duration;

/// How long the listener was away: since the pause, or since the saved place was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Away(pub Duration);

/// How far a book with text steps back (speech, or a synced recording).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepBack {
    /// Plays on from the playhead.
    None,
    /// From the start of the sentence playing.
    SentenceStart,
    /// From the start of the sentence this many spoken sentences earlier.
    Sentences(u8),
}

/// Away less than this, nothing moves.
const SHORT: Duration = Duration::from_secs(10);
/// Away this long, a sentence earlier.
const LONG: Duration = Duration::from_secs(5 * 60);
/// Away this long, two sentences earlier.
const VERY_LONG: Duration = Duration::from_secs(60 * 60);

/// How far a book with text steps back after `away`.
pub fn step_back(Away(away): Away) -> StepBack {
    if away < SHORT {
        StepBack::None
    } else if away < LONG {
        StepBack::SentenceStart
    } else if away < VERY_LONG {
        StepBack::Sentences(1)
    } else {
        StepBack::Sentences(2)
    }
}

/// How far an audiobook with no text steps back after `away`, in milliseconds.
pub fn step_back_ms(away: Away) -> u64 {
    match step_back(away) {
        StepBack::None => 0,
        StepBack::SentenceStart => 3_000,
        StepBack::Sentences(1) => 10_000,
        StepBack::Sentences(_) => 20_000,
    }
}

/// The entry to play from after stepping back `step` from entry `index`, or `None` to play on.
/// Only entries `spoken` says are spoken count as sentences, and it never goes before entry
/// `first` (the chapter's first).
pub fn entry_back(
    index: usize,
    step: StepBack,
    first: usize,
    spoken: impl Fn(usize) -> bool,
) -> Option<usize> {
    match step {
        StepBack::None => None,
        StepBack::SentenceStart => Some(index),
        StepBack::Sentences(n) => Some(
            (first.min(index)..index)
                .rev()
                .filter(|&i| spoken(i))
                .take(usize::from(n))
                .last()
                .unwrap_or(index),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: u64 = 1;
    const MIN: u64 = 60;
    const H: u64 = 3_600;

    #[test]
    fn the_longer_away_the_further_back() {
        let table = [
            // away (s), with text, audiobook alone (ms)
            (0, StepBack::None, 0),
            (9 * S, StepBack::None, 0),
            (10 * S, StepBack::SentenceStart, 3_000),
            (5 * MIN - 1, StepBack::SentenceStart, 3_000),
            (5 * MIN, StepBack::Sentences(1), 10_000),
            (H - 1, StepBack::Sentences(1), 10_000),
            (H, StepBack::Sentences(2), 20_000),
            (30 * 24 * H, StepBack::Sentences(2), 20_000),
        ];
        for (seconds, text, alone) in table {
            let away = Away(Duration::from_secs(seconds));
            assert_eq!(step_back(away), text, "{seconds} s away, with text");
            assert_eq!(step_back_ms(away), alone, "{seconds} s away, no text");
        }
        let just_under = Away(Duration::from_millis(9_999));
        assert_eq!(
            (step_back(just_under), step_back_ms(just_under)),
            (StepBack::None, 0)
        );
    }
}
