//! Shortening long silences in audio that arrives in pieces (a recording, as it is decoded).
//!
//! Silence is 10 ms frames whose loudness (RMS) is under a share of the loudest frame heard so
//! far, and under a floor. A silent run longer than `keep_ms` keeps its first and last halves,
//! joined by a short cross-fade; what was taken out is a [`Cut`], so times can be mapped between
//! the recording and what is played ([`to_trimmed`], [`to_original`]). Only the silent run being
//! read is held back.

use std::collections::VecDeque;

/// How silences are shortened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimOptions {
    /// What a long silence is shortened to.
    pub keep_ms: u32,
    /// A frame quieter than the loudest heard by this many decibels is silent...
    pub below_peak_db: f32,
    /// ...and so is one quieter than this, whatever was heard (an amplitude, 1.0 = full scale).
    pub floor: f32,
    /// The cross-fade where a silence is cut.
    pub fade_ms: u32,
}

impl Default for TrimOptions {
    fn default() -> Self {
        Self {
            keep_ms: 500,
            below_peak_db: 35.0,
            floor: 0.002,
            fade_ms: 5,
        }
    }
}

/// Audio taken out: `removed_ms` from `at_ms` on, in the original's time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cut {
    pub at_ms: u64,
    pub removed_ms: u64,
}

const FRAME_MS: u32 = 10;

/// Shortens the long silences of one stretch of audio, pushed in pieces.
pub struct Trimmer {
    rate: u32,
    frame: usize,
    keep_head: usize,
    keep_tail: usize,
    fade: usize,
    ratio: f32,
    floor: f32,
    /// Samples before this (from the stretch's start) are never cut.
    keep_from: u64,
    peak: f32,
    /// Samples read so far, in the original.
    read: u64,
    /// The part of a frame not complete yet.
    partial: Vec<f32>,
    /// The silent run being read: where it started, how long it is, its first and last samples.
    run_start: u64,
    run_len: u64,
    head: Vec<f32>,
    tail: VecDeque<f32>,
    cuts: Vec<Cut>,
}

impl Trimmer {
    /// A stretch at `rate` samples a second. Nothing before `keep_from_ms` is cut (playback
    /// sought to there, so the times before it are as they were). `peak` is the loudest frame
    /// heard earlier in the recording (0 at its start); see [`Trimmer::peak`].
    pub fn new(rate: u32, options: TrimOptions, keep_from_ms: u64, peak: f32) -> Self {
        let samples = |ms: u32| (u64::from(ms) * u64::from(rate) / 1000) as usize;
        let keep = samples(options.keep_ms);
        let fade = samples(options.fade_ms).min(keep / 2);
        let keep_head = (keep + fade) / 2;
        Self {
            rate,
            frame: samples(FRAME_MS).max(1),
            keep_head,
            keep_tail: keep + fade - keep_head,
            fade,
            ratio: 10f32.powf(-options.below_peak_db / 20.0),
            floor: options.floor,
            keep_from: keep_from_ms * u64::from(rate) / 1000,
            peak,
            read: 0,
            partial: Vec::new(),
            run_start: 0,
            run_len: 0,
            head: Vec::new(),
            tail: VecDeque::new(),
            cuts: Vec::new(),
        }
    }

    /// The loudest frame heard so far, to hand to the next stretch's trimmer.
    pub fn peak(&self) -> f32 {
        self.peak
    }

    /// The cuts made so far, in order.
    pub fn cuts(&self) -> &[Cut] {
        &self.cuts
    }

    /// Takes in the next samples; returns those ready to be played.
    pub fn push(&mut self, samples: &[f32]) -> Vec<f32> {
        let mut out = Vec::with_capacity(samples.len());
        let mut rest = samples;
        while !rest.is_empty() {
            let take = (self.frame - self.partial.len()).min(rest.len());
            self.partial.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.partial.len() == self.frame {
                let frame = std::mem::take(&mut self.partial);
                self.frame_in(&frame, &mut out);
            }
        }
        out
    }

    /// The stretch ended: returns what was held back.
    pub fn finish(&mut self) -> Vec<f32> {
        let mut out = Vec::new();
        self.end_run(&mut out);
        self.read += self.partial.len() as u64;
        out.append(&mut self.partial);
        out
    }

    fn frame_in(&mut self, frame: &[f32], out: &mut Vec<f32>) {
        let loudness = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
        self.peak = self.peak.max(loudness);
        let start = self.read;
        self.read += frame.len() as u64;
        let silent = loudness < self.floor.max(self.peak * self.ratio);
        if !silent || start < self.keep_from {
            self.end_run(out);
            out.extend_from_slice(frame);
            return;
        }
        if self.run_len == 0 {
            self.run_start = start;
        }
        self.run_len += frame.len() as u64;
        for &sample in frame {
            if self.head.len() < self.keep_head {
                self.head.push(sample);
            } else {
                self.tail.push_back(sample);
                if self.tail.len() > self.keep_tail {
                    self.tail.pop_front();
                }
            }
        }
    }

    /// The silent run ended: it is played, shortened if it was long.
    fn end_run(&mut self, out: &mut Vec<f32>) {
        if self.run_len == 0 {
            return;
        }
        let head = std::mem::take(&mut self.head);
        let tail: Vec<f32> = std::mem::take(&mut self.tail).into();
        let kept = (head.len() + tail.len()) as u64;
        if self.run_len <= kept {
            // Short: every sample is still here.
            out.extend(head);
            out.extend(tail);
        } else {
            let fade = self.fade;
            let joined = head.len() - fade;
            out.extend_from_slice(&head[..joined]);
            for i in 0..fade {
                let t = (i + 1) as f32 / (fade + 1) as f32;
                out.push(head[joined + i] * (1.0 - t) + tail[i] * t);
            }
            out.extend_from_slice(&tail[fade..]);
            let ms = |samples: u64| samples * 1000 / u64::from(self.rate);
            let emitted = kept - fade as u64;
            self.cuts.push(Cut {
                at_ms: ms(self.run_start + joined as u64),
                removed_ms: ms(self.run_len - emitted),
            });
        }
        self.run_len = 0;
    }
}

/// Original time `ms` as played, once `cuts` (in order) were taken out. A time inside what was
/// taken out plays where it was cut.
pub fn to_trimmed(cuts: &[Cut], ms: u64) -> u64 {
    let mut removed = 0;
    for cut in cuts {
        if ms < cut.at_ms {
            break;
        }
        if ms < cut.at_ms + cut.removed_ms {
            return cut.at_ms - removed;
        }
        removed += cut.removed_ms;
    }
    ms - removed
}

/// Time `ms` as played, in the original (the inverse of [`to_trimmed`] outside the cuts).
pub fn to_original(cuts: &[Cut], ms: u64) -> u64 {
    let mut removed = 0;
    for cut in cuts {
        if ms < cut.at_ms - removed {
            break;
        }
        removed += cut.removed_ms;
    }
    ms + removed
}
