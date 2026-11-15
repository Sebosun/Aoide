//! The types Core's commands answer with and its events carry, and the events themselves. The
//! window's contract (`src/api/contract.ts`) is generated from these (core/contract.rs).

use super::*;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "BookProgress"))]
pub struct BookProgressView {
    pub chapter: u32,
    pub segment_id: Option<String>,
    pub fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub status: BookStatus,
    pub cover_path: Option<String>,
    pub progress: Option<BookProgressView>,
    pub audio_bytes: u64,
    /// What plays if the user listens.
    pub audio: super::facets::AudioSource,
    /// Whole-book preparation status, if queued, running or finished.
    pub prepare: Option<PrepareView>,
    pub added_at: i64,
    pub last_opened_at: Option<i64>,
    /// Ids of the collections the book is on.
    pub collections: Vec<i64>,
    /// The sync stage of the book's recording, if it has one.
    pub recording: Option<syncing::Stage>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookView {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub chapters: Vec<ChapterSummary>,
    pub warnings: Vec<String>,
    pub excluded: Vec<ExcludedView>,
    pub language_warning: Option<String>,
    /// The book's voice is not offered by the active engine, so another one reads it.
    pub voice_warning: Option<String>,
    pub progress: Option<BookProgressView>,
    /// Words read aloud in the whole book, for time estimates.
    pub word_count: usize,
}

/// What the player is doing, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum PlaybackStatus {
    Stopped,
    Buffering,
    Playing,
    Paused,
}

/// The speech engine's state, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum EngineState {
    Offline,
    Loading,
    Ready,
    /// Making audio.
    Busy,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "PlayerState"))]
pub struct PlayerStateView {
    pub book_id: Option<String>,
    pub status: PlaybackStatus,
    pub chapter: usize,
    pub segment_id: Option<String>,
    pub word_index: Option<usize>,
    pub speed: f32,
    /// Output gain, 0.0..=1.0.
    pub volume: f32,
    pub chapter_fraction: f64,
    pub error: Option<String>,
    pub sleep: Option<SleepView>,
    /// What is being heard: a speech engine or the book's recording.
    pub source: Narration,
    /// The recording is playing narration that is not in the text (credits, a dedication…).
    pub off_text: bool,
    /// Listening time left in the chapter at this speed, when it is known exactly (a
    /// recording); otherwise the interface estimates it from the words left.
    pub chapter_left_ms: Option<u64>,
    /// The recording is being listened to while it syncs, and the sync has not reached the
    /// playhead yet: playback waits for it.
    pub waiting_for_sync: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EngineStatusView {
    pub profile_name: String,
    pub state: EngineState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "GenerationState"))]
