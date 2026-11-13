use std::ops::Range;

use crate::SyncMap;

/// One playlist entry's text: a segment's character range in its chapter, and its words'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub chapter: usize,
    pub range: Range<usize>,
    pub words: Vec<Range<usize>>,
}

/// The part of the recording a unit plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    /// From the cut before the unit to the cut after it, in recording milliseconds. Empty when
    /// the unit is not in the recording.
    pub audio: Range<u64>,
    /// Where its words are spoken, in recording milliseconds.
    pub text: Option<Range<u64>>,
    /// Each word's span relative to `audio.start`; `None` for words not in the recording.
    pub words: Vec<Option<Range<u64>>>,
    /// Narration in the clip that is not in the text (credits, an omitted dedication), in
    /// recording milliseconds.
    pub off_text: Vec<Range<u64>>,
}

/// Loudness of the recording in fixed frames (any monotonic scale; lower is quieter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loudness {
    pub frame_ms: u32,
    pub levels: Vec<u8>,
}

/// Lays `units` (in playlist order) onto the recording.
///
/// Timed units, taken in recording order, get contiguous clips: the first starts at 0, the last
/// ends at `total_ms`, and each cut between two units falls at the quietest moment of the pause
/// between them (its middle without `loudness`). Narration that matched no text and sits between
/// two units is played as the later one's lead-in, so the cut goes just after the earlier unit.
/// Units with no timed word get empty clips.
pub fn clips(
    map: &SyncMap,
    units: &[Unit],
    total_ms: u64,
    loudness: Option<&Loudness>,
) -> Vec<Clip> {
    let spans: Vec<Vec<Option<(u64, u64)>>> = units
        .iter()
        .map(|unit| {
            let words = &map.chapters[unit.chapter].words;
            unit.words
                .iter()
                .map(|word| {
                    let from = words.partition_point(|t| t.range.end <= word.start);
                    words[from..]
                        .iter()
                        .take_while(|t| t.range.start < word.end)
                        .filter_map(|t| t.timing.span())
                        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
                })
                .collect()
        })
        .collect();
    let text: Vec<Option<(u64, u64)>> = spans
        .iter()
        .map(|words| {
            let first = words.iter().flatten().next()?;
            let last = words.iter().flatten().last()?;
            Some((first.0, last.1.max(first.0)))
        })
        .collect();
    let mut order: Vec<usize> = (0..units.len()).filter(|&i| text[i].is_some()).collect();
    order.sort_by_key(|&i| (text[i].map(|t| t.0), i));

    let gaps = &map.report.unmatched_audio;
    let mut audio: Vec<Option<Range<u64>>> = vec![None; units.len()];
    let mut start = 0;
    for (k, &i) in order.iter().enumerate() {
        let (_, text_end) = text[i].expect("timed");
        let end = match order.get(k + 1) {
            None => total_ms.max(text_end),
            Some(&j) => cut(text_end, text[j].expect("timed").0, gaps, loudness).max(start),
        };
        audio[i] = Some(start..end);
        start = end;
    }

    let mut previous_end = 0;
    units
        .iter()
        .enumerate()
        .map(|(i, unit)| {
            let Some(range) = audio[i].clone() else {
                return Clip {
                    audio: previous_end..previous_end,
                    text: None,
                    words: vec![None; unit.words.len()],
                    off_text: Vec::new(),
                };
            };
            previous_end = range.end;
            let (text_start, text_end) = text[i].expect("timed");
            let text = text_start.max(range.start)..text_end.min(range.end).max(text_start);
            let off_text = gaps
                .iter()
                .filter(|g| g.start_ms < range.end && g.end_ms > range.start)
                .filter(|g| g.end_ms <= text.start || g.start_ms >= text.end)
                .map(|g| g.start_ms.max(range.start)..g.end_ms.min(range.end))
                .collect();
            let words = spans[i]
                .iter()
                .map(|w| {
                    w.map(|(s, e)| {
                        let s = s.clamp(range.start, range.end) - range.start;
                        let e = e.clamp(range.start, range.end) - range.start;
                        s..e.max(s)
                    })
                })
                .collect();
            Clip {
                audio: range,
                text: Some(text),
                words,
                off_text,
            }
        })
        .collect()
}

/// Where to cut between a unit whose words end at `a_end` and the next, whose words start at
/// `b_start`.
fn cut(a_end: u64, b_start: u64, gaps: &[crate::AudioGap], loudness: Option<&Loudness>) -> u64 {
    if b_start <= a_end {
        return b_start;
    }
    let window = match gaps
        .iter()
        .find(|g| g.start_ms >= a_end && g.end_ms <= b_start)
    {
        Some(lead) => a_end..lead.start_ms,
        None => a_end..b_start,
    };
    loudness
        .and_then(|l| l.quietest(&window))
        .unwrap_or(window.start + (window.end - window.start) / 2)
}

impl Loudness {
    /// The middle of the quietest whole frame inside `window` (the first, if several).
    pub fn quietest(&self, window: &Range<u64>) -> Option<u64> {
        let frame = u64::from(self.frame_ms.max(1));
        let first = window.start.div_ceil(frame) as usize;
        let last = (window.end / frame) as usize;
        (first..last.min(self.levels.len()))
            .min_by_key(|&f| (self.levels[f], f))
            .map(|f| f as u64 * frame + frame / 2)
    }
}
