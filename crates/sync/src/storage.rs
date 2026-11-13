//! A compact binary form of a sync map's word times, for storing in the library.

use crate::{ChapterSync, TimedWord, Timing};

const MAGIC: &[u8; 4] = b"AOS1";

/// Encodes the chapters' word times. Character offsets and times must fit in 32 bits.
pub fn encode_chapters(chapters: &[ChapterSync]) -> Vec<u8> {
    let words: usize = chapters.iter().map(|c| c.words.len()).sum();
    let mut out = Vec::with_capacity(8 + chapters.len() * 4 + words * 17);
    out.extend_from_slice(MAGIC);
    put(&mut out, chapters.len() as u64);
    for chapter in chapters {
        put(&mut out, chapter.words.len() as u64);
        for word in &chapter.words {
            put(&mut out, word.range.start as u64);
            put(&mut out, word.range.end as u64);
            let (kind, start, end) = match word.timing {
                Timing::Heard { start_ms, end_ms } => (0, start_ms, end_ms),
                Timing::Estimated { start_ms, end_ms } => (1, start_ms, end_ms),
                Timing::Unread => (2, 0, 0),
            };
            out.push(kind);
            put(&mut out, start);
            put(&mut out, end);
        }
    }
    out
}

/// Reverses [`encode_chapters`]; `None` for anything it did not produce.
pub fn decode_chapters(bytes: &[u8]) -> Option<Vec<ChapterSync>> {
    let mut input = bytes.strip_prefix(MAGIC)?;
    let count = take(&mut input)? as usize;
    let mut chapters = Vec::with_capacity(count.min(input.len() / 4));
    for _ in 0..count {
        let words = take(&mut input)? as usize;
        let mut chapter = Vec::with_capacity(words.min(input.len() / 17));
        for _ in 0..words {
            let range = take(&mut input)? as usize..take(&mut input)? as usize;
            let (&kind, rest) = input.split_first()?;
            input = rest;
            let (start_ms, end_ms) = (take(&mut input)?, take(&mut input)?);
            let timing = match kind {
                0 => Timing::Heard { start_ms, end_ms },
                1 => Timing::Estimated { start_ms, end_ms },
                2 => Timing::Unread,
                _ => return None,
            };
            chapter.push(TimedWord { range, timing });
        }
        chapters.push(ChapterSync { words: chapter });
    }
    input.is_empty().then_some(chapters)
}

fn put(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&(value.min(u64::from(u32::MAX)) as u32).to_le_bytes());
}

fn take(input: &mut &[u8]) -> Option<u64> {
    let (head, rest) = input.split_first_chunk::<4>()?;
    *input = rest;
    Some(u64::from(u32::from_le_bytes(*head)))
}
