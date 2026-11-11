//! The playback session: the book being listened to, which entries the player has been given
//! audio for, and their word timings. Core does the file reading and decoding around it.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use aoide_audio::Pcm;
use aoide_library::WordTiming;
use aoide_text::WordEstimate;

use super::Target;
use crate::book::word_index;

/// Decoded audio kept ready ahead of the playhead.
pub const PROVIDE_AHEAD: usize = 12;
/// Entries kept behind the playhead (a short skip back plays at once).
const KEEP_BEHIND: usize = 2;

pub struct Session {
    pub target: Target,
    provided: HashSet<usize>,
    /// Word timings of provided entries (stored, or fitted to the decoded audio).
    timings: HashMap<usize, Vec<WordEstimate>>,
    pub last_index: usize,
    pub last_saved: Instant,
}

impl Session {
    pub fn new(target: Target, start: usize) -> Self {
        Self {
            target,
            provided: HashSet::new(),
            timings: HashMap::new(),
            last_index: start,
            last_saved: Instant::now(),
        }
    }

    /// Entries from the playhead on whose audio is made but not given to the player yet, with
    /// their cache keys, nearest first.
    pub fn wanted(&self, playhead: usize) -> Vec<(usize, String)> {
        let items = &self.target.plan.items;
        let end = (playhead + PROVIDE_AHEAD).min(items.len());
        (playhead.min(end)..end)
            .filter(|i| {
                let item = &items[*i];
                item.speakable && item.cached && !self.provided.contains(i)
            })
            .map(|i| (i, items[i].key.clone()))
            .collect()
    }

    /// The player now has audio for `index`. Uses stored word timings when they fit the words,
    /// otherwise fits timings to the pauses in the audio. Returns whether exact alignment should
    /// be asked for.
    pub fn provided(&mut self, index: usize, stored: Option<&[WordTiming]>, pcm: &Pcm) -> bool {
        self.provided.insert(index);
        let Some(text) = self.target.book.segment(index) else {
            return false;
        };
        let stored: Option<Vec<WordEstimate>> =
            stored.filter(|t| t.len() == text.words.len()).map(|t| {
                t.iter()
                    .map(|w| WordEstimate {
                        start_ms: w.start_ms,
                        end_ms: w.end_ms,
                    })
                    .collect()
            });
        let align = stored.is_none();
        let timings = stored
            .unwrap_or_else(|| aoide_text::align_to_pauses(text, &pcm.samples, pcm.sample_rate));
        self.timings.insert(index, timings);
        align
    }

    /// Exact timings arrived for an entry the player has.
    pub fn aligned(&mut self, index: usize, timings: Vec<WordEstimate>) {
        if let Some(current) = self.timings.get_mut(&index) {
            *current = timings;
        }
    }

    /// Forgets entries far behind the playhead. Returns the entry before which the player may
    /// release audio.
    pub fn release_behind(&mut self, playhead: usize) -> Option<usize> {
        let before = playhead.checked_sub(KEEP_BEHIND).filter(|b| *b > 0)?;
        self.provided.retain(|i| *i >= before);
        self.timings.retain(|i, _| *i >= before);
        Some(before)
    }

    /// The word being spoken `offset_ms` into entry `index`, if its audio is playing.
    pub fn word_at(&self, index: usize, offset_ms: u32) -> Option<usize> {
        let item = self.target.plan.items.get(index)?;
        if !item.speakable || !item.cached {
            return None;
        }
        word_index(
            self.timings.get(&index).map(Vec::as_slice),
            self.target.book.segment(index)?,
            item.duration_ms,
            offset_ms,
        )
    }

    pub fn segment_made(&mut self, key: &str, duration_ms: u32) {
        self.target.plan.mark_cached(key, duration_ms);
    }

    /// Audio was deleted (cache limit, regenerate): it has to be made again.
    pub fn audio_lost(&mut self, keys: &HashSet<String>) {
        self.target.plan.mark_lost(keys);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aoide_generation::PlanItem;

    use super::*;
    use crate::book::BookModel;
    use crate::core::preparation::Plan;

    /// The test book (four entries: heading, paragraph, heading, paragraph) with entry 2's audio
    /// not made yet.
    fn session() -> Session {
        let book = Arc::new(BookModel::parse("b", &crate::core::tests::epub(), false).unwrap());
        let items = (0..book.total_entries())
            .map(|i| PlanItem {
                key: format!("k{i}"),
                speakable: true,
                cached: i != 2,
                duration_ms: 2_000,
            })
            .collect();
        Session::new(
            Target {
                book,
                plan: Plan {
                    items,
                    requests: vec![],
                },
            },
            0,
        )
    }

    fn pcm() -> Pcm {
        Pcm {
            sample_rate: 1_000,
            samples: vec![0.0; 2_000],
        }
    }

    fn indexes(wanted: Vec<(usize, String)>) -> Vec<usize> {
        wanted.into_iter().map(|(i, _)| i).collect()
    }

    #[test]
    fn wants_made_audio_from_the_playhead_that_the_player_does_not_have() {
        let mut s = session();
        assert_eq!(
            s.wanted(0),
            [(0, "k0".into()), (1, "k1".into()), (3, "k3".into())]
        );
        s.provided(0, None, &pcm());
        assert_eq!(indexes(s.wanted(0)), [1, 3]);
        assert_eq!(indexes(s.wanted(2)), [3]);
    }

    #[test]
    fn uses_stored_word_timings_that_fit_and_asks_for_alignment_otherwise() {
        let mut s = session();
        let words = s.target.book.segment(1).unwrap().words.len();
        let stored: Vec<WordTiming> = (0..words as u32)
            .map(|w| WordTiming {
                start_ms: w * 100,
                end_ms: w * 100 + 90,
            })
            .collect();
        assert!(!s.provided(1, Some(&stored), &pcm()));
        assert_eq!(s.word_at(1, 250), Some(2));
        assert!(
            s.provided(3, Some(&stored[..1]), &pcm()),
            "wrong word count: estimate, then align"
        );
        assert!(s.word_at(3, 0).is_some());
        assert_eq!(s.word_at(2, 0), None, "no audio yet");
    }

    #[test]
    fn exact_timings_replace_estimates_only_for_audio_the_player_has() {
        let mut s = session();
        s.provided(1, None, &pcm());
        let words = s.target.book.segment(1).unwrap().words.len() as u32;
        let exact = (0..words)
            .map(|w| WordEstimate {
                start_ms: w * 10,
                end_ms: w * 10 + 9,
            })
            .collect();
        s.aligned(1, exact);
        assert_eq!(s.word_at(1, 25), Some(2));
        s.aligned(3, vec![]); // Not provided: ignored, so the estimate from its length stays.
        assert!(s.word_at(3, 0).is_some());
    }

    #[test]
    fn audio_far_behind_is_released_and_provided_again_after_skipping_back() {
        let mut s = session();
        for i in [0, 1, 3] {
            s.provided(i, None, &pcm());
        }
        assert_eq!(s.release_behind(2), None, "nothing is that far behind yet");
        assert_eq!(s.release_behind(3), Some(1));
        assert_eq!(indexes(s.wanted(0)), [0]);
    }

    #[test]
    fn deleted_audio_is_not_wanted_until_it_is_made_again() {
        let mut s = session();
        s.audio_lost(&HashSet::from(["k1".to_string()]));
        assert_eq!(indexes(s.wanted(0)), [0, 3]);
        s.segment_made("k2", 1_500);
        s.segment_made("k1", 1_500);
        assert_eq!(indexes(s.wanted(0)), [0, 1, 2, 3]);
    }
}
