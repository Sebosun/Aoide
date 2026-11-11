//! Turns parsed chapter blocks into TTS segments with stable ids and character ranges.
//!
//! All offsets are Unicode scalar (`char`) offsets into the chapter display text, which is the
//! blocks' texts joined with `"\n\n"` (see `aoide_epub::Chapter::text`). Everything is pure and
//! deterministic: the same blocks and options always produce the same segments.

use std::ops::Range;

pub use aoide_epub::{Block, BlockKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentOptions {
    /// Upper bound for a segment's display length; longer sentences are split at clause or
    /// word boundaries.
    pub max_chars: usize,
    /// Sentences shorter than this are grouped with a neighbour in the same block when the
    /// result still fits `max_chars`.
    pub min_chars: usize,
    /// Silence to insert after a heading segment.
    pub heading_pause_ms: u32,
}

impl Default for SegmentOptions {
    fn default() -> Self {
        Self {
            max_chars: 250,
            min_chars: 40,
            heading_pause_ms: 700,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// `"{chapter}:{block}:{index within block}"`.
    pub id: String,
    pub block: usize,
    /// Range of the segment in the chapter display text.
    pub range: Range<usize>,
    /// Exactly the chapter text in `range`.
    pub display: String,
    /// Text to send to a TTS engine. Empty means "nothing to speak": play `pause_after_ms` only.
    pub speech: String,
    /// Chapter-text ranges of each sentence in the segment.
    pub sentences: Vec<Range<usize>>,
    /// Chapter-text ranges of each whitespace-separated word in the segment.
    pub words: Vec<Range<usize>>,
    pub pause_after_ms: u32,
}

/// Estimated `[start_ms, end_ms)` for each word of a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordEstimate {
    pub start_ms: u32,
    pub end_ms: u32,
}

/// Splits a chapter's blocks into speakable segments, in reading order.
pub fn segment_chapter(chapter: u32, blocks: &[Block], options: &SegmentOptions) -> Vec<Segment> {
    let offsets = block_offsets(blocks);
    let mut segments = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let chars: Vec<char> = block.text.chars().collect();
        let units: Vec<Range<usize>> = sentences(&chars)
            .into_iter()
            .flat_map(|sentence| split_long(&chars, sentence, options.max_chars))
            .collect();
        let heading = matches!(block.kind, BlockKind::Heading { .. });
        for (n, group) in group(&units, options).into_iter().enumerate() {
            let range = group[0].start..group[group.len() - 1].end;
            let display: String = chars[range.clone()].iter().collect();
            let base = offsets[index];
            let shift = |r: &Range<usize>| base + r.start..base + r.end;
            let mut speech = speech_text(&display);
            if heading && !speech.is_empty() && !speech.ends_with(['.', '!', '?', '…', ':', ';'])
            {
                speech.push('.');
            }
            segments.push(Segment {
                id: format!("{chapter}:{index}:{n}"),
                block: index,
                range: shift(&range),
                sentences: group.iter().map(shift).collect(),
                words: words(&chars, &range).iter().map(shift).collect(),
                display,
                speech,
                pause_after_ms: if heading { options.heading_pause_ms } else { 0 },
            });
        }
    }
    segments
}

/// Chapter-text start offset of each block.
pub fn block_offsets(blocks: &[Block]) -> Vec<usize> {
    let mut offset = 0;
    blocks
        .iter()
        .map(|block| {
            let start = offset;
            offset += block.text.chars().count() + 2;
            start
        })
        .collect()
}

/// Index of the segment containing `char_offset`, or the first segment that starts after it
/// (e.g. an offset inside the blank line between blocks). `None` when past the last segment.
pub fn segment_at(segments: &[Segment], char_offset: usize) -> Option<usize> {
    let index = segments.partition_point(|s| s.range.end <= char_offset);
    (index < segments.len()).then_some(index)
}

/// Spreads `duration_ms` across the segment's words in proportion to their spoken weight.
pub fn estimate_word_timings(segment: &Segment, duration_ms: u32) -> Vec<WordEstimate> {
    let weights = word_weights(segment);
    let total: u64 = weights.iter().sum::<u64>().max(1);
    let mut cumulative = 0;
    let mut start = 0;
    weights
        .iter()
        .map(|weight| {
            cumulative += weight;
            let end = ((duration_ms as u64 * cumulative + total / 2) / total) as u32;
            let estimate = WordEstimate {
                start_ms: start,
                end_ms: end,
            };
            start = end;
            estimate
        })
        .collect()
}

/// Shortest silence treated as a pause between words.
const MIN_PAUSE_MS: u32 = 120;
const FRAME_MS: u32 = 10;

/// Word timings fitted to the audio: leading and trailing silence is excluded, and each clear
/// pause is matched to the nearest word boundary (preferring boundaries after punctuation), so
/// words on either side of it end and start exactly at the pause. Between pauses, time is shared
/// as in [`estimate_word_timings`]. Falls back to the plain estimate when the audio is silent.
pub fn align_to_pauses(segment: &Segment, samples: &[f32], sample_rate: u32) -> Vec<WordEstimate> {
    let duration_ms = if sample_rate == 0 {
        0
    } else {
        (samples.len() as u64 * 1000 / u64::from(sample_rate)) as u32
    };
    let fallback = || estimate_word_timings(segment, duration_ms);
    if segment.words.is_empty() || sample_rate == 0 || samples.is_empty() {
        return fallback();
    }
    let frame = (sample_rate * FRAME_MS / 1000).max(1) as usize;
    let energy: Vec<f32> = samples
        .chunks(frame)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect();
    let peak = energy.iter().copied().fold(0.0, f32::max);
    if peak < 1e-3 {
        return fallback();
    }
    let voiced: Vec<bool> = energy.iter().map(|e| *e > peak * 0.08).collect();
    let (Some(first), Some(last)) = (
        voiced.iter().position(|v| *v),
        voiced.iter().rposition(|v| *v),
    ) else {
        return fallback();
    };
    let start = first as u32 * FRAME_MS;
    let end = ((last as u32 + 1) * FRAME_MS).min(duration_ms.max(start + 1));
    // Inner pauses as (start_ms, end_ms).
    let mut pauses = Vec::new();
    let mut run = None;
    for (i, v) in voiced.iter().enumerate().take(last + 1).skip(first) {
        match (v, run) {
            (false, None) => run = Some(i),
            (true, Some(r)) => {
                if (i - r) as u32 * FRAME_MS >= MIN_PAUSE_MS {
                    pauses.push((r as u32 * FRAME_MS, i as u32 * FRAME_MS));
                }
                run = None;
            }
            _ => {}
        }
    }
    let weights = word_weights(segment);
    let n = weights.len();
    let total: u64 = weights.iter().sum::<u64>().max(1);
    let span = u64::from(end - start);
    let mut cumulative = 0;
    let boundaries: Vec<u32> = weights[..n - 1]
        .iter()
        .map(|w| {
            cumulative += w;
            start + (span * cumulative / total) as u32
        })
        .collect();
    let chars: Vec<char> = segment.display.chars().collect();
    let punctuated: Vec<bool> = segment.words[..n - 1]
        .iter()
        .map(|r| {
            matches!(
                chars.get(r.end - 1 - segment.range.start),
                Some(',' | ';' | ':' | '.' | '!' | '?' | '…' | '—' | '–' | ')' | '"' | '”')
            )
        })
        .collect();
    let tolerance = (span as u32 * 35 / 100).max(400);
    // Anchors: (boundary index k between word k and k+1, pause start, pause end).
    let mut anchors: Vec<(usize, u32, u32)> = Vec::new();
    for (pause_start, pause_end) in pauses {
        let centre = (pause_start + pause_end) / 2;
        let from = anchors.last().map_or(0, |a| a.0 + 1);
        let best = (from..n - 1)
            .filter(|k| boundaries[*k].abs_diff(centre) <= tolerance)
            .min_by_key(|k| {
                let distance = u64::from(boundaries[*k].abs_diff(centre));
                if punctuated[*k] {
                    distance
                } else {
                    distance * 2 + 1
                }
            });
        if let Some(k) = best {
            anchors.push((k, pause_start, pause_end));
        }
    }
    let mut result = Vec::with_capacity(n);
    let mut word = 0;
    let mut t0 = start;
    for (k, pause_start, pause_end) in anchors.into_iter().chain([(n - 1, end, end)]) {
        distribute(&weights[word..=k], t0, pause_start.max(t0), &mut result);
        word = k + 1;
        t0 = pause_end;
    }
    result
}

/// Combines aligner output with fallback timings of the same words. Aligned spans are used as
/// given; unplaced words keep their fallback span, clamped between the aligned neighbours. The
/// result never moves backwards.
pub fn merge_alignment(
    aligned: &[Option<(u32, u32)>],
    fallback: &[WordEstimate],
) -> Vec<WordEstimate> {
    let mut merged: Vec<WordEstimate> = Vec::with_capacity(fallback.len());
    for (i, estimate) in fallback.iter().enumerate() {
        let floor = merged.last().map_or(0, |p| p.end_ms.max(p.start_ms));
        let span = match aligned.get(i).copied().flatten() {
            Some((start, end)) => WordEstimate {
                start_ms: start,
                end_ms: end,
            },
            None => {
                let ceiling = aligned[i + 1..]
                    .iter()
                    .flatten()
                    .next()
                    .map_or(u32::MAX, |(start, _)| *start);
                WordEstimate {
                    start_ms: estimate.start_ms.clamp(floor, ceiling.max(floor)),
                    end_ms: estimate.end_ms.clamp(floor, ceiling.max(floor)),
                }
            }
        };
        let previous_start = merged.last().map_or(0, |p| p.start_ms);
        let start_ms = span.start_ms.max(previous_start);
        merged.push(WordEstimate {
            start_ms,
            end_ms: span.end_ms.max(start_ms),
        });
    }
    merged
}

/// Spoken weight of each word: letters plus a little for trailing punctuation pauses.
fn word_weights(segment: &Segment) -> Vec<u64> {
    let chars: Vec<char> = segment.display.chars().collect();
    segment
        .words
        .iter()
        .map(|range| {
            let word = &chars[range.start - segment.range.start..range.end - segment.range.start];
            let letters = word.iter().filter(|c| c.is_alphanumeric()).count().max(1) as u64;
            let pause = match word.last() {
                Some(',' | ';' | ':' | '—' | '–') => 2,
                Some('.' | '!' | '?' | '…') => 3,
                _ => 0,
            };
            letters + pause
        })
        .collect()
}

fn distribute(weights: &[u64], from: u32, to: u32, out: &mut Vec<WordEstimate>) {
    let total: u64 = weights.iter().sum::<u64>().max(1);
    let span = u64::from(to - from);
    let mut cumulative = 0;
    let mut start = from;
    for weight in weights {
        cumulative += weight;
        let end = from + ((span * cumulative + total / 2) / total) as u32;
        out.push(WordEstimate {
            start_ms: start,
            end_ms: end.max(start),
        });
        start = end.max(start);
    }
}

const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "st", "prof", "sr", "jr", "vs", "etc", "e.g", "i.e", "cf", "mt",
    "gen", "col", "capt", "lt", "rev", "hon", "sgt", "eds", "approx", "ca", "viz",
];

