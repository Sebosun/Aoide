//! An audiobook as Aoide uses it: its files in order on one timeline, checked for DRM, and read
//! back as mono samples at any rate.
//!
//! The timeline is the files laid end to end. Its times are exact because a file's length is
//! counted from the same packets the reader decodes, so the transcription pass and playback agree
//! on every time. Files are only read, never changed.

use std::ops::Range;
use std::path::{Path, PathBuf};

mod decode;
mod drm;
mod inspect;
mod reader;

pub use inspect::inspect;
pub use reader::Reader;

/// What [`inspect`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct Inspection {
    /// Playable files, in timeline order.
    pub files: Vec<AudioFile>,
    /// Files left out, and why.
    pub problems: Vec<FileProblem>,
    /// Chapters on the timeline: embedded ones, or one per file when there are none.
    pub chapters: Vec<RecordingChapter>,
    /// The album or title tag of the first file.
    pub title: Option<String>,
    /// The album artist or artist tag of the first file.
    pub author: Option<String>,
}

impl Inspection {
    pub fn total_ms(&self) -> u64 {
        self.files.iter().map(AudioFile::duration_us).sum::<u64>() / 1_000
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFile {
    pub path: PathBuf,
    pub size: u64,
    /// Short codec name ("aac", "mp3", "flac", …).
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    /// Playable frames (samples per channel), counted exactly.
    pub frames: u64,
    pub tags: Tags,
}

impl AudioFile {
    pub fn duration_us(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        (u128::from(self.frames) * 1_000_000 / u128::from(self.sample_rate)) as u64
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub track: Option<u64>,
    pub disc: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingChapter {
    pub title: String,
    pub start_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProblem {
    pub path: PathBuf,
    pub problem: Problem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Protected by DRM; Aoide plays only audio that plays in any player.
    Drm(DrmKind),
    /// A format or codec Aoide cannot decode.
    Unsupported(String),
    /// Not readable as audio at all.
    Unreadable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrmKind {
    /// Audible AA, AAX or AAXC.
    Audible,
    /// Apple FairPlay.
    FairPlay,
    /// Encrypted MP4 tracks (Common Encryption).
    Encrypted,
    /// Protected Windows Media.
    WindowsMedia,
}

#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error("cannot read {path}: {message}")]
    Read { path: PathBuf, message: String },
    #[error("{0:?} is outside the recording")]
    OutOfRange(Range<u64>),
}

impl RecordingError {
    fn read(path: &Path, message: impl ToString) -> Self {
        RecordingError::Read {
            path: path.to_owned(),
            message: message.to_string(),
        }
    }
}
