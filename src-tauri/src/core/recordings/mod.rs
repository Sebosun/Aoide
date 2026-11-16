//! The sync job: recordings paired with books, inspected and attached, then transcribed block by
//! block and matched word by word to the text; and the speech-recognition models it uses.
//!
//! Each recording's stage changes only through [`syncing::next`]. The job runs one step at a
//! time ([`Recordings::sync_step`]) on a thread of its own, so a pause takes effect between
//! blocks, or at once through the transcriber's abort flag. What it needs from the rest of the
//! app comes through [`SyncHost`].

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use aoide_library::{RecordingRow, StoredSyncMap, TranscriptBlock};
use aoide_recording::{AudioFile, DrmKind, Problem, Reader, Tags};
use aoide_sync::{
    clips, decode_chapters, encode_chapters, found_in_text, match_book, HeardWord, Loudness,
    Report, SyncMap, Unit,
};
use aoide_transcribe::models::{self, lateness_ms, ModelInfo, ModelSize};
use aoide_transcribe::{quietest_cut, Request, Transcriber, SAMPLE_RATE};
use serde::Deserialize;

mod attach;
mod layout;
mod live;
mod speech_models;
mod steps;

use super::recording::{
    ChapterMatchView, DownloadStage, MatchSummaryView, ModelDownloadView, ModelSizeChoice,
    ModelView, OffTextView, RecognitionDevice, RecognitionEngine, RecognitionSettings,
    RecordingFileView, RecordingPlan, RecordingProblemView, TranscriberChoice, TranscriberFactory,
};
use super::syncing::{self, Event as SyncEvent, Found, Stage as SyncStage};
use super::*;
use live::*;

/// Audio transcribed at a time.
const BLOCK_MS: u64 = 300_000;
/// How far around a block's end its cut may move to land in a pause.
const CUT_SLACK_MS: u64 = 30_000;
/// Loudness frames, and the grid block cuts fall on so frames line up across blocks.
const FRAME_MS: u32 = 10;
/// The quick check's samples: their length and where they are taken.
const SAMPLE_MS: u64 = 20_000;
const SAMPLE_AT: [f64; 3] = [0.2, 0.5, 0.8];
/// A sample counts as found when this share of its word triples is in the text.
const SAMPLE_FOUND: f32 = 0.3;
/// While a recording syncs, only what ends this long before the sync's edge is played.
const SYNC_MARGIN_MS: u64 = 30_000;
/// Below this share of the text matched, a sync needs the user's review.
const POOR_BELOW: f64 = 0.75;
/// Changes when matching changes, so stored maps are made again.
const MATCHER_VERSION: &str = "1";
// ---------------------------------------------------------------- stored details

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StoredRecording {
    files: Vec<StoredFile>,
    chapters: usize,
    title: Option<String>,
    author: Option<String>,
    /// Its parts, for playing it on its own: the embedded chapters, else one per file.
    #[serde(default)]
    parts: Vec<StoredPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StoredPart {
    title: String,
    start_ms: u64,
}

/// `found`'s parts: its embedded chapters, else its files (titled by their tags, else their
/// names) when there are several, else none.
fn parts_of(found: &aoide_recording::Inspection) -> Vec<StoredPart> {
    if !found.chapters.is_empty() {
        return found
            .chapters
            .iter()
            .map(|c| StoredPart {
                title: c.title.clone(),
                start_ms: c.start_ms,
            })
            .collect();
    }
    if found.files.len() < 2 {
        return Vec::new();
    }
    let mut at = 0;
    found
        .files
        .iter()
        .map(|file| {
            let part = StoredPart {
                title: file
                    .tags
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| aoide_discovery::title_from_name(&file.path)),
                start_ms: at,
            };
            at += file.duration_us() / 1_000;
            part
        })
        .collect()
}

/// What an audiobook on its own plays: its files, its length, and its parts' titles and starts.
pub(super) struct TimelineSource {
    pub files: Vec<AudioFile>,
    pub total_ms: u64,
    pub parts: Vec<(String, u64)>,
}

/// A file as it was when attached; a different size or time means it changed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct StoredFile {
    path: PathBuf,
    size: u64,
    modified_ms: i64,
    codec: String,
    sample_rate: u32,
    channels: u32,
    frames: u64,
}

impl StoredFile {
    fn audio(&self) -> AudioFile {
        AudioFile {
            path: self.path.clone(),
            size: self.size,
            codec: self.codec.clone(),
            sample_rate: self.sample_rate,
            channels: self.channels,
            frames: self.frames,
            tags: Tags::default(),
        }
    }

    fn view(&self) -> RecordingFileView {
        file_view(&self.audio())
    }

