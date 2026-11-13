use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use symphonia::core::codecs::audio::AudioDecoder;
use symphonia::core::errors::Error;
use symphonia::core::formats::{SeekMode, SeekTo};
use symphonia::core::units::Timestamp;

use crate::decode::Source;
use crate::{AudioFile, Problem, RecordingError};

/// Zero crossings of the resampling kernel on each side.
const ZERO_CROSSINGS: f64 = 10.0;
/// Kernel table entries per input sample.
const TABLE_STEPS: f64 = 512.0;
/// Decoded audio kept behind the read position, in seconds.
const KEEP_BEHIND_S: u64 = 2;
/// Jumps further ahead than this seek instead of decoding through, in seconds.
const SEEK_AHEAD_S: u64 = 20;
/// Decoded before a seek target, so decoders with overlapping frames settle.
const PREROLL_MS: u64 = 300;

/// Reads mono samples from anywhere on the timeline. Reading on from where the last read ended
/// never seeks; a jump seeks once. Output sample `n` at rate `r` is always the recording at time
/// `n / r`, so reads of adjacent ranges join without a gap and give the same samples as one read.
pub struct Reader {
    files: Vec<AudioFile>,
    /// Where each file starts on the timeline, in microseconds.
    starts_us: Vec<u64>,
    total_us: u64,
    open: Option<Open>,
    kernels: HashMap<(u32, u32), Arc<Kernel>>,
}

impl Reader {
    pub fn new(files: Vec<AudioFile>) -> Self {
        let mut starts_us = Vec::with_capacity(files.len());
        let mut total_us = 0;
        for file in &files {
            starts_us.push(total_us);
            total_us += file.duration_us();
        }
        Self {
            files,
            starts_us,
            total_us,
            open: None,
            kernels: HashMap::new(),
        }
    }

    pub fn total_ms(&self) -> u64 {
        self.total_us / 1_000
    }

    /// Mono samples for `range` (milliseconds) at `rate`: exactly
    /// `range.end * rate / 1000 - range.start * rate / 1000` of them.
    pub fn read(&mut self, range: Range<u64>, rate: u32) -> Result<Vec<f32>, RecordingError> {
        if range.start > range.end || range.end.saturating_mul(1_000) > self.total_us || rate == 0 {
            return Err(RecordingError::OutOfRange(range));
        }
        let first = range.start * u64::from(rate) / 1_000;
        let last = range.end * u64::from(rate) / 1_000;
        let mut out = Vec::with_capacity((last - first) as usize);
        for n in first..last {
            let t_us = n as f64 * 1e6 / f64::from(rate);
            let index = self
                .starts_us
                .partition_point(|&s| s as f64 <= t_us)
                .saturating_sub(1);
            let file_rate = self.files[index].sample_rate;
            let x = (t_us - self.starts_us[index] as f64) * f64::from(file_rate) / 1e6;
            let open = self.file(index)?;
            if file_rate == rate {
                let k = x.round() as i64;
                open.cover(k, k)?;
                out.push(open.get(k));
                continue;
            }
            let kernel = self.kernel(file_rate, rate);
            let open = self.file(index)?;
            let lo = (x - kernel.half_width).ceil() as i64;
            let hi = (x + kernel.half_width).floor() as i64;
            open.cover(lo, hi)?;
            let mut sum = 0.0f64;
            for k in lo..=hi {
                sum += f64::from(open.get(k)) * kernel.at(x - k as f64);
            }
            out.push(sum as f32);
        }
        Ok(out)
    }

    fn file(&mut self, index: usize) -> Result<&mut Open, RecordingError> {
        if self.open.as_ref().is_none_or(|o| o.index != index) {
            self.open = Some(Open::new(index, &self.files[index])?);
        }
        Ok(self.open.as_mut().expect("just opened"))
    }

    fn kernel(&mut self, from: u32, to: u32) -> Arc<Kernel> {
        Arc::clone(
            self.kernels
                .entry((from, to))
                .or_insert_with(|| Arc::new(Kernel::new(from, to))),
        )
    }
}

/// A windowed-sinc low-pass kernel for resampling `from` Hz to `to` Hz, tabulated in input
/// samples.
struct Kernel {
    half_width: f64,
    table: Vec<f64>,
}

