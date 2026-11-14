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

use super::recording::*;
use super::syncing::{self, Event as SyncEvent, Found, Stage as SyncStage};
use super::*;

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

// ---------------------------------------------------------------- live state

/// What Core keeps in memory about each recording besides its stored row.
struct Live {
    stage: SyncStage,
    added_at: i64,
    total_ms: u64,
    transcribed_ms: u64,
    /// When transcription (re)started in this run, and how far it was then: the pace.
    pace_from: Option<(Instant, u64)>,
    /// Set to stop transcription mid-block.
    abort: Arc<AtomicBool>,
    /// The quick check passed in this run.
    sample_checked: bool,
}

impl Live {
    /// Synced, or syncing with part of it transcribed: what is synced can be listened to.
    fn listenable(&self) -> bool {
        self.stage == SyncStage::Ready || (self.stage.listenable() && self.transcribed_ms > 0)
    }
}

#[derive(Default)]
struct SyncState {
    live: HashMap<String, Live>,
    /// Wakes the sync thread.
    wake: Option<std::sync::mpsc::Sender<()>>,
    /// The loaded transcriber, and what it was loaded with.
    transcriber: Option<(TranscriberChoice, Arc<dyn Transcriber>)>,
    /// The recording being transcribed, with the audio decoded ahead of it.
    reader: Option<(String, Decoded)>,
    /// Models being downloaded.
    downloading: HashSet<String>,
}

/// A recording's reader and the 16 kHz audio decoded so far, from `from_ms` on. While one block
/// is being recognised the next one is decoded beside it, so recognition does not wait on it.
struct Decoded {
    reader: Reader,
    from_ms: u64,
    samples: Vec<f32>,
}

const PER_MS: usize = SAMPLE_RATE as usize / 1_000;

impl Decoded {
    fn new(reader: Reader) -> Self {
        Self {
            reader,
            from_ms: 0,
            samples: Vec::new(),
        }
    }

    fn to_ms(&self) -> u64 {
        self.from_ms + (self.samples.len() / PER_MS) as u64
    }

    /// Holds `range`: what is before it is dropped and what is missing is read.
    fn cover(&mut self, range: Range<u64>) -> Result<(), String> {
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
    fn samples(&self, range: Range<u64>) -> &[f32] {
        let at = |ms: u64| (ms - self.from_ms) as usize * PER_MS;
        &self.samples[at(range.start)..at(range.end)]
    }
}

enum Work {
    Check(String),
    Match(String),
    Transcribe(String),
    Promote(String),
}

impl SyncState {
    fn next_work(&self) -> Option<Work> {
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
    fn position(&self, id: &str) -> Option<usize> {
        let mut line: Vec<(&String, &Live)> = self
            .live
            .iter()
            .filter(|(_, l)| matches!(l.stage, SyncStage::Transcribing | SyncStage::Waiting))
            .collect();
        line.sort_by_key(|(id, live)| (live.stage != SyncStage::Transcribing, live.added_at, *id));
        line.iter().position(|(other, _)| other.as_str() == id)
    }
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
}

/// Every recording paired with a book, its sync and the transcriber it uses.
pub(super) struct Recordings {
    library: Arc<Mutex<Library>>,
    state: Mutex<SyncState>,
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
            eprintln!("aoide: syncing {id}: {error}");
            lock(&self.state).reader = None;
            self.sync_event(host, &id, SyncEvent::Fail, Some(&error));
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
            if live.stage == SyncStage::Transcribing && before != SyncStage::Transcribing {
                live.pace_from = Some((Instant::now(), live.transcribed_ms));
                live.abort.store(false, Ordering::Relaxed);
            }
            (before, live.stage)
        };
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