pub struct GenerationStateView {
    pub book_id: Option<String>,
    pub generating_segment_id: Option<String>,
    pub buffered_ahead_ms: u32,
    pub pending: usize,
    pub error: Option<String>,
    /// Progress of the book currently being prepared, if any.
    pub preparing: Option<PreparingView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PreparingView {
    pub book_id: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ImportResult {
    pub path: String,
    pub book_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AppInfo {
    pub library_folder: String,
}

/// How much of one chapter's audio is made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterAudioView {
    pub done: usize,
    pub total: usize,
}

/// Per-book overrides of the global engine profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BookSettings {
    pub voice_id: Option<String>,
    pub params: aoide_tts::Params,
    /// BCP-47 language to send instead of the book's declared language.
    pub language: Option<String>,
    /// Speak footnotes after the paragraph that references them.
    pub read_notes: bool,
    /// Which narration plays; `None` until chosen (a speech engine).
    pub narration: Option<recording::Narration>,
    /// While the recording plays, the speech engine speaks the text it lacks.
    pub speak_missing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct EngineOptions {
    pub descriptors: Vec<aoide_tts::EngineDescriptor>,
    pub profile: EngineProfile,
}

/// Everything Core tells the UI: one variant per event, with its payload. The UI's map from
/// event name to payload type is generated from this (core/contract.rs).
#[derive(Debug, Clone)]
pub enum CoreEvent {
    LibraryChanged,
    ConfigChanged(PreferencesView),
    PlayerState(PlayerStateView),
    EngineStatus(EngineStatusView),
    GenerationState(GenerationStateView),
    ScanProgress(discovery::ScanProgress),
    /// Online details arrived for this found book.
    DiscoveryDetails(String),
    DiscoveryImport(discovery::ImportProgress),
    ExportProgress(export::ExportProgress),
    /// A book's recording changed stage or made progress.
    SyncState(recording::RecordingView),
    ModelDownload(recording::ModelDownloadView),
    /// Where a search for audiobooks is.
    AudiobookScan(discovery::ScanProgress),
    /// The audiobook matches changed.
    AudiobookMatches(Vec<audiobooks::AudiobookMatch>),
    /// A failure in the background the user should know about.
    Problem(error::ProblemView),
}

impl CoreEvent {
    pub const LIBRARY_CHANGED: &'static str = "library://changed";
    pub const CONFIG_CHANGED: &'static str = "config://changed";
    pub const PLAYER_STATE: &'static str = "player://state";
    pub const ENGINE_STATUS: &'static str = "engine://status";
    pub const GENERATION_STATE: &'static str = "generation://state";
    pub const SCAN_PROGRESS: &'static str = "discovery://progress";
    pub const DISCOVERY_DETAILS: &'static str = "discovery://details";
    pub const DISCOVERY_IMPORT: &'static str = "discovery://import";
    pub const EXPORT_PROGRESS: &'static str = "export://progress";
    pub const SYNC_STATE: &'static str = "sync://state";
    pub const MODEL_DOWNLOAD: &'static str = "model://download";
    pub const AUDIOBOOK_SCAN: &'static str = "audiobooks://progress";
    pub const AUDIOBOOK_MATCHES: &'static str = "audiobooks://matches";
    pub const PROBLEM: &'static str = "core://problem";

    pub fn name(&self) -> &'static str {
        match self {
            Self::LibraryChanged => Self::LIBRARY_CHANGED,
            Self::ConfigChanged(_) => Self::CONFIG_CHANGED,
            Self::PlayerState(_) => Self::PLAYER_STATE,
            Self::EngineStatus(_) => Self::ENGINE_STATUS,
            Self::GenerationState(_) => Self::GENERATION_STATE,
            Self::ScanProgress(_) => Self::SCAN_PROGRESS,
            Self::DiscoveryDetails(_) => Self::DISCOVERY_DETAILS,
            Self::DiscoveryImport(_) => Self::DISCOVERY_IMPORT,
            Self::ExportProgress(_) => Self::EXPORT_PROGRESS,
            Self::SyncState(_) => Self::SYNC_STATE,
            Self::ModelDownload(_) => Self::MODEL_DOWNLOAD,
            Self::AudiobookScan(_) => Self::AUDIOBOOK_SCAN,
            Self::AudiobookMatches(_) => Self::AUDIOBOOK_MATCHES,
            Self::Problem(_) => Self::PROBLEM,
        }
    }

    pub(super) fn payload(&self) -> serde_json::Result<serde_json::Value> {
        match self {
            Self::LibraryChanged => Ok(serde_json::Value::Null),
            Self::ConfigChanged(p) => serde_json::to_value(p),
            Self::PlayerState(p) => serde_json::to_value(p),
            Self::EngineStatus(p) => serde_json::to_value(p),
            Self::GenerationState(p) => serde_json::to_value(p),
            Self::ScanProgress(p) => serde_json::to_value(p),
            Self::DiscoveryDetails(p) => serde_json::to_value(p),
            Self::DiscoveryImport(p) => serde_json::to_value(p),
            Self::ExportProgress(p) => serde_json::to_value(p),
            Self::SyncState(p) => serde_json::to_value(p),
            Self::ModelDownload(p) => serde_json::to_value(p),
            Self::AudiobookScan(p) => serde_json::to_value(p),
            Self::AudiobookMatches(p) => serde_json::to_value(p),
            Self::Problem(p) => serde_json::to_value(p),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SleepView {
    /// Milliseconds until playback pauses, when timed.
    pub remaining_ms: Option<u64>,
    pub end_of_chapter: bool,
}
