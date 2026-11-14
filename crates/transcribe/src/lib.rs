//! Speech recognition for syncing recordings: words with their times.
//!
//! A [`Transcriber`] turns 16 kHz mono samples into [`Word`]s. whisper.cpp is built in behind the
//! `whisper` feature; the rest (turning tokens into words, choosing where to cut long audio into
//! blocks) is plain code that tests run without it.

use std::ops::Range;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[cfg(feature = "whisper")]
mod whisper;
#[cfg(feature = "whisper")]
pub use whisper::{WhisperCpp, WhisperOptions};
pub mod models;
mod server;
pub use aoide_tts::NetworkScope;
pub use server::{ServerOptions, ServerTranscriber};

/// The sample rate transcribers take.
pub const SAMPLE_RATE: u32 = 16_000;

/// A recognised word. Times are milliseconds from the start of the request's samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub probability: f32,
}

pub struct Request<'a> {
    /// Mono samples at [`SAMPLE_RATE`].
    pub samples: &'a [f32],
    /// ISO 639-1 language code ("en"); `None` lets the model guess.
    pub language: Option<&'a str>,
    /// Set to stop a request early.
    pub abort: Option<Arc<AtomicBool>>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot load the speech recognition model: {0}")]
    Model(String),
    #[error("stopped")]
    Aborted,
    #[error("speech recognition failed: {0}")]
    Failed(String),
}

pub trait Transcriber: Send + Sync {
    /// Which engine and model, for the record kept with each transcribed block.
    fn id(&self) -> &str;
    fn transcribe(&self, request: &Request<'_>) -> Result<Vec<Word>, Error>;
}

/// One recognised token with its times in centiseconds, as whisper.cpp reports them.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenTimes {
    /// Raw bytes; a multi-byte character may be split across tokens.
    pub bytes: Vec<u8>,
    pub t0: i64,
    pub t1: i64,
    /// Start from dynamic time warping, or negative when not computed.
    pub t_dtw: i64,
    pub probability: f32,
    /// Timestamp and control tokens.
    pub special: bool,
}

/// Joins tokens into words (a word starts at a token with a leading space). A word starts at its
/// first token's DTW time (else its `t0`) and ends at its last token's `t1`, but never after the
/// next word starts; times never go backwards.
pub fn words_from_tokens(tokens: &[TokenTimes]) -> Vec<Word> {
    struct Pending {
        bytes: Vec<u8>,
        start: i64,
        end: i64,
        probability: f32,
        tokens: u32,
    }
    let mut pending: Vec<Pending> = Vec::new();
    for token in tokens.iter().filter(|t| !t.special) {
        match pending.last_mut() {
            Some(word) if !token.bytes.starts_with(b" ") => {
                word.bytes.extend_from_slice(&token.bytes);
                word.end = word.end.max(token.t1);
                word.probability += token.probability;
                word.tokens += 1;
            }
            _ => pending.push(Pending {
                bytes: token.bytes.clone(),
                start: if token.t_dtw >= 0 {
                    token.t_dtw
                } else {
                    token.t0
                },
                end: token.t1,
                probability: token.probability,
                tokens: 1,
            }),
        }
    }
    let mut words: Vec<Word> = pending
        .into_iter()
        .filter_map(|p| {
            let text = String::from_utf8_lossy(&p.bytes).trim().to_owned();
            (!text.is_empty()).then(|| Word {
                text,
                start_ms: p.start.max(0) as u64 * 10,
                end_ms: p.end.max(0) as u64 * 10,
                probability: p.probability / p.tokens as f32,
            })
        })
        .collect();
    for i in 1..words.len() {
        words[i].start_ms = words[i].start_ms.max(words[i - 1].start_ms);
    }
    for i in 0..words.len() {
        let next = words.get(i + 1).map_or(u64::MAX, |w| w.start_ms);
        let word = &mut words[i];
        word.end_ms = word.end_ms.min(next).max(word.start_ms);
    }
    words
}

/// The sample index, within `window`, at the middle of the quietest 100 ms: where to cut a long
/// recording into blocks so that no word straddles the cut.
pub fn quietest_cut(samples: &[f32], rate: u32, window: Range<usize>) -> usize {
    let window = window.start.min(samples.len())..window.end.min(samples.len());
    let frame = (rate as usize / 100).max(1);
    let span = 10;
    let energy: Vec<f64> = samples[window.clone()]
        .chunks(frame)
        .map(|c| c.iter().map(|s| f64::from(*s) * f64::from(*s)).sum())
        .collect();
    if energy.len() <= span {
        return window.start + window.len() / 2;
    }
    let mut sum: f64 = energy[..span].iter().sum();
    let (mut best, mut best_at) = (sum, 0);
    for i in span..energy.len() {
        sum += energy[i] - energy[i - span];
        if sum < best {
            best = sum;
            best_at = i + 1 - span;
        }
    }
    window.start + (best_at * frame + span * frame / 2).min(window.len().saturating_sub(1))
}
