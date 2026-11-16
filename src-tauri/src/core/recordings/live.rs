//! What is kept in memory about each recording besides its stored row: its stage and progress,
//! the queue of sync work, and the audio decoded ahead of transcription.

use super::*;

/// What Core keeps in memory about each recording besides its stored row.
pub(super) struct Live {
    pub(super) stage: SyncStage,
    pub(super) added_at: i64,
    pub(super) total_ms: u64,
    pub(super) transcribed_ms: u64,
    /// When transcription (re)started in this run, and how far it was then: the pace.
    pub(super) pace_from: Option<(Instant, u64)>,
    /// Set to stop transcription mid-block.
    pub(super) abort: Arc<AtomicBool>,
    /// The quick check passed in this run.
    pub(super) sample_checked: bool,
    /// Changes with its stage and transcript (see [`next_change`]), so what is worked out from
    /// them is known to be current.
    pub(super) changes: u64,
}

impl Live {
    /// Synced, or syncing with part of it transcribed: what is synced can be listened to.
    pub(super) fn listenable(&self) -> bool {
        crate::core::syncing::can_listen(self.stage, self.transcribed_ms)
    }
}

#[derive(Default)]
pub(super) struct SyncState {
    pub(super) live: HashMap<String, Live>,
    /// Wakes the sync thread.
    pub(super) wake: Option<std::sync::mpsc::Sender<()>>,
    /// The loaded transcriber, and what it was loaded with.
    pub(super) transcriber: Option<(TranscriberChoice, Arc<dyn Transcriber>)>,
    /// The recording being transcribed, with the audio decoded ahead of it.
    pub(super) reader: Option<(String, Decoded)>,
    /// Models being downloaded.
    pub(super) downloading: HashSet<String>,
}

/// A recording's reader and the 16 kHz audio decoded so far, from `from_ms` on. While one block
/// is being recognised the next one is decoded beside it, so recognition does not wait on it.
pub(super) struct Decoded {
    pub(super) reader: Reader,
    pub(super) from_ms: u64,
    pub(super) samples: Vec<f32>,
}

pub(super) const PER_MS: usize = SAMPLE_RATE as usize / 1_000;

impl Decoded {
    pub(super) fn new(reader: Reader) -> Self {
        Self {
            reader,
            from_ms: 0,
            samples: Vec::new(),
        }
    }

    pub(super) fn to_ms(&self) -> u64 {
        self.from_ms + (self.samples.len() / PER_MS) as u64
    }

    /// Holds `range`: what is before it is dropped and what is missing is read.
    pub(super) fn cover(&mut self, range: Range<u64>) -> Result<(), String> {
        if range.start < self.from_ms || range.start > self.to_ms() {
            self.from_ms = range.start;
            self.samples.clear();
        } else {
            self.samples
                .drain(..(range.start - self.from_ms) as usize * PER_MS);
            self.from_ms = range.start;
        }
        if self.to_ms() < range.end {
            let more = self
                .reader
                .read(self.to_ms()..range.end, SAMPLE_RATE)
                .map_err(|e| e.to_string())?;
            self.samples.extend(more);
        }
        Ok(())
    }

    /// `range`, which must be covered.
    pub(super) fn samples(&self, range: Range<u64>) -> &[f32] {
        let at = |ms: u64| (ms - self.from_ms) as usize * PER_MS;
        &self.samples[at(range.start)..at(range.end)]
    }
}

pub(super) enum Work {
    Check(String),
    Match(String),
    Transcribe(String),
    Promote(String),
}

impl SyncState {
    pub(super) fn next_work(&self) -> Option<Work> {
        let mut by_age: Vec<(&String, &Live)> = self.live.iter().collect();
        by_age.sort_by_key(|(id, live)| (live.added_at, *id));
        let find = |stage| {
            by_age
                .iter()
                .find(|(_, live)| live.stage == stage)
                .map(|(id, _)| (*id).clone())
        };
        find(SyncStage::Checking)
            .map(Work::Check)
            .or_else(|| find(SyncStage::Matching).map(Work::Match))
            .or_else(|| find(SyncStage::Transcribing).map(Work::Transcribe))
            .or_else(|| find(SyncStage::Waiting).map(Work::Promote))
    }

    /// Place in line of a waiting or transcribing recording.
    pub(super) fn position(&self, id: &str) -> Option<usize> {
        let mut line: Vec<(&String, &Live)> = self
            .live
            .iter()
            .filter(|(_, l)| matches!(l.stage, SyncStage::Transcribing | SyncStage::Waiting))
            .collect();
        line.sort_by_key(|(id, live)| (live.stage != SyncStage::Transcribing, live.added_at, *id));
        line.iter().position(|(other, _)| other.as_str() == id)
    }
}

/// A number no change to any recording has had before, in this run.
pub(super) fn next_change() -> u64 {
    static CHANGES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    CHANGES.fetch_add(1, Ordering::Relaxed) + 1
}

/// What a recording laid out for playback depends on: the book's entries and its sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Playable {
    pub(super) book: String,
    pub(super) entries: usize,
    pub(super) changes: u64,
}
