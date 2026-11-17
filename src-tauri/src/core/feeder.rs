//! Decodes a recording's clips for the player on a thread of its own, so the tick loop never
//! waits for decoding. Clips arrive in pieces of a few seconds, so a long lead-in starts playing
//! at once. A jump drops everything asked for before it.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use aoide_audio::trim::{Cut, TrimOptions, Trimmer};
use aoide_recording::{AudioFile, Reader};

/// Audio decoded at a time.
const CHUNK_MS: u64 = 5_000;

/// A piece of a clip's audio.
#[derive(Debug)]
pub struct Fed {
    pub generation: u64,
    pub index: usize,
    pub samples: Vec<f32>,
    /// The clip's last piece.
    pub last: bool,
    /// Silences shortened in this piece (milliseconds from the clip's start, in the original).
    pub cuts: Vec<Cut>,
}

/// Shorten the clip's long silences, but nothing before `keep_from_ms` (where playback was
/// sought to) into it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trim {
    pub options: TrimOptions,
    pub keep_from_ms: u64,
}

#[derive(Default)]
struct Queue {
    generation: u64,
    pending: VecDeque<(usize, Range<u64>, Option<Trim>)>,
    done: Vec<Fed>,
    stop: bool,
}

pub struct Feeder {
    shared: Arc<(Mutex<Queue>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl Feeder {
    /// Decodes `files` (one timeline) to mono samples at `rate`.
    pub fn spawn(files: Vec<AudioFile>, rate: u32) -> Self {
        let shared = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("aoide-feeder".into())
            .spawn(move || run(worker, Reader::new(files), rate))
            .ok();
        Self { shared, thread }
    }

    /// Asks for the clip `audio` (milliseconds on the timeline) of playlist entry `index`,
    /// trimmed or not.
    pub fn push(&self, index: usize, audio: Range<u64>, trim: Option<Trim>) {
        let (queue, wake) = &*self.shared;
        super::lock(queue).pending.push_back((index, audio, trim));
        wake.notify_one();
    }

    /// Drops what was asked for and not delivered yet. Returns the new generation: pieces of
    /// earlier ones still arriving are to be ignored.
    pub fn reset(&self) -> u64 {
        let mut queue = super::lock(&self.shared.0);
        queue.pending.clear();
        queue.done.clear();
        queue.generation += 1;
        queue.generation
    }

    /// The pieces decoded since the last call.
    pub fn take(&self) -> Vec<Fed> {
        std::mem::take(&mut super::lock(&self.shared.0).done)
    }
}

impl Drop for Feeder {
    fn drop(&mut self) {
        super::lock(&self.shared.0).stop = true;
        self.shared.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(shared: Arc<(Mutex<Queue>, Condvar)>, mut reader: Reader, rate: u32) {
    let (queue, wake) = &*shared;
    // The loudest frame heard so far in the recording, which silence is measured against.
    let mut peak = 0.0;
    loop {
        let (generation, index, audio, trim) = {
            let mut q = super::lock(queue);
            loop {
                if q.stop {
                    return;
                }
                if let Some((index, audio, trim)) = q.pending.pop_front() {
                    break (q.generation, index, audio, trim);
                }
                q = wake.wait(q).unwrap_or_else(|p| p.into_inner());
            }
        };
        let mut trimmer = trim.map(|t| Trimmer::new(rate, t.options, t.keep_from_ms, peak));
        let mut cut = 0;
        let mut from = audio.start;
        loop {
            let to = (from + CHUNK_MS).min(audio.end);
            let mut samples = reader.read(from..to, rate).unwrap_or_else(|error| {
                tracing::warn!(event = "feeder.read", %error, "Cannot read the recording");
                Vec::new()
            });
            let last = to >= audio.end;
            let mut cuts = Vec::new();
            if let Some(trimmer) = trimmer.as_mut() {
                samples = trimmer.push(&samples);
                if last {
                    samples.extend(trimmer.finish());
                }
                cuts = trimmer.cuts()[cut..].to_vec();
                cut = trimmer.cuts().len();
                peak = trimmer.peak();
            }
            let mut q = super::lock(queue);
            if q.stop || q.generation != generation {
                break;
            }
            q.done.push(Fed {
                generation,
                index,
                samples,
                last,
                cuts,
            });
            if last {
                break;
            }
            from = to;
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub const RATE: u32 = 8_000;

    /// A mono 16-bit WAV of `samples` at [`RATE`].
    pub fn wav(path: &std::path::Path, samples: &[f32]) -> AudioFile {
        let data: Vec<u8> = samples
            .iter()
            .flat_map(|s| ((s * 32767.0) as i16).to_le_bytes())
            .collect();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        for field in [
            16u32.to_le_bytes().to_vec(),
            1u16.to_le_bytes().to_vec(),
            1u16.to_le_bytes().to_vec(),
        ] {
            bytes.extend(field);
        }
        bytes.extend_from_slice(&RATE.to_le_bytes());
        bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
        bytes.extend(data);
        std::fs::write(path, &bytes).unwrap();
        AudioFile {
            path: path.into(),
            size: bytes.len() as u64,
            codec: "pcm_s16le".into(),
            sample_rate: RATE,
            channels: 1,
            frames: samples.len() as u64,
            tags: Default::default(),
        }
    }

    /// The clip `audio` of the file, fed whole.
    fn fed(feeder: &Feeder, audio: Range<u64>, trim: Option<Trim>) -> (Vec<f32>, Vec<Cut>) {
        feeder.push(0, audio, trim);
        let (mut samples, mut cuts) = (Vec::new(), Vec::new());
        for _ in 0..500 {
            for piece in feeder.take() {
                samples.extend(piece.samples);
                cuts.extend(piece.cuts);
                if piece.last {
                    return (samples, cuts);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("never fed");
    }

    #[test]
    fn a_clip_is_fed_with_its_long_silences_shortened_when_asked() {
        let dir = tempfile::tempdir().unwrap();
        let second = RATE as usize;
        let tone = |n: usize| (0..n).map(|i| ((i as f32) * 0.3).sin() * 0.5);
        let samples: Vec<f32> = tone(second)
            .chain(std::iter::repeat(0.0).take(2 * second))
            .chain(tone(second))
            .collect();
        let file = wav(&dir.path().join("a.wav"), &samples);
        let feeder = Feeder::spawn(vec![file], RATE);
        let (whole, cuts) = fed(&feeder, 0..4_000, None);
        assert_eq!((whole.len(), cuts.len()), (4 * second, 0));
        let trim = Trim {
            options: TrimOptions::default(),
            keep_from_ms: 0,
        };
        let (shortened, cuts) = fed(&feeder, 0..4_000, Some(trim));
        assert_eq!(shortened.len(), 2 * second + second / 2);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].removed_ms, 1_500);
        // Sought to 2.8 s, in the silence: before it nothing is cut, and what is left stays.
        let (kept, cuts) = fed(
            &feeder,
            0..4_000,
            Some(Trim {
                keep_from_ms: 2_800,
                ..trim
            }),
        );
        assert_eq!((kept.len(), cuts.len()), (4 * second, 0));
    }
}
