//! Recordings paired with books: inspecting and attaching audio files, the sync job (transcribe
//! block by block, then match the words to the text), the speech-recognition models, and the
//! plan that plays a recording in place of a speech engine.
//!
//! Each recording's stage changes only through [`syncing::next`]. The job runs one step at a
//! time ([`Core::sync_step`]) on a thread of its own, so a pause takes effect between blocks, or
//! at once through the transcriber's abort flag.

use std::ops::Range;

use aoide_recording::AudioFile;
use aoide_sync::Clip;
use aoide_transcribe::models::ModelSize;
use aoide_transcribe::Transcriber;
use aoide_tts::NetworkScope;
use serde::Deserialize;

use super::recordings::SyncHost;
use super::syncing::Stage as SyncStage;
use super::*;

/// A clicked sentence starts this much before its first word.
const LEAD_MS: u64 = 150;

// ---------------------------------------------------------------- settings

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ModelSizeChoice {
    Tiny,
    #[default]
    Base,
    Small,
}

impl From<ModelSizeChoice> for ModelSize {
    fn from(choice: ModelSizeChoice) -> Self {
        match choice {
            ModelSizeChoice::Tiny => ModelSize::Tiny,
            ModelSizeChoice::Base => ModelSize::Base,
            ModelSizeChoice::Small => ModelSize::Small,
        }
    }
}

/// Where speech recognition runs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum RecognitionDevice {
    /// The GPU when this build supports one, otherwise the CPU.
    #[default]
    Auto,
    Gpu,
    Cpu,
}

/// What recognises the narration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum RecognitionEngine {
    /// whisper.cpp inside Aoide, with a model downloaded once.
    #[default]
    BuiltIn,
    /// A server with the OpenAI transcription API (`/v1/audio/transcriptions`).
    Server,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecognitionServer {
    pub base_url: String,
    /// Sent as `model`; empty sends `whisper-1`.
    pub model: String,
    pub network: NetworkScope,
}

impl Default for RecognitionServer {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8000".into(),
            model: String::new(),
            network: NetworkScope::Loopback,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecognitionSettings {
    pub engine: RecognitionEngine,
    /// Used when `engine` is `Server`.
    pub server: RecognitionServer,
    pub model_size: ModelSizeChoice,
    pub device: RecognitionDevice,
    /// CPU threads; 0 uses half the cores.
    pub threads: u32,
    /// Carry on syncing recordings when Aoide starts.
    pub resume_on_start: bool,
}

impl Default for RecognitionSettings {
    fn default() -> Self {
        Self {
            engine: RecognitionEngine::BuiltIn,
            server: RecognitionServer::default(),
            model_size: ModelSizeChoice::Base,
            device: RecognitionDevice::Auto,
            threads: 0,
            resume_on_start: true,
        }
    }
}

/// Which narration a book plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum Narration {
    /// A speech engine reads it.
    #[default]
    Speech,
    /// Its paired recording plays, once synced.
    Recording,
}

// ---------------------------------------------------------------- transcribers

/// What a transcriber is loaded with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriberChoice {
    Whisper {
        model: PathBuf,
        use_gpu: bool,
        threads: usize,
    },
    Server {
        base_url: String,
        model: String,
        network: NetworkScope,
    },
}

/// Makes transcribers: whisper.cpp or a server in the app, a script in tests.
pub type TranscriberFactory =
    Arc<dyn Fn(&TranscriberChoice) -> Result<Box<dyn Transcriber>, String> + Send + Sync>;

/// whisper.cpp (when this build has it) or a recognition server.
pub fn app_transcribers() -> TranscriberFactory {
    Arc::new(|choice: &TranscriberChoice| match choice {
        TranscriberChoice::Server {
            base_url,
            model,
            network,
        } => server_transcriber(base_url, model, *network)
            .map(|s| Box::new(s) as Box<dyn Transcriber>),
        #[cfg(feature = "whisper")]
        TranscriberChoice::Whisper {
            model,
            use_gpu,
            threads,
        } => aoide_transcribe::WhisperCpp::new(aoide_transcribe::WhisperOptions {
            model: model.clone(),
            threads: *threads,
            use_gpu: *use_gpu,
            vad_model: None,
        })
        .map(|w| Box::new(w) as Box<dyn Transcriber>)
        .map_err(|e| e.to_string()),
        #[cfg(not(feature = "whisper"))]
        TranscriberChoice::Whisper { .. } => Err(
            "this build of Aoide has no built-in speech recognition; use a recognition server"
                .into(),
        ),
    })
}