    fn unchanged(&self) -> bool {
        std::fs::metadata(&self.path)
            .is_ok_and(|m| m.len() == self.size && modified_ms(&m) == self.modified_ms)
    }
}

fn modified_ms(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

fn file_view(file: &AudioFile) -> RecordingFileView {
    RecordingFileView {
        path: file.path.to_string_lossy().into_owned(),
        name: file_name(&file.path),
        duration_ms: file.duration_us() / 1_000,
        codec: file.codec.clone(),
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.to_string_lossy(), |n| n.to_string_lossy())
        .into_owned()
}

fn problem_text(problem: &Problem) -> String {
    match problem {
        Problem::Drm(kind) => format!(
            "Protected by {} DRM. Aoide plays only audio files that play in any player.",
            match kind {
                DrmKind::Audible => "Audible",
                DrmKind::FairPlay => "Apple FairPlay",
                DrmKind::Encrypted => "encryption",
                DrmKind::WindowsMedia => "Windows Media",
            }
        ),
        Problem::Unsupported(what) => format!("Not a format Aoide can play ({what})."),
        Problem::Unreadable(what) => format!("Could not be read as audio ({what})."),
    }
}

/// Identifies the text a map was made for, with the matcher version.
fn text_hash(book: &BookModel) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    MATCHER_VERSION.hash(&mut hasher);
    for chapter in &book.chapters {
        chapter.text.hash(&mut hasher);
    }
    format!("{MATCHER_VERSION}:{:016x}", hasher.finish())
}

fn poor(report: &Report) -> bool {
    (report.matched_words as f64) < POOR_BELOW * report.words.max(1) as f64
}

fn summary(report: &Report) -> MatchSummaryView {
    MatchSummaryView {
        words: report.words,
        matched_words: report.matched_words,
        timed_words: report.timed_words,
    }
}

fn whisper_language(language: Option<&str>) -> String {
    language
        .and_then(|l| l.split(['-', '_']).next())
        .filter(|l| !l.is_empty())
        .map_or_else(|| "en".into(), str::to_ascii_lowercase)
}

// ---------------------------------------------------------------- the job

/// What the sync job needs from the rest of the app.
pub(super) trait SyncHost: Sync {
    fn emit(&self, event: CoreEvent);
    fn load_book(&self, id: &str) -> Result<Arc<BookModel>, String>;
    fn recognition(&self) -> RecognitionSettings;
    /// The narration chosen for book `id`, if one was.
    fn narration(&self, id: &str) -> Option<Narration>;
    /// More of book `id`'s recording is synced; `ready` once all of it is.
    fn synced(&self, id: &str, ready: bool);
    /// Whether book `id`'s recording passed the quick check; false too when it failed before.
    fn checked(&self, id: &str, passed: bool);
    /// Book `id`'s recording was removed (by the user, or unpaired after no audiobook passed the
    /// quick check): it is not the book's narration any more.
    fn removed(&self, id: &str);
}

/// Every recording paired with a book, its sync and the transcriber it uses.
pub(super) struct Recordings {
    library: Arc<Mutex<Library>>,
    state: Mutex<SyncState>,
    /// The last recording laid onto a book's entries, kept until its sync moves on.
    playable: Mutex<Option<(Playable, Arc<RecordingPlan>, usize)>>,
    transcribers: TranscriberFactory,
    models_dir: PathBuf,
}

impl Recordings {
    pub(super) fn new(
        library: Arc<Mutex<Library>>,
        transcribers: TranscriberFactory,
        models_dir: PathBuf,
    ) -> Self {
        Self {
            library,
            state: Mutex::new(SyncState::default()),
            playable: Mutex::new(None),
            transcribers,
            models_dir,
        }
    }

    /// The sync job's thread is woken through `wake`.
    pub(super) fn set_wake(&self, wake: std::sync::mpsc::Sender<()>) {
        lock(&self.state).wake = Some(wake);
    }

    pub(super) fn load_recordings(&self, resume: bool) {
        let rows = lock(&self.library).recordings().unwrap_or_default();
        let mut state = lock(&self.state);
        for row in rows {
            let transcribed_ms = lock(&self.library)
                .transcript_blocks(&row.book_id)
                .ok()
                .and_then(|b| b.last().map(|b| b.end_ms))
                .unwrap_or(0);
            let mut stage = SyncStage::from_stored(&row.stage);
            let unfinished = matches!(row.stage.as_str(), "waiting" | "transcribing");
            if !resume && unfinished {
                stage = SyncStage::Paused;
            }
            state.live.insert(
                row.book_id.0.clone(),
                Live {
                    stage,
                    added_at: row.added_at,
                    total_ms: row.total_ms,
                    transcribed_ms,
                    pace_from: None,
                    abort: Arc::new(AtomicBool::new(false)),
                    sample_checked: false,
                    changes: next_change(),
                },
            );
        }
    }