impl Kernel {
    fn new(from: u32, to: u32) -> Self {
        // Cut off a little below the lower Nyquist frequency, relative to the input rate.
        let r = 0.9 * f64::from(from.min(to)) / f64::from(from);
        let half_width = ZERO_CROSSINGS / r;
        let len = (half_width * TABLE_STEPS).ceil() as usize + 2;
        let table = (0..len)
            .map(|i| {
                let t = i as f64 / TABLE_STEPS;
                if t > half_width {
                    return 0.0;
                }
                let u = t / half_width;
                let window = 0.42
                    + 0.5 * (std::f64::consts::PI * u).cos()
                    + 0.08 * (2.0 * std::f64::consts::PI * u).cos();
                let arg = std::f64::consts::PI * r * t;
                let sinc = if arg == 0.0 { 1.0 } else { arg.sin() / arg };
                r * sinc * window
            })
            .collect();
        Kernel { half_width, table }
    }

    fn at(&self, offset: f64) -> f64 {
        let position = offset.abs() * TABLE_STEPS;
        let i = position.floor() as usize;
        if i + 1 >= self.table.len() {
            return 0.0;
        }
        let fraction = position - i as f64;
        self.table[i] * (1.0 - fraction) + self.table[i + 1] * fraction
    }
}

/// The file being decoded, with a window of its mono frames.
struct Open {
    index: usize,
    path: std::path::PathBuf,
    source: Source,
    decoder: Box<dyn AudioDecoder>,
    rate: u32,
    frames: u64,
    buffer: Vec<f32>,
    /// Frame index of `buffer[0]`; unknown until the first packet after opening or seeking.
    start: Option<u64>,
    ended: bool,
    /// A seek just happened (a seek that lands after its target falls back to the start).
    seeked: bool,
    /// Seeks to frames from `.0` on fail; `.1` is the latest frame found to seek.
    unseekable: Option<(u64, u64)>,
    planes: Vec<Vec<f32>>,
}

impl Open {
    fn new(index: usize, file: &AudioFile) -> Result<Self, RecordingError> {
        let problem = |p: Problem| RecordingError::read(&file.path, format!("{p:?}"));
        let source = Source::open(&file.path).map_err(problem)?;
        let decoder = source.decoder().map_err(problem)?;
        Ok(Open {
            index,
            path: file.path.clone(),
            source,
            decoder,
            rate: file.sample_rate,
            frames: file.frames,
            buffer: Vec::new(),
            start: None,
            ended: false,
            seeked: false,
            unseekable: None,
            planes: Vec::new(),
        })
    }

    fn get(&self, k: i64) -> f32 {
        let Some(start) = self.start else { return 0.0 };
        if k < start as i64 {
            return 0.0;
        }
        self.buffer
            .get((k - start as i64) as usize)
            .copied()
            .unwrap_or(0.0)
    }

    /// Makes frames `lo..=hi` (clamped to the file) available, decoding forward or seeking.
    fn cover(&mut self, lo: i64, hi: i64) -> Result<(), RecordingError> {
        let lo = lo.max(0) as u64;
        let hi = hi.min(self.frames as i64 - 1);
        if hi < 0 || lo > hi as u64 {
            return Ok(());
        }
        let hi = hi as u64;
        let rate = u64::from(self.rate);
        loop {
            match self.start {
                None => {
                    if !self.decode_next()? {
                        return Ok(());
                    }
                }
                Some(start) if lo < start => {
                    if self.seeked {
                        self.restart()?;
                    } else {
                        self.seek(lo)?;
                    }
                }
                Some(start) => {
                    self.seeked = false;
                    let end = start + self.buffer.len() as u64;
                    if hi < end || self.ended {
                        self.trim(lo);
                        return Ok(());
                    }
                    if lo > end + SEEK_AHEAD_S * rate && self.landing(lo) > end {
                        self.seek(lo)?;
                    } else {
                        self.trim(lo);
                        self.decode_next()?;
                    }
                }
            }
        }
    }