fn server_transcriber(
    base_url: &str,
    model: &str,
    network: NetworkScope,
) -> Result<aoide_transcribe::ServerTranscriber, String> {
    aoide_transcribe::ServerTranscriber::new(aoide_transcribe::ServerOptions {
        base_url: base_url.to_owned(),
        model: Some(model.to_owned()),
        network,
        ..Default::default()
    })
    .map_err(|e| e.to_string())
}

impl RecognitionSettings {
    /// Refuses a server address outside its network scope.
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.engine == RecognitionEngine::Server {
            aoide_tts::validate_endpoint(&self.server.base_url, self.server.network)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- views

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecordingFileView {
    pub path: String,
    pub name: String,
    pub duration_ms: u64,
    pub codec: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecordingProblemView {
    pub path: String,
    pub name: String,
    /// Why it is left out, for people.
    pub problem: String,
    pub drm: bool,
}

/// Audio files dropped or chosen, before they are paired with a book.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct InspectionView {
    pub files: Vec<RecordingFileView>,
    pub problems: Vec<RecordingProblemView>,
    pub total_ms: u64,
    pub chapters: usize,
    pub title: Option<String>,
    pub author: Option<String>,
    /// The library book whose title the tags name, if any.
    pub suggested_book: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct MatchSummaryView {
    pub words: usize,
    pub matched_words: usize,
    pub timed_words: usize,
}

/// A book's recording and its sync.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecordingView {
    pub book_id: String,
    pub stage: SyncStage,
    pub error: Option<String>,
    pub files: Vec<RecordingFileView>,
    pub total_ms: u64,
    pub chapters: usize,
    /// How much of the recording is transcribed.
    pub transcribed_ms: u64,
    /// Time left transcribing, once there is a pace to go by.
    pub remaining_ms: Option<u64>,
    /// Place in line among recordings waiting to be transcribed (0 is next).
    pub position: Option<usize>,
    pub summary: Option<MatchSummaryView>,
    pub accepted: bool,
    /// The model this book's recording is transcribed with.
    pub model_id: String,
    /// It can be listened to: synced, or syncing with part of it transcribed (what is synced
    /// so far plays).
    pub listenable: bool,
    /// It narrates the book: chosen as the book's narration, and listenable.
    pub narrates: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterMatchView {
    pub index: usize,
    pub title: String,
    pub words: usize,
    pub matched_words: usize,
    pub timed_words: usize,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct OffTextView {
    pub start_ms: u64,
    pub end_ms: u64,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SyncReportView {
    pub summary: MatchSummaryView,
    pub chapters: Vec<ChapterMatchView>,
    /// Narration that is not in the text (credits, a dedication left out of the EPUB…).
    pub off_text: Vec<OffTextView>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum SyncAction {
    Pause,
    Resume,
    Retry,
    Accept,
    Resync,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ModelView {
    pub id: String,
    pub size: ModelSizeChoice,
    pub english_only: bool,
    pub bytes: u64,
    pub downloaded: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct RecognitionOptions {
    pub settings: RecognitionSettings,
    /// This build can use a GPU.
    pub gpu_build: bool,
    pub models: Vec<ModelView>,
    pub models_folder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum DownloadStage {
    Downloading,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ModelDownloadView {
    pub id: String,
    pub stage: DownloadStage,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub error: Option<String>,
}

// ---------------------------------------------------------------- playing a recording

/// A recording laid onto a book's playlist: one clip per entry.
pub struct RecordingPlan {
    pub files: Vec<AudioFile>,
    pub clips: Vec<Clip>,
}

impl RecordingPlan {
    /// Where playing `index` should start, in milliseconds into its clip: just before its text.
    pub fn start_offset(&self, index: usize) -> u32 {
        self.clips.get(index).map_or(0, |clip| {
            clip.text.as_ref().map_or(0, |text| {
                text.start
                    .saturating_sub(LEAD_MS)
                    .saturating_sub(clip.audio.start) as u32
            })
        })
    }

    /// Whether `offset_ms` into `index` is narration that is not in the text.
    pub fn off_text(&self, index: usize, offset_ms: u32) -> bool {
        self.clips.get(index).is_some_and(|clip| {
            let at = clip.audio.start + u64::from(offset_ms);
            clip.off_text.iter().any(|r| r.contains(&at))
        })
    }

    /// Word times of `index` relative to its clip, unheard words placed between their neighbours.
    pub fn word_times(&self, index: usize) -> Vec<aoide_text::WordEstimate> {
        let Some(clip) = self.clips.get(index) else {
            return Vec::new();
        };
        let length = clip.audio.end - clip.audio.start;
        let words = &clip.words;
        let mut out = Vec::with_capacity(words.len());
        for (i, word) in words.iter().enumerate() {
            let span = word.clone().unwrap_or_else(|| {
                let before = words[..i]
                    .iter()
                    .rev()
                    .flatten()
                    .next()
                    .map_or(0, |w| w.end);
                let after = words[i + 1..]
                    .iter()
                    .flatten()
                    .next()
                    .map_or(length, |w| w.start);
                before..after.max(before)
            });
            out.push(aoide_text::WordEstimate {
                start_ms: span.start as u32,
                end_ms: span.end as u32,
            });
        }
        out
    }
}

// ---------------------------------------------------------------- core

impl SyncHost for Core {
    fn emit(&self, event: CoreEvent) {
        Core::emit(self, event);
    }

    fn load_book(&self, id: &str) -> Result<Arc<BookModel>, String> {
        Core::load_book(self, id)
    }

    fn recognition(&self) -> RecognitionSettings {
        self.app_settings().recognition
    }

    fn narration(&self, id: &str) -> Option<Narration> {
        self.book_settings(id).narration
    }

    fn synced(&self, id: &str, ready: bool) {
        if ready {
            self.narrate_with_recording_by_default(id);
        }
        self.follow_sync(id, ready);
    }

    fn checked(&self, id: &str, passed: bool) {
        self.audiobook_checked(id, passed);
    }
}

/// The sync job's commands, with Core as its host.
impl Core {
    /// Does the next piece of sync work. Returns false when there is none.
    pub fn sync_step(&self) -> bool {
        self.recordings.sync_step(self)
    }

    pub fn inspect_recording(&self, paths: Vec<String>) -> InspectionView {
        self.recordings.inspect_recording(paths)
    }

    /// Pairs audio files with a book (replacing any recording it had) and starts syncing.
    pub fn attach_recording(&self, id: &str, paths: Vec<String>) -> Result<RecordingView, String> {
        self.recordings.attach_recording(self, id, paths)
    }

    /// Like [`Core::attach_recording`] for audio files put in order by hand: they play in the
    /// order given.
    pub fn attach_recording_as_listed(
        &self,
        id: &str,
        files: Vec<String>,
    ) -> Result<RecordingView, String> {
        self.recordings.attach_recording_as_listed(self, id, files)
    }

    pub fn recording(&self, id: &str) -> Option<RecordingView> {
        self.recordings.view(self, id)
    }

    /// Every recording with its stage, for the queue.
    pub fn recordings(&self) -> Vec<RecordingView> {
        self.recordings.views(self)
    }

    pub fn sync_report(&self, id: &str) -> Result<SyncReportView, String> {
        self.recordings.sync_report(self, id)
    }

    pub fn sync_action(&self, id: &str, action: SyncAction) -> Result<(), String> {
        self.recordings.sync_action(self, id, action)
    }

    pub fn recognition_options(&self) -> RecognitionOptions {
        self.recordings.recognition_options(self)
    }

    /// Downloads a model from its fixed address and keeps it only if its checksum matches.
    pub async fn download_model(&self, id: &str) -> Result<(), String> {
        self.recordings.download_model(self, id).await
    }

    /// A model arrived (downloaded or put in place): recordings waiting for it carry on.
    pub(super) fn models_changed(&self) {
        self.recordings.models_changed(self);
    }

    pub(super) fn emit_recording(&self, id: &str) {
        self.recordings.emit_recording(self, id);
    }

    pub(super) fn recording_stage(&self, id: &str) -> Option<SyncStage> {
        self.recordings.recording_stage(id)
    }

    /// Missing recordings whose files are back are checked again.
    pub(super) fn look_for_missing_files(&self) {
        self.recordings.look_for_missing_files(self);
    }

    pub(super) fn forget_recording(&self, id: &str) {
        self.recordings.forget_recording(id);
    }
}

impl Core {
    /// Runs the sync job on a thread of its own until Core is dropped.
    pub(super) fn spawn_sync_runner(self: &Arc<Self>) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.recordings.set_wake(tx);
        let core = Arc::downgrade(self);
        let spawned = std::thread::Builder::new()
            .name("aoide-sync".into())
            .spawn(move || loop {
                let Some(strong) = core.upgrade() else { return };
                let worked = strong.sync_step();
                drop(strong);
                if worked {
                    continue;
                }
                match rx.recv_timeout(Duration::from_secs(60)) {
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        if let Some(strong) = core.upgrade() {
                            strong.look_for_missing_files();
                        }
                    }
                    Ok(()) => {}
                }
            });
        if let Err(error) = spawned {
            eprintln!("aoide: cannot start syncing: {error}");
        }
    }

    /// A book whose narration was never chosen plays its recording once it is synced.
    fn narrate_with_recording_by_default(&self, id: &str) {
        let key = book_settings_key(id);
        let library = lock(&self.library);
        let stored = library.setting(&key).ok().flatten();
        let mut settings: BookSettings = stored
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default();
        if settings.narration.is_some() {
            return;
        }
        settings.narration = Some(Narration::Recording);
        if let Ok(json) = serde_json::to_string(&settings) {
            let _ = library.set_setting(&key, &json);
        }
        drop(library);
        lock(&self.books).remove(id);
        self.emit_recording(id);
    }

    // ------------------------------------------------------------ commands

    /// Forgets a book's recording and its sync. The audio files are left as they are.
    pub async fn remove_recording(&self, id: &str) -> Result<(), String> {
        self.forget_recording(id);
        lock(&self.library)
            .remove_recording(&BookId(id.into()))
            .map_err(|e| e.to_string())?;
        self.audiobook_unpaired(id);
        let mut settings = self.book_settings(id);
        if settings.narration.is_some() {
            settings.narration = None;
            self.save_book_settings(id, settings).await?;
        }
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    pub async fn set_narration(&self, id: &str, narration: Narration) -> Result<(), String> {
        if narration == Narration::Recording && !self.recordings.listenable(id) {
            return Err("the recording is not synced yet".into());
        }
        let mut settings = self.book_settings(id);
        settings.narration = Some(narration);
        self.save_book_settings(id, settings).await
    }

    /// Plays the book's recording from `at_ms` on its timeline (a stretch in the sync report).
    pub async fn play_recording_at(&self, id: &str, at_ms: u64) -> Result<(), String> {
        if !self.plays_recording(id, &self.book_settings(id)) {
            return Err("choose the recording as the book's narration to listen to it".into());
        }
        if self.playback.recording_of(id).is_none() {
            self.play(id, None, None).await?;
        }
        self.playback.seek_recording(id, at_ms)?;
        self.save_progress(true);
        self.replan(true);
        self.playback.play();
        self.ensure_output()
    }

    /// The spoken sentences of `chapter` that the book's recording has no audio for, when the
    /// recording narrates the book.
    pub(super) fn not_narrated(&self, book: Arc<BookModel>, chapter: usize) -> Option<Vec<String>> {
        let settings = self.book_settings(&book.id);
        if !self.plays_recording(&book.id, &settings) {
            return None;
        }
        let model = book.chapters.get(chapter)?;
        let entries = model.first_entry..model.first_entry + model.segments.len();
        let unrecorded = match self.playback.unrecorded(&book.id, entries.clone()) {
            Some(unrecorded) => unrecorded,
            None => self
                .build_recording_target(Arc::clone(&book), &settings)
                .ok()?
                .narration
                .unrecorded(entries),
        };
        Some(
            unrecorded
                .into_iter()
                .filter_map(|index| book.segment(index))
                .filter(|segment| !segment.speech.is_empty())
                .map(|segment| segment.id.clone())
                .collect(),
        )
    }

    /// Whether a book plays its recording.
    pub(super) fn plays_recording(&self, id: &str, settings: &BookSettings) -> bool {
        settings.narration == Some(Narration::Recording) && self.recordings.listenable(id)
    }

    /// The recording being listened to while it syncs has more synced (or is done): playback
    /// follows the new map. `force` refreshes even when nobody is waiting on the sync.
    fn follow_sync(&self, id: &str, force: bool) {
        let Some(book) = self.playback.should_follow(id, force) else {
            return;
        };
        let settings = self.book_settings(id);
        let target = match self.build_recording_target(book, &settings) {
            Ok(target) => target,
            Err(error) => return eprintln!("aoide: following the sync of {id}: {error}"),
        };
        if self.playback.follow(id, target) == Some(true) {
            self.replan(false);
            self.emit_generation();
        }
    }

    // ------------------------------------------------------------ playback

    /// The book's recording as a playback target: its clips (see
    /// [`super::recordings::Recordings::playable`]), and, when `settings` ask for it, what it
    /// lacks said by the speech engine.
    pub(super) fn build_recording_target(
        &self,
        book: Arc<BookModel>,
        settings: &BookSettings,
    ) -> Result<Target, String> {
        let (recording, synced) = self.recordings.playable(self, &book)?;
        let narration =
            super::narration::NarrationPlan::recording(&book, recording, synced, || {
                settings
                    .speak_missing
                    .then(|| {
                        self.speech_plan(&book)
                            .map_err(|e| {
                                eprintln!("aoide: cannot speak what the recording lacks: {e}")
                            })
                            .ok()
                    })
                    .flatten()
            });
        Ok(Target { book, narration })
    }
}

/// A clip's audio, for the feeder.
pub(super) fn clip_audio(plan: &RecordingPlan, index: usize) -> Option<Range<u64>> {
    plan.clips
        .get(index)
        .map(|c| c.audio.clone())
        .filter(|a| !a.is_empty())
}