    fn wake_sync(&self) {
        if let Some(wake) = lock(&self.state).wake.as_ref() {
            let _ = wake.send(());
        }
    }

    /// Does the next piece of sync work. Returns false when there is none.
    pub(super) fn sync_step(&self, host: &dyn SyncHost) -> bool {
        let Some(work) = lock(&self.state).next_work() else {
            return false;
        };
        let (kind, book) = match &work {
            Work::Check(id) => ("check", id),
            Work::Match(id) => ("match", id),
            Work::Transcribe(id) => ("transcribe", id),
            Work::Promote(id) => ("promote", id),
        };
        tracing::debug!(
            event = "sync.work",
            kind,
            book = crate::logging::book(book),
            "Sync work picked"
        );
        let (id, result) = match work {
            Work::Check(id) => {
                let result = self.check_recording(host, &id);
                (id, result)
            }
            Work::Match(id) => {
                let result = self.match_recording(host, &id);
                (id, result)
            }
            Work::Transcribe(id) => {
                let result = self.transcribe_block(host, &id);
                (id, result)
            }
            Work::Promote(id) => {
                let result = self.promote(host, &id);
                (id, result)
            }
        };
        if let Err(error) = result {
            tracing::warn!(event = "sync.error", book = crate::logging::book(&id), %error, "Sync work failed");
            lock(&self.state).reader = None;
            if self.sync_event(host, &id, SyncEvent::Fail, Some(&error)) == SyncStage::Failed {
                host.checked(&id, false);
            }
        }
        true
    }

    /// Applies `event` to a recording's stage, saves and announces the change.
    fn sync_event(
        &self,
        host: &dyn SyncHost,
        id: &str,
        event: SyncEvent,
        error: Option<&str>,
    ) -> SyncStage {
        let (before, after) = {
            let mut state = lock(&self.state);
            let Some(live) = state.live.get_mut(id) else {
                return SyncStage::Failed;
            };
            let before = live.stage;
            live.stage = syncing::next(before, event);
            live.changes = next_change();
            if live.stage == SyncStage::Transcribing && before != SyncStage::Transcribing {
                live.pace_from = Some((Instant::now(), live.transcribed_ms));
                live.abort.store(false, Ordering::Relaxed);
            }
            (before, live.stage)
        };
        if before != after {
            tracing::info!(
                event = "sync.stage",
                book = crate::logging::book(id),
                cause = ?event,
                from = before.name(),
                to = after.name(),
                "Sync stage changed"
            );
        }
        if before != after || error.is_some() {
            let error = (after == SyncStage::Failed).then_some(error).flatten();
            let _ =
                lock(&self.library).set_recording_stage(&BookId(id.into()), after.name(), error);
            self.emit_recording(host, id);
            if before != after {
                host.emit(CoreEvent::LibraryChanged);
            }
        }
        after
    }

    pub(super) fn emit_recording(&self, host: &dyn SyncHost, id: &str) {
        if let Some(view) = self.view(host, id) {
            host.emit(CoreEvent::SyncState(view));
        }
    }

