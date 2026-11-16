//! Exact word timings from the optional local aligner service, for made speech and for clips
//! of a recording alike.
//!
//! Playback hands over what it gave the player without exact timings ([`Alignable`]). One is
//! aligned at a time. Speech's timings are stored with its cached audio, so it is aligned once;
//! a clip's are kept for this run. Until then (and for words the aligner cannot place) the
//! estimate is used.

use std::collections::VecDeque;

use super::recording::RecordingPlan;
use super::*;

/// The pause between one request and the next.
const IDLE: Duration = Duration::from_millis(150);
/// How long to wait after the aligner could not be reached.
const RETRY: Duration = Duration::from_secs(20);
/// Waiting work kept; the oldest goes first when there is more.
const QUEUE: usize = 32;

/// Audio kept around a recording clip's words when it is aligned.
const RECORDING_PAD_MS: u64 = 200;

/// Something playback gave the player that exact word times can be asked for.
#[derive(Clone)]
pub enum Alignable {
    /// Made speech with cache key `key`, entry `index` of `book`.
    Speech {
        book: String,
        index: usize,
        key: String,
    },
    /// The clip of entry `index` of `book` in its recording.
    Clip {
        book: String,
        index: usize,
        plan: Arc<RecordingPlan>,
    },
}

impl Alignable {
    /// What names it across this run: speech by its cache key, a clip by its sentence and its
    /// stretch of the recording. `None` for a clip with no words heard.
    fn key(&self) -> Option<String> {
        match self {
            Alignable::Speech { key, .. } => Some(key.clone()),
            Alignable::Clip { book, index, plan } => clip_key(book, *index, plan),
        }
    }
}

/// The aligner's work: what waits, what is done, and the service if Aoide started it.
#[derive(Default)]
pub(super) struct Aligning {
    queue: VecDeque<(String, Alignable)>,
    /// Keys aligned or given up on in this run.
    done: HashSet<String>,
    /// Exact word times of recording clips aligned in this run, by key, relative to the clip.
    clips: HashMap<String, Vec<aoide_text::WordEstimate>>,
    child: Option<std::process::Child>,
    launched: bool,
}

/// Names a clip's spoken stretch: the same sentence over the same part of the recording.
fn clip_key(book: &str, index: usize, plan: &RecordingPlan) -> Option<String> {
    let text = plan.clips.get(index)?.text.clone()?;
    Some(format!("{book}:{index}:{}-{}", text.start, text.end))
}

/// The words of `segment` as the text has them.
fn words(segment: &aoide_text::Segment) -> Vec<String> {
    let chars: Vec<char> = segment.display.chars().collect();
    segment
        .words
        .iter()
        .map(|r| {
            chars[r.start - segment.range.start..r.end - segment.range.start]
                .iter()
                .collect()
        })
        .collect()
}

/// What aligning asks of the rest of the app.
pub(super) trait AlignHost: Sync {
    /// The aligner's settings; `None` when there is no aligner.
    fn settings(&self) -> Option<AlignerSettings>;
    fn book(&self, id: &str) -> Result<Arc<BookModel>, String>;
    /// The recording playing for book `id`, if it is.
    fn recording_of(&self, id: &str) -> Option<Arc<RecordingPlan>>;
    /// Exact word times for entry `index` of `book`.
    fn aligned(&self, book: &str, index: usize, timings: Vec<aoide_text::WordEstimate>);
}

/// Asks the aligner service for exact word times, one thing at a time.
pub(super) struct Aligner {
    library: Arc<Mutex<Library>>,
    audio_dir: PathBuf,
    aligning: Mutex<Aligning>,
    /// Told when work is queued.
    queued: tokio::sync::Notify,
}

impl Aligner {
    pub fn new(library: Arc<Mutex<Library>>, audio_dir: PathBuf) -> Self {
        Self {
            library,
            audio_dir,
            aligning: Default::default(),
            queued: tokio::sync::Notify::new(),
        }
    }