    /// Checking: works out where the recording stands from what is stored.
    fn check_recording(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let (row, stored) = self.stored_recording(id)?;
        if !stored.files.iter().all(StoredFile::unchanged) {
            self.sync_event(host, id, SyncEvent::FilesLost, None);
            return Ok(());
        }
        let book_id = BookId(id.into());
        let transcribed = lock(&self.library)
            .transcript_blocks(&book_id)
            .map_err(|e| e.to_string())?
            .last()
            .map_or(0, |b| b.end_ms);
        if let Some(live) = lock(&self.state).live.get_mut(id) {
            live.transcribed_ms = transcribed;
        }
        if transcribed < row.total_ms {
            self.sync_event(host, id, SyncEvent::Verified(Found::Untranscribed), None);
            return Ok(());
        }
        let book = host.load_book(id)?;
        let stored_map = lock(&self.library)
            .sync_map(&book_id)
            .map_err(|e| e.to_string())?;
        let found = match stored_map.filter(|m| m.text_hash == text_hash(&book)) {
            Some(map) => {
                let report: Report =
                    serde_json::from_str(&map.report).map_err(|e| e.to_string())?;
                Found::Matched {
                    review: poor(&report) && !row.accepted,
                }
            }
            None => Found::Transcribed,
        };
        self.sync_event(host, id, SyncEvent::Verified(found), None);
        Ok(())
    }

    /// The model a book's recording is transcribed with.
    fn model_for_book(&self, host: &dyn SyncHost, id: &str) -> &'static ModelInfo {
        let size = host.recognition().model_size.into();
        let language = lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .and_then(|b| b.language);
        models::model_for(size, language.as_deref())
    }

