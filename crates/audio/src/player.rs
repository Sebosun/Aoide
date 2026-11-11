use signalsmith_stretch::Stretch;

use crate::{resample, Pcm};

/// One playlist item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    /// False when there is nothing to speak: only the pause plays.
    pub speakable: bool,
    /// Silence after the entry's audio.
    pub pause_after_ms: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Nothing loaded.
    Stopped,
    Paused,
    Playing,
    /// Playing, but the current entry's audio has not been provided yet.
    Buffering,
    /// Reached the end of the playlist.
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub index: usize,
    /// Milliseconds into the entry's audio (stays at the audio length during its pause).
    pub offset_ms: u32,
    pub status: Status,
}

struct Slot {
    entry: Entry,
    audio: Option<Vec<f32>>,
}

/// Device-independent playback state. All audio is held at `output_rate`.
pub struct PlayerCore {
    output_rate: u32,
    slots: Vec<Slot>,
    index: usize,
    /// Samples into the current entry (audio, then pause).
    offset: usize,
    playing: bool,
    speed: f32,
    stretch: Option<Stretch>,
    /// Fractional input samples carried between renders at non-unit speed.
    carry: f64,
}

impl PlayerCore {
    pub fn new(output_rate: u32) -> Self {
        Self {
            output_rate,
            slots: Vec::new(),
            index: 0,
            offset: 0,
            playing: false,
            speed: 1.0,
            stretch: None,
            carry: 0.0,
        }
    }

    pub fn output_rate(&self) -> u32 {
        self.output_rate
    }

    /// Replaces the playlist and pauses at the start of `start`. Previously provided audio is
    /// dropped.
    pub fn load(&mut self, entries: Vec<Entry>, start: usize) {
        self.slots = entries
            .into_iter()
            .map(|entry| Slot { entry, audio: None })
            .collect();
        self.playing = false;
        self.index = start.min(self.slots.len());
        self.offset = 0;
        self.reset_stretch();
    }

    /// Supplies audio for an entry (resampled to the output rate). Ignored for unknown indexes.
    pub fn provide(&mut self, index: usize, pcm: &Pcm) {
        let rate = self.output_rate;
        if let Some(slot) = self.slots.get_mut(index) {
            slot.audio = Some(resample(pcm, rate).samples);
        }
    }

    pub fn has_audio(&self, index: usize) -> bool {
        self.slots.get(index).is_some_and(|s| s.audio.is_some())
    }

    /// Frees audio for entries before `index`.
    pub fn release_before(&mut self, index: usize) {
        for slot in self.slots.iter_mut().take(index) {
            slot.audio = None;
        }
    }

    pub fn play(&mut self) {
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Moves the playhead to the start of `index` (clamped), keeping the play/pause state.
    pub fn seek(&mut self, index: usize) {
        self.index = index.min(self.slots.len().saturating_sub(1));
        self.offset = 0;
        self.reset_stretch();
    }

    /// Playback rate, clamped to 0.5..=3.0; pitch is preserved.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = if speed.is_finite() {
            speed.clamp(0.5, 3.0)
        } else {
            1.0
        };
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn position(&self) -> Position {
        let offset_ms = self.slots.get(self.index).map_or(0, |slot| {
            let audio = slot.audio.as_ref().map_or(0, Vec::len);
            (self.offset.min(audio) as u64 * 1000 / u64::from(self.output_rate)) as u32
        });
        let status = if self.slots.is_empty() {
            Status::Stopped
        } else if self.index >= self.slots.len() {
            Status::Finished
        } else if !self.playing {
            Status::Paused
        } else if self.stalled() {
            Status::Buffering
        } else {
            Status::Playing
        };
        Position {
            index: self.index,
            offset_ms,
            status,
        }
    }

    /// Fills `out` with the next mono samples (silence when not playing or while buffering).
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        if !self.playing || self.slots.is_empty() || self.index >= self.slots.len() {
            return;
        }
        if (self.speed - 1.0).abs() < f32::EPSILON {
            let pulled = self.pull(out.len());
            out[..pulled.len()].copy_from_slice(&pulled);
            return;
        }
        let wanted = out.len() as f64 * f64::from(self.speed) + self.carry;
        let count = wanted.floor() as usize;
        self.carry = wanted - count as f64;
        let mut input = self.pull(count);
        input.resize(count, 0.0);
        let rate = self.output_rate;
        self.stretch
            .get_or_insert_with(|| Stretch::preset_default(1, rate))
            .process(&input, &mut *out);
    }

    fn pause_samples(&self, slot: &Slot) -> usize {
        (u64::from(slot.entry.pause_after_ms) * u64::from(self.output_rate) / 1000) as usize
    }

    fn stalled(&self) -> bool {
        self.slots
            .get(self.index)
            .is_some_and(|slot| slot.entry.speakable && slot.audio.is_none())
    }

    /// Takes up to `count` samples from the playhead onward, stopping early when the next audio
    /// is missing or the playlist ends.
    fn pull(&mut self, count: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(count);
        while out.len() < count && self.index < self.slots.len() && !self.stalled() {
            let slot = &self.slots[self.index];
            let audio = slot.audio.as_deref().unwrap_or(&[]);
            let total = audio.len() + self.pause_samples(slot);
            let take = (count - out.len()).min(total - self.offset);
            if self.offset < audio.len() {
                let end = (self.offset + take).min(audio.len());
                out.extend_from_slice(&audio[self.offset..end]);
                out.resize(out.len() + (self.offset + take - end), 0.0);
            } else {
                out.resize(out.len() + take, 0.0);
            }
            self.offset += take;
            if self.offset >= total {
                self.index += 1;
                self.offset = 0;
            }
        }
        out
    }

    fn reset_stretch(&mut self) {
        if let Some(stretch) = &mut self.stretch {
            stretch.reset();
        }
        self.carry = 0.0;
    }
}