    /// How many things wait to be aligned.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        lock(&self.aligning).queue.len()
    }

    /// Asks for exact word times of what playback just gave the player: clips aligned earlier
    /// in this run get theirs at once, the rest wait for the aligner (when there is one).
    pub fn want(&self, wanted: Vec<Alignable>, host: &dyn AlignHost) {
        let configured = host.settings().is_some();
        // The playback and aligning locks are never held together.
        let mut known = Vec::new();
        {
            let mut aligning = lock(&self.aligning);
            for item in wanted {
                let Some(key) = item.key() else { continue };
                if let (Alignable::Clip { book, index, .. }, Some(timings)) =
                    (&item, aligning.clips.get(&key))
                {
                    known.push((book.clone(), *index, timings.clone()));
                    continue;
                }
                if !configured
                    || aligning.done.contains(&key)
                    || aligning.queue.iter().any(|(queued, _)| *queued == key)
                {
                    continue;
                }
                // Playhead-near work first; keep the queue short.
                aligning.queue.push_back((key, item));
                while aligning.queue.len() > QUEUE {
                    aligning.queue.pop_front();
                }
            }
        }
        self.queued.notify_one();
        for (book, index, timings) in known {
            host.aligned(&book, index, timings);
        }
    }

    /// Aligns what waits, one at a time, for as long as Aoide runs.
    pub async fn run(&self, host: &dyn AlignHost) {
        let mut offline_until: Option<Instant> = None;
        loop {
            // Nothing waits: wait for work. The aligner was offline: wait out the retry.
            match offline_until.filter(|until| Instant::now() < *until) {
                Some(until) => tokio::time::sleep_until(until.into()).await,
                None if lock(&self.aligning).queue.is_empty() => self.queued.notified().await,
                None => {}
            }
            self.step(host, &mut offline_until).await;
            tokio::time::sleep(IDLE).await;
        }
    }

    /// Aligns the next thing waiting, unless the aligner was offline until after now.
    pub async fn step(&self, host: &dyn AlignHost, offline_until: &mut Option<Instant>) {
        let Some(settings) = host.settings() else {
            lock(&self.aligning).queue.clear();
            return;
        };
        if offline_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        let Some((key, item)) = lock(&self.aligning).queue.pop_front() else {
            return;
        };
        let aligner = match aoide_aligner::Aligner::new(aoide_aligner::Options {
            base_url: settings.base_url.clone(),
            network: settings.network,
            ..Default::default()
        }) {
            Ok(aligner) => aligner,
            Err(error) => {
                tracing::warn!(event = "align.settings", %error, "Aligner settings are not usable");
                return;
            }
        };
        let started = Instant::now();
        let aligned = match &item {
            Alignable::Speech { book, index, key } => {
                self.align_speech(host, &aligner, book, *index, key).await
            }
            Alignable::Clip { book, index, plan } => {
                self.align_clip(host, &aligner, book, *index, plan, &key)
                    .await
            }
        };
        match aligned {
            Ok(()) => {
                tracing::debug!(event = "align.request", %key, took_ms = crate::logging::took(started), outcome = "ok", "Aligned words");
                lock(&self.aligning).done.insert(key);
            }
            Err(aoide_aligner::Error::Unavailable(_) | aoide_aligner::Error::Timeout) => {
                tracing::info!(event = "align.offline", %key, "The aligner is not answering; trying again later");
                // Try again later; start the service if Aoide may.
                lock(&self.aligning).queue.push_front((key, item));
                *offline_until = Some(Instant::now() + RETRY);
                if let Some(launch) = &settings.launch {
                    self.launch_aligner(launch);
                }
            }
            Err(error) => {
                tracing::warn!(event = "align.error", %key, %error, "Alignment failed");
                lock(&self.aligning).done.insert(key);
            }
        }
    }

    /// Aligns made speech; its timings are stored with its audio.
    async fn align_speech(
        &self,
        host: &dyn AlignHost,
        aligner: &aoide_aligner::Aligner,
        book_id: &str,
        index: usize,
        key: &str,
    ) -> Result<(), aoide_aligner::Error> {
        let other = |m: String| aoide_aligner::Error::InvalidInput(m);
        let Some(mut cached) = lock(&self.library).segment(key).ok().flatten() else {
            return Ok(()); // Evicted meanwhile.
        };
        let book = host.book(book_id).map_err(other)?;
        let segment = book
            .segment(index)
            .ok_or_else(|| other("segment moved".into()))?;
        if cached
            .word_timings
            .as_ref()
            .is_some_and(|t| t.len() == segment.words.len())
        {
            return Ok(());
        }
        let bytes =
            std::fs::read(self.audio_dir.join(&cached.file)).map_err(|e| other(e.to_string()))?;
        let pcm = codec::decode(&bytes).map_err(|e| other(e.to_string()))?;
        let spans = aligner
            .align(&words(segment), &pcm.samples, pcm.sample_rate)
            .await?;
        let fallback = aoide_text::align_to_pauses(segment, &pcm.samples, pcm.sample_rate);
        let timings = aoide_text::merge_alignment(&spans, &fallback);
        cached.word_timings = Some(
            timings
                .iter()
                .map(|t| aoide_library::WordTiming {
                    start_ms: t.start_ms,
                    end_ms: t.end_ms,
                })
                .collect(),
        );
        lock(&self.library)
            .record_segment(&cached)
            .map_err(|e| other(e.to_string()))?;
        host.aligned(book_id, index, timings);
        Ok(())
    }

    /// Exact word times for a clip of a recording, from its spoken stretch decoded straight
    /// from the audiobook. Kept for this run only.
    async fn align_clip(
        &self,
        host: &dyn AlignHost,
        aligner: &aoide_aligner::Aligner,
        book_id: &str,
        index: usize,
        plan: &RecordingPlan,
        key: &str,
    ) -> Result<(), aoide_aligner::Error> {
        let other = |m: String| aoide_aligner::Error::InvalidInput(m);
        // Listening moved on, or the sync map changed under it.
        let current = host.recording_of(book_id);
        if current
            .and_then(|p| clip_key(book_id, index, &p))
            .as_deref()
            != Some(key)
        {
            return Ok(());
        }
        let clip = &plan.clips[index];
        let text = clip.text.clone().ok_or_else(|| other("no text".into()))?;
        let book = host.book(book_id).map_err(other)?;
        let segment = book
            .segment(index)
            .ok_or_else(|| other("segment moved".into()))?;
        let from = text
            .start
            .saturating_sub(RECORDING_PAD_MS)
            .max(clip.audio.start);
        let to = (text.end + RECORDING_PAD_MS).min(clip.audio.end);
        let files = plan.files.clone();
        let samples = tokio::task::spawn_blocking(move || {
            aoide_recording::Reader::new(files).read(from..to, aoide_transcribe::SAMPLE_RATE)
        })
        .await
        .map_err(|e| other(e.to_string()))?
        .map_err(|e| other(e.to_string()))?;
        let spans = aligner
            .align(&words(segment), &samples, aoide_transcribe::SAMPLE_RATE)
            .await?;
        // From the decoded stretch to the clip.
        let shift = (from - clip.audio.start) as u32;
        let spans: Vec<Option<(u32, u32)>> = spans
            .into_iter()
            .map(|span| span.map(|(s, e)| (s + shift, e + shift)))
            .collect();
        let timings = aoide_text::merge_alignment(&spans, &plan.word_times(index));
        lock(&self.aligning)
            .clips
            .insert(key.to_owned(), timings.clone());
        host.aligned(book_id, index, timings);
        Ok(())
    }

    fn launch_aligner(&self, launch: &LaunchCommand) {
        let mut aligning = lock(&self.aligning);
        if aligning.launched {
            return;
        }
        aligning.launched = true;
        let mut command = std::process::Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = &launch.working_dir {
            command.current_dir(dir);
        }
        match command.spawn() {
            Ok(child) => {
                tracing::info!(
                    event = "proc.spawn",
                    program = "aligner",
                    pid = child.id(),
                    outcome = "ok",
                    "Launched the aligner"
                );
                aligning.child = Some(child)
            }
            Err(error) => {
                tracing::error!(event = "proc.spawn", program = "aligner", outcome = "failed", %error, "Could not launch the aligner")
            }
        }
    }

    /// Stops an aligner Aoide started.
    pub fn stop(&self) {
        if let Some(mut child) = lock(&self.aligning).child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Core {
    /// Asks for exact word times of what playback just gave the player.
    pub(super) fn align(&self, wanted: Vec<Alignable>) {
        self.aligner.want(wanted, self)
    }

    /// Stops an aligner Aoide started.
    pub(super) fn stop_aligner(&self) {
        self.aligner.stop()
    }
}

impl AlignHost for Core {
    fn settings(&self) -> Option<AlignerSettings> {
        self.app_settings().aligner
    }

    fn book(&self, id: &str) -> Result<Arc<BookModel>, String> {
        self.load_book(id)
    }

    fn recording_of(&self, id: &str) -> Option<Arc<RecordingPlan>> {
        self.playback.recording_of(id)
    }

    fn aligned(&self, book: &str, index: usize, timings: Vec<aoide_text::WordEstimate>) {
        self.playback.aligned(book, index, timings);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(text: Option<std::ops::Range<u64>>) -> aoide_sync::Clip {
        aoide_sync::Clip {
            audio: 0..10_000,
            text,
            words: vec![],
            off_text: vec![],
        }
    }

    fn alignable(texts: Vec<Option<std::ops::Range<u64>>>, index: usize) -> Alignable {
        Alignable::Clip {
            book: "b".into(),
            index,
            plan: Arc::new(RecordingPlan {
                files: vec![],
                clips: texts.into_iter().map(clip).collect(),
            }),
        }
    }

    #[test]
    fn a_clip_is_aligned_again_only_when_its_stretch_of_the_recording_moves() {
        let before = alignable(vec![Some(100..900), Some(1_000..2_000)], 1);
        let followed = alignable(vec![Some(150..950), Some(1_000..2_000)], 1);
        let moved = alignable(vec![Some(100..900), Some(1_200..2_000)], 1);
        assert_eq!(before.key(), followed.key(), "same sentence, same stretch");
        assert_ne!(before.key(), moved.key());
        assert_eq!(alignable(vec![None], 0).key(), None, "no words heard");
        let speech = Alignable::Speech {
            book: "b".into(),
            index: 1,
            key: "cache-key".into(),
        };
        assert_eq!(speech.key().as_deref(), Some("cache-key"));
    }

    struct Host {
        settings: Option<AlignerSettings>,
    }

    impl AlignHost for Host {
        fn settings(&self) -> Option<AlignerSettings> {
            self.settings.clone()
        }
        fn book(&self, _: &str) -> Result<Arc<BookModel>, String> {
            Err("no books here".into())
        }
        fn recording_of(&self, _: &str) -> Option<Arc<RecordingPlan>> {
            None
        }
        fn aligned(&self, _: &str, _: usize, _: Vec<aoide_text::WordEstimate>) {
            panic!("nothing is aligned in these tests");
        }
    }

    fn speech(key: &str) -> Alignable {
        Alignable::Speech {
            book: "b".into(),
            index: 0,
            key: key.into(),
        }
    }

    fn set_up() -> Option<AlignerSettings> {
        Some(AlignerSettings {
            base_url: "http://127.0.0.1:9".into(),
            network: Default::default(),
            launch: None,
        })
    }

    #[test]
    fn work_waits_only_when_an_aligner_is_set_up_and_only_once() {
        let aligner = Aligner::new(super::super::tests::library_with(&["b"]), "/nowhere".into());
        aligner.want(vec![speech("k")], &Host { settings: None });
        assert_eq!(aligner.waiting(), 0);

        let host = Host { settings: set_up() };
        aligner.want(vec![speech("k"), speech("k")], &host);
        aligner.want(vec![speech("k")], &host);
        assert_eq!(aligner.waiting(), 1);
    }

    #[tokio::test]
    async fn speech_whose_audio_is_gone_is_not_asked_for_again() {
        let aligner = Aligner::new(super::super::tests::library_with(&["b"]), "/nowhere".into());
        let host = Host { settings: set_up() };
        aligner.want(vec![speech("evicted")], &host);

        aligner.step(&host, &mut None).await;
        aligner.want(vec![speech("evicted")], &host);

        assert_eq!(aligner.waiting(), 0);
    }

    #[tokio::test]
    async fn running_it_takes_up_work_as_soon_as_it_is_wanted() {
        let aligner = Arc::new(Aligner::new(
            super::super::tests::library_with(&["b"]),
            "/nowhere".into(),
        ));
        let host = Arc::new(Host { settings: set_up() });
        let running = {
            let (aligner, host) = (Arc::clone(&aligner), Arc::clone(&host));
            tokio::spawn(async move { aligner.run(&*host).await })
        };
        tokio::time::sleep(Duration::from_millis(400)).await;
        aligner.want(vec![speech("evicted")], &*host);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(aligner.waiting(), 0);
        running.abort();
    }
}