    fn model_path(&self, model: &ModelInfo) -> PathBuf {
        self.models_dir.join(model.file_name())
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

    fn transcriber_for(
        &self,
        host: &dyn SyncHost,
        id: &str,
    ) -> Result<Arc<dyn Transcriber>, String> {
        let settings = host.recognition();
        let choice = match settings.engine {
            RecognitionEngine::Server => TranscriberChoice::Server {
                base_url: settings.server.base_url,
                model: settings.server.model,
                network: settings.server.network,
            },
            RecognitionEngine::BuiltIn => TranscriberChoice::Whisper {
                model: self.model_path(self.model_for_book(host, id)),
                use_gpu: match settings.device {
                    RecognitionDevice::Auto | RecognitionDevice::Gpu => models::GPU_BUILD,
                    RecognitionDevice::Cpu => false,
                },
                threads: settings.threads as usize,
            },
        };
        if let Some((loaded, transcriber)) = lock(&self.state).transcriber.as_ref() {
            if *loaded == choice {
                return Ok(Arc::clone(transcriber));
            }
        }
        lock(&self.state).transcriber = None;
        let transcriber: Arc<dyn Transcriber> = Arc::from((self.transcribers)(&choice)?);
        lock(&self.state).transcriber = Some((choice, Arc::clone(&transcriber)));
        Ok(transcriber)
    }

    /// Transcribing: the quick check first, then one block.
    fn transcribe_block(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let (row, stored) = self.stored_recording(id)?;
        let transcriber = self.transcriber_for(host, id)?;
        let language = whisper_language(
            lock(&self.library)
                .book(&BookId(id.into()))
                .ok()
                .flatten()
                .and_then(|b| b.language)
                .as_deref(),
        );
        let (done, abort, checked) = {
            let state = lock(&self.state);
            let live = state.live.get(id).ok_or("the recording is gone")?;
            (
                live.transcribed_ms,
                Arc::clone(&live.abort),
                live.sample_checked,
            )
        };
        let mut decoded = match lock(&self.state).reader.take() {
            Some((reading, decoded)) if reading == id => decoded,
            _ => Decoded::new(Reader::new(
                stored.files.iter().map(StoredFile::audio).collect(),
            )),
        };
        let total = row.total_ms;
        let hear = |samples: &[f32]| {
            transcriber.transcribe(&Request {
                samples,
                language: Some(&language),
                abort: Some(Arc::clone(&abort)),
            })
        };

        if done == 0 && !checked {
            let book = host.load_book(id)?;
            let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
            let mut found = 0;
            for at in SAMPLE_AT {
                let from = ((total as f64 * at) as u64).min(total.saturating_sub(SAMPLE_MS));
                let to = (from + SAMPLE_MS).min(total);
                let samples = decoded
                    .reader
                    .read(from..to, SAMPLE_RATE)
                    .map_err(|e| e.to_string())?;
                let words = match hear(&samples) {
                    Ok(words) => words,
                    Err(aoide_transcribe::Error::Aborted) => return Ok(()),
                    Err(error) => return Err(error.to_string()),
                };
                let heard: Vec<HeardWord> = words
                    .into_iter()
                    .map(|w| HeardWord {
                        text: w.text,
                        start_ms: w.start_ms,
                        end_ms: w.end_ms,
                    })
                    .collect();
                if found_in_text(&texts, &heard) >= SAMPLE_FOUND {
                    found += 1;
                }
            }
            if found < 2 {
                self.sync_event(host, id, SyncEvent::SampleFailed, None);
                return Ok(());
            }
            if let Some(live) = lock(&self.state).live.get_mut(id) {
                live.sample_checked = true;
            }
        }

        let target = (done + BLOCK_MS).min(total);
        let read_to = (target + CUT_SLACK_MS).min(total);
        decoded.cover(done..read_to)?;
        let per_frame = (SAMPLE_RATE * FRAME_MS / 1_000) as usize;
        let cut = if read_to == total {
            decoded.samples(done..read_to).len()
        } else {
            let samples = decoded.samples(done..read_to);
            let from = (target - done).saturating_sub(CUT_SLACK_MS).max(1_000);
            let window = from as usize * per_frame / FRAME_MS as usize..samples.len();
            (quietest_cut(samples, SAMPLE_RATE, window) / per_frame * per_frame).max(per_frame)
        };
        // The next block starts at this one's cut, at most CUT_SLACK_MS before `read_to`, so
        // reading this far covers all of it. It is decoded while this block is recognised.
        let ahead = (read_to + BLOCK_MS + CUT_SLACK_MS).min(total);
        // From where the decoded audio ends, which may be past `read_to` already.
        let decoded_to = decoded.to_ms();
        let (heard, more) = std::thread::scope(|scope| {
            let Decoded {
                reader, samples, ..
            } = &mut decoded;
            let reading = (ahead > decoded_to)
                .then(|| scope.spawn(move || reader.read(decoded_to..ahead, SAMPLE_RATE)));
            let heard = hear(&samples[..cut]);
            (heard, reading.and_then(|r| r.join().ok()))
        });
        // A failed read ahead is read again, and its error reported, with the next block.
        if let Some(Ok(more)) = more {
            decoded.samples.extend(more);
        }
        let words = match heard {
            Ok(words) => words,
            Err(aoide_transcribe::Error::Aborted) => {
                lock(&self.state).reader = Some((id.to_owned(), decoded));
                return Ok(());
            }
            Err(error) => return Err(error.to_string()),
        };
        let end = if read_to == total {
            total
        } else {
            done + (cut / per_frame) as u64 * u64::from(FRAME_MS)
        };
        let words_json: Vec<(u64, u64, String)> = words
            .into_iter()
            .map(|w| (done + w.start_ms, done + w.end_ms, w.text))
            .collect();
        let levels = Loudness::measure(decoded.samples(done..end), SAMPLE_RATE, FRAME_MS).levels;
        lock(&self.library)
            .add_transcript_block(
                &BookId(id.into()),
                &TranscriptBlock {
                    start_ms: done,
                    end_ms: end,
                    transcriber: transcriber.id().to_owned(),
                    words: serde_json::to_string(&words_json).map_err(|e| e.to_string())?,
                    levels,
                },
            )
            .map_err(|e| e.to_string())?;
        {
            let mut state = lock(&self.state);
            if let Some(live) = state.live.get_mut(id) {
                live.transcribed_ms = end;
            }
            if end < total {
                state.reader = Some((id.to_owned(), decoded));
            }
        }
        if end >= total {
            self.sync_event(host, id, SyncEvent::TranscriptDone, None);
        } else {
            self.emit_recording(host, id);
            host.synced(id, false);
        }
        Ok(())
    }

    fn heard_words(&self, id: &str) -> Result<Vec<HeardWord>, String> {
        let blocks = lock(&self.library)
            .transcript_blocks(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        let mut heard = Vec::new();
        for block in blocks {
            let words: Vec<(u64, u64, String)> =
                serde_json::from_str(&block.words).map_err(|e| e.to_string())?;
            // Transcripts are stored as heard; the model's lateness is taken off here.
            let late = lateness_ms(&block.transcriber);
            heard.extend(words.into_iter().map(|(start_ms, end_ms, text)| HeardWord {
                text,
                start_ms: start_ms.saturating_sub(late),
                end_ms: end_ms.saturating_sub(late),
            }));
        }
        Ok(heard)
    }

    /// Matching: the transcript against the text, then Ready (or PoorMatch).
    fn match_recording(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let (row, _) = self.stored_recording(id)?;
        let book = host.load_book(id)?;
        let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
        let heard = self.heard_words(id)?;
        let SyncMap { chapters, report } = match_book(&texts, &heard);
        let review = poor(&report) && !row.accepted;
        lock(&self.library)
            .save_sync_map(
                &BookId(id.into()),
                &StoredSyncMap {
                    text_hash: text_hash(&book),
                    map: encode_chapters(&chapters),
                    report: serde_json::to_string(&report).map_err(|e| e.to_string())?,
                    created_at: now_ms(),
                },
            )
            .map_err(|e| e.to_string())?;
        if self.sync_event(host, id, SyncEvent::Matched { review }, None) == SyncStage::Ready {
            host.synced(id, true);
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

    pub(super) fn inspect_recording(&self, paths: Vec<String>) -> InspectionView {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        let found = aoide_recording::inspect(&paths);
        let suggested_book = found.title.as_deref().and_then(|title| {
            let wanted = simplify(title);
            let books = lock(&self.library).books().ok()?;
            books
                .into_iter()
                .find(|b| {
                    b.title.as_deref().is_some_and(|t| {
                        let t = simplify(t);
                        !t.is_empty() && (wanted.contains(&t) || t.contains(&wanted))
                    })
                })
                .map(|b| b.id.0)
        });
        InspectionView {
            files: found.files.iter().map(file_view).collect(),
            problems: found
                .problems
                .iter()
                .map(|p| RecordingProblemView {
                    path: p.path.to_string_lossy().into_owned(),
                    name: file_name(&p.path),
                    problem: problem_text(&p.problem),
                    drm: matches!(p.problem, Problem::Drm(_)),
                })
                .collect(),
            total_ms: found.total_ms(),
            chapters: found.chapters.len(),
            title: found.title.clone(),
            author: found.author.clone(),
            suggested_book,
        }
    }

    /// Pairs audio files with a book (replacing any recording it had) and starts syncing.
    pub(super) fn attach_recording(
        &self,
        host: &dyn SyncHost,
        id: &str,
        paths: Vec<String>,
    ) -> Result<RecordingView, String> {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        self.attach(host, id, aoide_recording::inspect(&paths))
    }

    /// Like [`Core::attach_recording`] for audio files put in order by hand: they play in the
    /// order given.
    pub(super) fn attach_recording_as_listed(
        &self,
        host: &dyn SyncHost,
        id: &str,
        files: Vec<String>,
    ) -> Result<RecordingView, String> {
        let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
        self.attach(host, id, aoide_recording::inspect_as_listed(&files))
    }

    fn attach(
        &self,
        host: &dyn SyncHost,
        id: &str,
        found: aoide_recording::Inspection,
    ) -> Result<RecordingView, String> {
        let book_id = BookId(id.into());
        lock(&self.library)
            .book(&book_id)
            .map_err(|e| e.to_string())?
            .ok_or("book not found")?;
        if let Some(problem) = found.problems.first() {
            return Err(format!(
                "{}: {}",
                file_name(&problem.path),
                problem_text(&problem.problem)
            ));
        }
        if found.files.is_empty() {
            return Err("No audio files were found there.".into());
        }
        let mut files = Vec::with_capacity(found.files.len());
        for file in &found.files {
            let metadata = std::fs::metadata(&file.path).map_err(|e| e.to_string())?;
            files.push(StoredFile {
                path: file.path.clone(),
                size: file.size,
                modified_ms: modified_ms(&metadata),
                codec: file.codec.clone(),
                sample_rate: file.sample_rate,
                channels: file.channels,
                frames: file.frames,
            });
        }
        let stored = StoredRecording {
            files,
            chapters: found.chapters.len(),
            title: found.title.clone(),
            author: found.author.clone(),
        };
        self.forget_recording(id);
        let now = now_ms();
        {
            let library = lock(&self.library);
            library
                .remove_recording(&book_id)
                .map_err(|e| e.to_string())?;
            library
                .save_recording(&RecordingRow {
                    book_id: book_id.clone(),
                    files: serde_json::to_string(&stored).map_err(|e| e.to_string())?,
                    total_ms: found.total_ms(),
                    stage: SyncStage::Checking.name().into(),
                    error: None,
                    accepted: false,
                    added_at: now,
                })
                .map_err(|e| e.to_string())?;
        }
        lock(&self.state).live.insert(
            id.to_owned(),
            Live {
                stage: SyncStage::Checking,
                added_at: now,
                total_ms: found.total_ms(),
                transcribed_ms: 0,
                pace_from: None,
                abort: Arc::new(AtomicBool::new(false)),
                sample_checked: false,
            },
        );
        host.emit(CoreEvent::LibraryChanged);
        self.emit_recording(host, id);
        self.wake_sync();
        self.view(host, id)
            .ok_or_else(|| "the recording was not saved".into())
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

    pub(super) fn recognition_options(&self, host: &dyn SyncHost) -> RecognitionOptions {
        RecognitionOptions {
            settings: host.recognition(),
            gpu_build: models::GPU_BUILD,
            models: models::MODELS
                .iter()
                .map(|m| ModelView {
                    id: m.id.into(),
                    size: match m.size {
                        ModelSize::Tiny => ModelSizeChoice::Tiny,
                        ModelSize::Base => ModelSizeChoice::Base,
                        ModelSize::Small => ModelSizeChoice::Small,
                    },
                    english_only: m.english_only,
                    bytes: m.bytes,
                    downloaded: self.model_path(m).is_file(),
                })
                .collect(),
            models_folder: self.models_dir.to_string_lossy().into_owned(),
        }
    }

    /// A model arrived (downloaded or put in place): recordings waiting for it carry on.
    pub(super) fn models_changed(&self, host: &dyn SyncHost) {
        let waiting: Vec<String> = lock(&self.state)
            .live
            .iter()
            .filter(|(_, l)| l.stage == SyncStage::NeedsModel)
            .map(|(id, _)| id.clone())
            .collect();
        for id in waiting {
            self.sync_event(host, &id, SyncEvent::ModelReady, None);
        }
        lock(&self.state).transcriber = None;
        self.wake_sync();
    }

    /// Downloads a model from its fixed address and keeps it only if its checksum matches.
    pub(super) async fn download_model(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let model = models::model(id).ok_or("no such model")?;
        if !lock(&self.state).downloading.insert(id.to_owned()) {
            return Ok(());
        }
        let result = self.fetch_model(host, model, &model.url()).await;
        lock(&self.state).downloading.remove(id);
        let (stage, error) = match &result {
            Ok(()) => (DownloadStage::Done, None),
            Err(error) => (DownloadStage::Failed, Some(error.clone())),
        };
        host.emit(CoreEvent::ModelDownload(ModelDownloadView {
            id: id.into(),
            stage,
            done_bytes: if result.is_ok() { model.bytes } else { 0 },
            total_bytes: model.bytes,
            error,
        }));
        if result.is_ok() {
            self.models_changed(host);
        }
        result
    }

    async fn fetch_model(
        &self,
        host: &dyn SyncHost,
        model: &ModelInfo,
        url: &str,
    ) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        use tokio::io::AsyncWriteExt;
        std::fs::create_dir_all(&self.models_dir).map_err(|e| e.to_string())?;
        let path = self.model_path(model);
        let partial = path.with_extension("bin.part");
        let response = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| format!("download failed: {e}"))?;
        let mut file = tokio::fs::File::create(&partial)
            .await
            .map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut last_report = Instant::now();
        let mut response = response;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| format!("download failed: {e}"))?
        {
            hasher.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            done += chunk.len() as u64;
            if last_report.elapsed() > Duration::from_millis(250) {
                last_report = Instant::now();
                host.emit(CoreEvent::ModelDownload(ModelDownloadView {
                    id: model.id.into(),
                    stage: DownloadStage::Downloading,
                    done_bytes: done,
                    total_bytes: model.bytes,
                    error: None,
                }));
            }
        }
        file.flush().await.map_err(|e| e.to_string())?;
        drop(file);
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if digest != model.sha256 {
            let _ = std::fs::remove_file(&partial);
            return Err("the downloaded model is not the expected file (checksum mismatch)".into());
        }
        std::fs::rename(&partial, &path).map_err(|e| e.to_string())
    }

    /// The book's recording laid onto its entries, and how many of them are synced: once
    /// synced, by the stored sync map; while it syncs, by the transcript so far matched here,
    /// with the entries the sync has not safely reached left for playback to wait on.
    pub(super) fn playable(
        &self,
        host: &dyn SyncHost,
        book: &BookModel,
    ) -> Result<(RecordingPlan, usize), String> {
        let book_id = BookId(book.id.clone());
        let (row, stored) = self.stored_recording(&book.id)?;
        let blocks = lock(&self.library)
            .transcript_blocks(&book_id)
            .map_err(|e| e.to_string())?;
        let (map, edge) = if self.recording_stage(&book.id) == Some(SyncStage::Ready) {
            let map = lock(&self.library)
                .sync_map(&book_id)
                .map_err(|e| e.to_string())?
                .ok_or("the recording is not matched yet")?;
            if map.text_hash != text_hash(book) {
                self.sync_event(host, &book.id, SyncEvent::TextChanged, None);
                self.wake_sync();
                return Err("the book's text changed: its recording is being matched again".into());
            }
            let chapters = decode_chapters(&map.map).ok_or("the stored sync map is damaged")?;
            let report: Report = serde_json::from_str(&map.report).map_err(|e| e.to_string())?;
            (SyncMap { chapters, report }, None)
        } else {
            let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
            let edge = blocks.last().map_or(0, |b| b.end_ms);
            (match_book(&texts, &self.heard_words(&book.id)?), Some(edge))
        };
        let mut levels = Vec::new();
        for block in blocks {
            levels.extend(block.levels);
        }
        let loudness = Loudness {
            frame_ms: FRAME_MS,
            levels,
        };
        let units: Vec<Unit> = book
            .chapters
            .iter()
            .enumerate()
            .flat_map(|(chapter, model)| {
                model.segments.iter().map(move |segment| Unit {
                    chapter,
                    range: segment.range.clone(),
                    words: segment.words.clone(),
                })
            })
            .collect();
        let clips = clips(&map, &units, edge.unwrap_or(row.total_ms), Some(&loudness));
        // While it syncs: the units after the last one that ends safely before the sync's edge
        // wait for it (the matcher may still move what lies near the edge).
        let synced = edge.map_or(clips.len(), |edge| {
            let safe = edge.saturating_sub(SYNC_MARGIN_MS);
            clips
                .iter()
                .rposition(|clip| !clip.audio.is_empty() && clip.audio.end <= safe)
                .map_or(0, |last| last + 1)
        });
        let recording = RecordingPlan {
            files: stored.files.iter().map(StoredFile::audio).collect(),
            clips,
        };
        Ok((recording, synced))
    }
}

/// Lower-case letters and digits only, for comparing titles.
fn simplify(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}
