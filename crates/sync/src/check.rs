//! The quick check: is a short stretch of the recording from this book at all?

use std::collections::HashSet;

use crate::{tokenize, HeardWord};

/// The share of the heard word triples (0.0 to 1.0) that also occur in the text. A few seconds
/// read from the book score high even with recognition errors; other narration scores near 0.
pub fn found_in_text(chapters: &[&str], heard: &[HeardWord]) -> f32 {
    let spoken: Vec<String> = heard
        .iter()
        .flat_map(|w| tokenize(&w.text))
        .map(|t| t.norm)
        .collect();
    if spoken.len() < 3 {
        return 0.0;
    }
    let mut triples = HashSet::new();
    for chapter in chapters {
        let words: Vec<String> = tokenize(chapter).into_iter().map(|t| t.norm).collect();
        for w in words.windows(3) {
            triples.insert((w[0].clone(), w[1].clone(), w[2].clone()));
        }
    }
    let found = spoken
        .windows(3)
        .filter(|w| triples.contains(&(w[0].clone(), w[1].clone(), w[2].clone())))
        .count();
    found as f32 / (spoken.len() - 2) as f32
}