    fn stored_recording(&self, id: &str) -> Result<(RecordingRow, StoredRecording), String> {
        let row = lock(&self.library)
            .recording(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("the book has no recording")?;
        let stored = serde_json::from_str(&row.files).map_err(|e| e.to_string())?;
        Ok((row, stored))
    }

    /// Waiting and first in line: starts transcribing once the model is here (a recognition
    /// server needs none).
    fn promote(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let model = self.model_for_book(host, id);
        if host.recognition().engine == RecognitionEngine::Server
            || self.model_path(model).is_file()
        {
            self.sync_event(host, id, SyncEvent::Promote, None);
        } else {
            self.sync_event(host, id, SyncEvent::ModelMissing, None);
        }
        Ok(())
    }

    /// Missing recordings whose files are back are checked again.
    pub(super) fn look_for_missing_files(&self, host: &dyn SyncHost) {
        let missing: Vec<String> = lock(&self.state)
            .live
            .iter()
            .filter(|(_, l)| l.stage == SyncStage::Missing)
            .map(|(id, _)| id.clone())
            .collect();
        for id in missing {
            let back = self
                .stored_recording(&id)
                .is_ok_and(|(_, s)| s.files.iter().all(StoredFile::unchanged));
            if back {
                self.sync_event(host, &id, SyncEvent::FilesFound, None);
                self.wake_sync();
            }
        }
    }

    /// What audiobook `id` plays on its timeline. Refused when a file is gone.
    pub(super) fn timeline_source(&self, id: &str) -> Result<TimelineSource, String> {
        let (row, stored) = self.stored_recording(id)?;
        if let Some(gone) = stored.files.iter().find(|f| !f.path.exists()) {
            return Err(format!("{} is missing.", file_name(&gone.path)));
        }
        Ok(TimelineSource {
            files: stored.files.iter().map(StoredFile::audio).collect(),
            total_ms: row.total_ms,
            parts: stored
                .parts
                .into_iter()
                .map(|p| (p.title, p.start_ms))
                .collect(),
        })
    }

    /// The recording moved to book `id` (from an audiobook on its own, by the merge) syncs with
    /// the book's text from now on.
    pub(super) fn text_attached(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let (row, _) = self.stored_recording(id)?;
        let transcribed_ms = lock(&self.library)
            .transcript_blocks(&row.book_id)
            .ok()
            .and_then(|b| b.last().map(|b| b.end_ms))
            .unwrap_or(0);
        lock(&self.state).live.insert(
            id.to_owned(),
            Live {
                stage: SyncStage::from_stored(&row.stage),
                added_at: row.added_at,
                total_ms: row.total_ms,
                transcribed_ms,
                pace_from: None,
                abort: Arc::new(AtomicBool::new(false)),
                sample_checked: false,
                changes: next_change(),
            },
        );
        self.sync_event(host, id, SyncEvent::TextAttached, None);
        self.wake_sync();
        Ok(())
    }

    /// Every audio file paired with a book (other than book `except`).
    pub(super) fn paired_files(&self, except: Option<&str>) -> Vec<PathBuf> {
        let rows = lock(&self.library).recordings().unwrap_or_default();
        rows.iter()
            .filter(|row| except != Some(row.book_id.0.as_str()))
            .filter_map(|row| serde_json::from_str::<StoredRecording>(&row.files).ok())
            .flat_map(|stored| stored.files.into_iter().map(|f| f.path))
            .collect()
    }

    /// Removes book `id`'s recording: its sync stops, its row goes, and the host is told. The
    /// audio files are left as they are. The one way a recording is removed from a book that
    /// stays in the library.
    pub(super) fn remove(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        self.forget_recording(id);
        lock(&self.library)
            .remove_recording(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        tracing::info!(
            event = "sync.removed",
            book = crate::logging::book(id),
            "Recording removed"
        );
        host.removed(id);
        host.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// Stops any work on a book's recording and forgets it in memory.
    pub(super) fn forget_recording(&self, id: &str) {
        let mut state = lock(&self.state);
        if let Some(live) = state.live.remove(id) {
            live.abort.store(true, Ordering::Relaxed);
        }
        if state.reader.as_ref().is_some_and(|(r, _)| r == id) {
            state.reader = None;
        }
    }

    pub(super) fn view(&self, host: &dyn SyncHost, id: &str) -> Option<RecordingView> {
        let (row, stored) = self.stored_recording(id).ok()?;
        let report: Option<Report> = lock(&self.library)
            .sync_map(&BookId(id.into()))
            .ok()
            .flatten()
            .and_then(|m| serde_json::from_str(&m.report).ok());
        let state = lock(&self.state);
        let live = state.live.get(id);
        let stage = live.map_or_else(|| SyncStage::from_stored(&row.stage), |l| l.stage);
        let transcribed_ms = live.map_or(0, |l| l.transcribed_ms);
        let remaining_ms = live.and_then(|l| {
            let (since, from) = l.pace_from?;
            let done = l.transcribed_ms.checked_sub(from).filter(|d| *d > 0)?;
            let elapsed = since.elapsed().as_millis() as u64;
            Some((l.total_ms - l.transcribed_ms.min(l.total_ms)) * elapsed / done)
        });
        let position = state.position(id);
        let listenable = live.is_some_and(Live::listenable);
        drop(state);
        let narrates = listenable && host.narration(id) == Some(Narration::Recording);
        Some(RecordingView {
            book_id: id.to_owned(),
            stage,
            error: row.error.clone(),
            files: stored.files.iter().map(StoredFile::view).collect(),
            total_ms: row.total_ms,
            chapters: stored.chapters,
            transcribed_ms,
            remaining_ms: remaining_ms.filter(|_| stage == SyncStage::Transcribing),
            position,
            summary: report.as_ref().map(summary),
            accepted: row.accepted,
            model_id: self.model_for_book(host, id).id.to_owned(),
            listenable,
            narrates,
            actions: syncing::actions(stage, listenable),
        })
    }

    /// Every recording with its stage, for the queue.
    pub(super) fn views(&self, host: &dyn SyncHost) -> Vec<RecordingView> {
        let mut ids: Vec<(i64, String)> = lock(&self.state)
            .live
            .iter()
            .map(|(id, l)| (l.added_at, id.clone()))
            .collect();
        ids.sort();
        ids.into_iter()
            .filter_map(|(_, id)| self.view(host, &id))
            .collect()
    }

    pub(super) fn recording_stage(&self, id: &str) -> Option<SyncStage> {
        lock(&self.state).live.get(id).map(|l| l.stage)
    }

    pub(super) fn sync_report(
        &self,
        host: &dyn SyncHost,
        id: &str,
    ) -> Result<SyncReportView, String> {
        let map = lock(&self.library)
            .sync_map(&BookId(id.into()))
            .map_err(|e| e.to_string())?
            .ok_or("the recording is not matched yet")?;
        let report: Report = serde_json::from_str(&map.report).map_err(|e| e.to_string())?;
        let book = host.load_book(id)?;
        Ok(SyncReportView {
            summary: summary(&report),
            chapters: report
                .chapters
                .iter()
                .enumerate()
                .map(|(index, c)| ChapterMatchView {
                    index,
                    title: book
                        .chapters
                        .get(index)
                        .map(|c| c.title.clone())
                        .unwrap_or_default(),
                    words: c.words,
                    matched_words: c.matched_words,
                    timed_words: c.timed_words,
                    start_ms: c.span.map(|s| s.0),
                    end_ms: c.span.map(|s| s.1),
                })
                .collect(),
            off_text: report
                .unmatched_audio
                .iter()
                .map(|g| OffTextView {
                    start_ms: g.start_ms,
                    end_ms: g.end_ms,
                    excerpt: g.excerpt.clone(),
                })
                .collect(),
        })
    }

    pub(super) fn sync_action(
        &self,
        host: &dyn SyncHost,
        id: &str,
        action: SyncAction,
    ) -> Result<(), String> {
        let book_id = BookId(id.into());
        if !lock(&self.state).live.contains_key(id) {
            return Err("the book has no recording".into());
        }
        match action {
            SyncAction::Pause => {
                if let Some(live) = lock(&self.state).live.get(id) {
                    live.abort.store(true, Ordering::Relaxed);
                }
                self.sync_event(host, id, SyncEvent::Pause, None);
            }
            SyncAction::Resume => {
                self.sync_event(host, id, SyncEvent::Resume, None);
            }
            SyncAction::Retry => {
                self.sync_event(host, id, SyncEvent::Retry, None);
            }
            SyncAction::Accept => match self.recording_stage(id) {
                // "It is this book": transcribe it without the quick check.
                Some(SyncStage::SampleMismatch) => {
                    if let Some(live) = lock(&self.state).live.get_mut(id) {
                        live.sample_checked = true;
                    }
                    self.sync_event(host, id, SyncEvent::Accept, None);
                }
                // Keep a poor match: remembered, so matching again does not ask again.
                Some(SyncStage::PoorMatch) => {
                    lock(&self.library)
                        .set_recording_accepted(&book_id, true)
                        .map_err(|e| e.to_string())?;
                    if self.sync_event(host, id, SyncEvent::Accept, None) == SyncStage::Ready {
                        host.synced(id, true);
                    }
                }
                _ => return Err("there is nothing to accept".into()),
            },
            SyncAction::Resync => {
                {
                    let library = lock(&self.library);
                    library
                        .clear_transcript(&book_id)
                        .map_err(|e| e.to_string())?;
                    library
                        .clear_sync_map(&book_id)
                        .map_err(|e| e.to_string())?;
                    library
                        .set_recording_accepted(&book_id, false)
                        .map_err(|e| e.to_string())?;
                }
                {
                    let mut state = lock(&self.state);
                    if state.reader.as_ref().is_some_and(|(r, _)| r == id) {
                        state.reader = None;
                    }
                    if let Some(live) = state.live.get_mut(id) {
                        live.transcribed_ms = 0;
                        live.changes = next_change();
                        live.sample_checked = false;
                    }
                }
                self.sync_event(host, id, SyncEvent::Resync, None);
            }
        }
        self.wake_sync();
        Ok(())
    }

    pub(super) fn listenable(&self, id: &str) -> bool {
        lock(&self.state).live.get(id).is_some_and(Live::listenable)
    }
}

#[cfg(test)]
mod tests;

/// Lower-case letters and digits only, for comparing titles.
fn simplify(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