/// Abbreviations that only continue the sentence when a number follows ("No. 5", "pp. 12").
const NUMBERING: &[&str] = &["no", "nos", "vol", "vols", "ch", "fig", "pp", "art", "sec"];

const TERMINATORS: &[char] = &['.', '!', '?', '…'];
const CLOSERS: &[char] = &['"', '\'', '”', '’', ')', ']', '»', '›'];

/// Sentence ranges within a block, without surrounding whitespace.
fn sentences(chars: &[char]) -> Vec<Range<usize>> {
    let mut result = Vec::new();
    let mut start = skip_space(chars, 0);
    let mut i = start;
    while i < chars.len() {
        if !TERMINATORS.contains(&chars[i]) {
            i += 1;
            continue;
        }
        let terminator = i;
        let mut end = i + 1;
        while end < chars.len()
            && (TERMINATORS.contains(&chars[end]) || CLOSERS.contains(&chars[end]))
        {
            end += 1;
        }
        end = note_markers(chars, end);
        let next = skip_space(chars, end);
        let boundary = next == chars.len()
            || (next > end
                && starts_sentence(chars, next)
                && !abbreviation(chars, start, terminator, next));
        if boundary {
            result.push(start..end);
            start = next;
        }
        i = end.max(i + 1);
    }
    if start < chars.len() {
        let end = trim_end(chars, start, chars.len());
        result.push(start..end);
    }
    result
}

