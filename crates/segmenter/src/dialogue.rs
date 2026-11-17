//! What characters say: the spans of a paragraph inside quotation marks.

use std::ops::Range;

/// Who says a segment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    #[default]
    Narration,
    /// Inside quotation marks.
    Dialogue,
}

/// A quoted span shorter than this, in a sentence with narration, stays with the narration:
/// switching voice for "yes" sounds chopped.
pub const SHORT_SPAN: usize = 25;

/// The quoted spans of a paragraph (character ranges, quote marks included): `“ ”`, `" "`,
/// `« »`, `„ “`, and `‘ ’` where the paragraph has no double quote (a `’` before a letter is an
/// apostrophe). A span left open runs to the paragraph's end; a long speech over several
/// paragraphs opens each with a quote mark, so each is found.
pub fn quoted_spans(chars: &[char]) -> Vec<Range<usize>> {
    let doubles = chars
        .iter()
        .any(|c| matches!(c, '“' | '”' | '"' | '«' | '»' | '„'));
    let mut spans = Vec::new();
    let mut open: Option<(usize, char)> = None;
    for (i, &c) in chars.iter().enumerate() {
        match open {
            None => {
                let closer = match c {
                    '“' => Some('”'),
                    '"' => Some('"'),
                    '«' => Some('»'),
                    '„' => Some('“'),
                    '‘' if !doubles && !letter_before(chars, i) => Some('’'),
                    _ => None,
                };
                if let Some(closer) = closer {
                    open = Some((i, closer));
                }
            }
            Some((start, closer)) => {
                let closes = c == closer && (c != '’' || !letter_after(chars, i));
                if closes {
                    spans.push(start..i + 1);
                    open = None;
                }
            }
        }
    }
    if let Some((start, _)) = open {
        let end = chars.len() - chars.iter().rev().take_while(|c| c.is_whitespace()).count();
        if end > start + 1 {
            spans.push(start..end);
        }
    }
    spans
}

fn letter_before(chars: &[char], i: usize) -> bool {
    i > 0 && chars[i - 1].is_alphanumeric()
}

fn letter_after(chars: &[char], i: usize) -> bool {
    chars.get(i + 1).is_some_and(|c| c.is_alphanumeric())
}

/// `unit` (a sentence, as a character range) cut where `spans` start and end, each piece with
/// its role: pieces without a letter join the one before, a short quote next to narration
/// joins it, and neighbours of one role are one piece again.
pub(crate) fn pieces(
    chars: &[char],
    unit: Range<usize>,
    spans: &[Range<usize>],
) -> Vec<(Range<usize>, Role)> {
    let mut cuts = vec![unit.start, unit.end];
    for span in spans {
        for at in [span.start, span.end] {
            if at > unit.start && at < unit.end {
                cuts.push(at);
            }
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    let role_at = |at: usize| {
        if spans.iter().any(|s| s.contains(&at)) {
            Role::Dialogue
        } else {
            Role::Narration
        }
    };
    let mut pieces: Vec<(Range<usize>, Role)> = Vec::new();
    for pair in cuts.windows(2) {
        let (mut start, mut end) = (pair[0], pair[1]);
        while start < end && chars[start].is_whitespace() {
            start += 1;
        }
        while end > start && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        if start == end {
            continue;
        }
        let role = role_at(start);
        if !chars[start..end].iter().any(|c| c.is_alphanumeric()) {
            if let Some(last) = pieces.last_mut() {
                last.0.end = end;
                continue;
            }
        }
        pieces.push((start..end, role));
    }
    let narrated = pieces.iter().any(|(_, role)| *role == Role::Narration);
    if narrated {
        for (range, role) in &mut pieces {
            if *role == Role::Dialogue && range.len() < SHORT_SPAN {
                *role = Role::Narration;
            }
        }
    }
    let mut merged: Vec<(Range<usize>, Role)> = Vec::new();
    for (range, role) in pieces {
        match merged.last_mut() {
            Some(last) if last.1 == role => last.0.end = range.end,
            _ => merged.push((range, role)),
        }
    }
    merged
}