    /// Decodes one packet of the track onto the buffer. False at the end of the file.
    fn decode_next(&mut self) -> Result<bool, RecordingError> {
        loop {
            let packet = match self.source.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) | Err(Error::ResetRequired) => {
                    self.ended = true;
                    if self.start.is_none() {
                        self.start = Some(self.frames);
                    }
                    return Ok(false);
                }
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    self.ended = true;
                    if self.start.is_none() {
                        self.start = Some(self.frames);
                    }
                    return Ok(false);
                }
                Err(e) => return Err(RecordingError::read(&self.path, e)),
            };
            if packet.track_id != self.source.track_id {
                continue;
            }
            let duration = self
                .source
                .frames(packet.dur.get() as i64, self.rate)
                .max(0) as usize;
            if self.start.is_none() {
                let pts = self.source.frames(packet.pts.get(), self.rate).max(0) as u64;
                self.start = Some(pts);
            }
            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    let n = decoded.frames();
                    decoded.copy_to_vecs_planar::<f32>(&mut self.planes);
                    let channels = self.planes.len().max(1);
                    let trim = self
                        .source
                        .frames(packet.trim_start.get() as i64, self.rate)
                        .max(0) as usize;
                    let from = if n > duration {
                        trim.min(n - duration)
                    } else {
                        0
                    };
                    let take = duration.min(n - from);
                    let planes = &self.planes;
                    self.buffer.extend((from..from + take).map(|i| {
                        planes
                            .iter()
                            .map(|p| p.get(i).copied().unwrap_or(0.0))
                            .sum::<f32>()
                            / channels as f32
                    }));
                    self.buffer
                        .resize(self.buffer.len() + (duration - take), 0.0);
                }
                Err(Error::DecodeError(_)) => {
                    // Keep the timeline: a damaged packet plays as silence.
                    self.buffer.resize(self.buffer.len() + duration, 0.0);
                }
                Err(e) => return Err(RecordingError::read(&self.path, e)),
            }
            return Ok(true);
        }
    }

    /// Where a seek for `frame` lands: before it by the pre-roll, or at the latest point that
    /// seeks when the file cannot be sought that far.
    fn landing(&self, frame: u64) -> u64 {
        let target = frame.saturating_sub(PREROLL_MS * u64::from(self.rate) / 1_000);
        match self.unseekable {
            Some((fails_from, reachable)) if target >= fails_from => reachable,
            _ => target,
        }
    }

    /// Seeks to just before `frame`. Where seeking fails (symphonia will not seek an M4B past its
    /// last chapter's start), lands at the latest point that seeks, found once by bisection, and
    /// leaves the rest to decoding forward.
    fn seek(&mut self, frame: u64) -> Result<(), RecordingError> {
        let target = self.landing(frame);
        if target == 0 {
            return self.restart();
        }
        if self.seek_exactly(target) {
            return Ok(());
        }
        let (mut reachable, mut fails_from) = (0, target);
        while fails_from - reachable > u64::from(self.rate) {
            let middle = reachable + (fails_from - reachable) / 2;
            if self.seek_exactly(middle) {
                reachable = middle;
            } else {
                fails_from = middle;
            }
        }
        self.unseekable = Some((fails_from, reachable));
        if reachable > 0 && self.seek_exactly(reachable) {
            Ok(())
        } else {
            self.restart()
        }
    }

    fn seek_exactly(&mut self, frame: u64) -> bool {
        let to = SeekTo::Timestamp {
            ts: Timestamp::new(self.source.timestamp(frame as i64, self.rate)),
            track_id: self.source.track_id,
        };
        if self.source.format.seek(SeekMode::Accurate, to).is_err() {
            return false;
        }
        self.decoder.reset();
        self.buffer.clear();
        self.start = None;
        self.ended = false;
        self.seeked = true;
        true
    }

    /// Drops decoded frames more than [`KEEP_BEHIND_S`] before `lo`.
    fn trim(&mut self, lo: u64) {
        let Some(start) = self.start else { return };
        let keep = KEEP_BEHIND_S * u64::from(self.rate);
        let keep_from = lo.saturating_sub(keep);
        if keep_from > start + keep {
            let drop = ((keep_from - start) as usize).min(self.buffer.len());
            self.buffer.drain(..drop);
            self.start = Some(start + drop as u64);
        }
    }

    /// Decodes from the start of the file again.
    fn restart(&mut self) -> Result<(), RecordingError> {
        let problem = |p: Problem| RecordingError::read(&self.path, format!("{p:?}"));
        self.source = Source::open(&self.path).map_err(problem)?;
        self.decoder = self.source.decoder().map_err(problem)?;
        self.buffer.clear();
        self.start = Some(0);
        self.ended = false;
        self.seeked = false;
        Ok(())
    }
}