/// Extends `end` over directly attached `[digits]` note markers.
fn note_markers(chars: &[char], mut end: usize) -> usize {
    while end < chars.len() && chars[end] == '[' {
        let digits = chars[end + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if digits > 0 && chars.get(end + 1 + digits) == Some(&']') {
            end += digits + 2;
        } else {
            break;
        }
    }
    end
}

fn starts_sentence(chars: &[char], at: usize) -> bool {
    chars[at..]
        .iter()
        .find(|c| !matches!(c, '"' | '\'' | '“' | '‘' | '(' | '[' | '«' | '‹'))
        .is_some_and(|c| c.is_uppercase() || c.is_numeric() || !c.is_alphabetic())
}

/// Whether the word ending at the terminator (a '.') is an abbreviation or an initial.
fn abbreviation(chars: &[char], sentence_start: usize, terminator: usize, next: usize) -> bool {
    if chars[terminator] != '.' {
        return false;
    }
    let word_start = chars[sentence_start..terminator]
        .iter()
        .rposition(|c| c.is_whitespace() || matches!(c, '(' | '"' | '“'))
        .map_or(sentence_start, |p| sentence_start + p + 1);
    let word: String = chars[word_start..terminator]
        .iter()
        .collect::<String>()
        .to_lowercase();
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let numbered =
        NUMBERING.contains(&word.as_str()) && chars.get(next).is_some_and(|c| c.is_ascii_digit());
    numbered
        || (letters == 1 && word.chars().all(char::is_alphabetic))
        || ABBREVIATIONS.contains(&word.as_str())
}

fn skip_space(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn trim_end(chars: &[char], start: usize, mut end: usize) -> usize {
    while end > start && chars[end - 1].is_whitespace() {
        end -= 1;
    }
    end
}

/// Splits a sentence longer than `max` at clause punctuation, else at word boundaries.
fn split_long(chars: &[char], sentence: Range<usize>, max: usize) -> Vec<Range<usize>> {
    let max = max.max(1);
    let mut pieces = Vec::new();
    let mut start = sentence.start;
    while sentence.end - start > max {
        let limit = start + max;
        let fits = |i: &usize| *i > start && *i <= limit;
        let clause = (start..limit)
            .filter(|&i| {
                matches!(chars[i], ',' | ';' | ':' | '—' | '–')
                    && chars.get(i + 1).is_some_and(|c| c.is_whitespace())
            })
            .map(|i| i + 1)
            .rfind(fits);
        let space = (start + 1..=limit).rfind(|&i| chars[i].is_whitespace());
        let cut = clause.or(space).unwrap_or(limit);
        pieces.push(start..trim_end(chars, start, cut));
        start = skip_space(chars, cut);
    }
    if start < sentence.end {
        pieces.push(start..sentence.end);
    }
    pieces
}

/// Groups consecutive units when either side is shorter than `min_chars` and the result fits.
fn group(units: &[Range<usize>], options: &SegmentOptions) -> Vec<Vec<Range<usize>>> {
    let mut groups: Vec<Vec<Range<usize>>> = Vec::new();
    for unit in units {
        if let Some(current) = groups.last_mut() {
            let start = current[0].start;
            let current_len = current[current.len() - 1].end - start;
            let fits = unit.end - start <= options.max_chars;
            if fits && (current_len < options.min_chars || unit.len() < options.min_chars) {
                current.push(unit.clone());
                continue;
            }
        }
        groups.push(vec![unit.clone()]);
    }
    groups
}

fn words(chars: &[char], range: &Range<usize>) -> Vec<Range<usize>> {
    let mut result = Vec::new();
    let mut start = None;
    for i in range.clone() {
        match (chars[i].is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                result.push(s..i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        result.push(s..range.end);
    }
    result
}

/// Display text minus `[digits]` note markers; empty when nothing speakable remains.
fn speech_text(display: &str) -> String {
    let chars: Vec<char> = display.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' {
            let skipped = note_markers(&chars, i);
            if skipped > i {
                i = skipped;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.chars().any(char::is_alphanumeric) {
        out
    } else {
        String::new()
    }
}
