//! Decodes a recording's clips for the player on a thread of its own, so the tick loop never
//! waits for decoding. Clips arrive in pieces of a few seconds, so a long lead-in starts playing
//! at once. A jump drops everything asked for before it.

use std::collections::VecDeque;
use std::ops::Range;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

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
}

#[derive(Default)]
struct Queue {
    generation: u64,
    pending: VecDeque<(usize, Range<u64>)>,
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

    /// Asks for the clip `audio` (milliseconds on the timeline) of playlist entry `index`.
    pub fn push(&self, index: usize, audio: Range<u64>) {
        let (queue, wake) = &*self.shared;
        super::lock(queue).pending.push_back((index, audio));
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
    loop {
        let (generation, index, audio) = {
            let mut q = super::lock(queue);
            loop {
                if q.stop {
                    return;
                }
                if let Some((index, audio)) = q.pending.pop_front() {
                    break (q.generation, index, audio);
                }
                q = wake.wait(q).unwrap_or_else(|p| p.into_inner());
            }
        };
        let mut from = audio.start;
        loop {
            let to = (from + CHUNK_MS).min(audio.end);
            let samples = reader.read(from..to, rate).unwrap_or_else(|error| {
                eprintln!("aoide: cannot read the recording: {error}");
                Vec::new()
            });
            let last = to >= audio.end;
            let mut q = super::lock(queue);
            if q.stop || q.generation != generation {
                break;
            }
            q.done.push(Fed {
                generation,
                index,
                samples,
                last,
            });
            if last {
                break;
            }
            from = to;
        }
    }
}
