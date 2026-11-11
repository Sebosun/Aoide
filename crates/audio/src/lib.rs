//! Aoide audio: Ogg Opus segment files and a gapless, pitch-preserving segment player.
//!
//! [`PlayerCore`] is device-independent: it renders mono `f32` samples at a fixed output rate
//! from a playlist of segments, tracks the playhead, and applies speed changes with
//! time-stretching. An output backend (see [`output`]) pulls samples from it in real time.

pub mod codec;
pub mod output;
mod player;

pub use player::{Entry, PlayerCore, Position, Status};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AudioError {
    #[error("audio encoding failed: {0}")]
    Encode(String),
    #[error("audio decoding failed: {0}")]
    Decode(String),
    #[error("audio output failed: {0}")]
    Output(String),
}

/// Mono PCM samples.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

impl Pcm {
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        self.samples.len() as u64 * 1000 / u64::from(self.sample_rate)
    }
}

/// Linear-interpolation resampling (adequate for speech playback).
pub fn resample(pcm: &Pcm, rate: u32) -> Pcm {
    if pcm.sample_rate == rate || pcm.sample_rate == 0 || pcm.samples.is_empty() {
        return Pcm {
            sample_rate: rate,
            samples: pcm.samples.clone(),
        };
    }
    let input = &pcm.samples;
    let ratio = f64::from(pcm.sample_rate) / f64::from(rate);
    let len = (input.len() as f64 / ratio).round() as usize;
    let last = input.len() - 1;
    let samples = (0..len)
        .map(|i| {
            let position = i as f64 * ratio;
            let index = (position.floor() as usize).min(last);
            let fraction = (position - index as f64) as f32;
            let next = input[(index + 1).min(last)];
            input[index] * (1.0 - fraction) + next * fraction
        })
        .collect();
    Pcm {
        sample_rate: rate,
        samples,
    }
}
