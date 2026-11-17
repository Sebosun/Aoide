//! Exact word timings from the optional local aligner service, for made speech and for clips
//! of a recording alike.
//!
//! Playback hands over what it gave the player without exact timings ([`Alignable`]). One is
//! aligned at a time. Speech's timings are stored with its cached audio, so it is aligned once;
//! a clip's are kept for this run. Until then (and for words the aligner cannot place) the
//! estimate is used.

use std::collections::VecDeque;

use super::engine_process::{
    Answer, Names, ServerHost, ServerProcess, ServerState, ServerView, Spec, Timing,
};
use super::recording::RecordingPlan;
use super::*;

/// The pause between one request and the next.
const IDLE: Duration = Duration::from_millis(150);
/// How long to wait after the aligner could not be reached.
const RETRY: Duration = Duration::from_secs(20);
/// How often a service Aoide is starting is asked whether it is ready.
const STARTING: Duration = Duration::from_secs(2);
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
    /// How book `id`'s words are said (its pronunciations), so the aligner hears them as said.
    fn rules(&self, id: &str) -> Arc<[aoide_text::Rule]>;
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
    /// The aligner service Aoide starts, when the settings give a launch command.
    server: ServerProcess,
}

impl Aligner {
    /// Its pidfile goes in `data_dir`, the service's log in `logs_dir`.
    pub fn new(
        library: Arc<Mutex<Library>>,
        audio_dir: PathBuf,
        data_dir: &Path,
        logs_dir: &Path,
    ) -> Self {
        Self {
            library,
            audio_dir,
            aligning: Default::default(),
            queued: tokio::sync::Notify::new(),
            server: ServerProcess::new(Names::ALIGNER, data_dir, logs_dir, None, Timing::default()),
        }
    }

    /// The service Aoide starts for the settings' launch command, as it is now.
    #[cfg(test)]
    pub fn server_view(&self) -> Option<ServerView> {
        self.server.view()
    }

    /// Keeps the service Aoide starts in step with the settings.
    fn follow(&self, settings: Option<&AlignerSettings>) {
        self.server
            .follow(settings.and_then(|s| Spec::of(&s.launch, &s.base_url)));
    }

    /// Runs the service Aoide starts (launches, exits, idle stops) for as long as Aoide runs.
    pub async fn serve(&self) {
        self.server.run(&AlignerServer(self)).await
    }

    /// How many things wait to be aligned.
    #[cfg(test)]
    pub fn waiting(&self) -> usize {
        lock(&self.aligning).queue.len()
    }

    /// Asks for exact word times of what playback just gave the player: clips aligned earlier
    /// in this run get theirs at once, the rest wait for the aligner (when there is one).
    pub fn want(&self, wanted: Vec<Alignable>, host: &dyn AlignHost) {
        let settings = host.settings();
        self.follow(settings.as_ref());
        let configured = settings.is_some();
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
        if !lock(&self.aligning).queue.is_empty() {
            self.server.want();
        }
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
        let settings = host.settings();
        self.follow(settings.as_ref());
        let Some(settings) = settings else {
            lock(&self.aligning).queue.clear();
            return;
        };
        if offline_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        if lock(&self.aligning).queue.is_empty() {
            return;
        }
        let aligner = match aoide_aligner::Aligner::new(aoide_aligner::Options {
            base_url: settings.base_url.clone(),
            network: settings.network,
            ..Default::default()
        }) {
            Ok(aligner) => aligner,
            Err(error) => {
                tracing::warn!(event = "align.settings", %error, "Aligner settings are not usable");
                lock(&self.aligning).queue.pop_front(); // As each would fail.
                return;
            }
        };
        // A service Aoide starts and that does not run yet: ask whether it answers, which
        // starts it when it does not.
        let starting = self
            .server
            .view()
            .is_some_and(|v| !matches!(v.state, ServerState::Running | ServerState::External));
        if starting {
            let answer = match aligner.ready().await {
                Ok(true) => Answer::Ready,
                Ok(false) => Answer::Loading,
                Err(_) => Answer::Offline,
            };
            self.server.answered(answer);
            if answer != Answer::Ready {
                *offline_until = Some(Instant::now() + STARTING);
                return;
            }
        }
        let Some((key, item)) = lock(&self.aligning).queue.pop_front() else {
            return;
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
                self.server.used();
            }
            Err(aoide_aligner::Error::Unavailable(_) | aoide_aligner::Error::Timeout) => {
                tracing::info!(event = "align.offline", %key, "The aligner is not answering; trying again later");
                // Try again later; start the service if Aoide may.
                lock(&self.aligning).queue.push_front((key, item));
                *offline_until = Some(Instant::now() + RETRY);
                self.server.answered(Answer::Offline);
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
        // The aligner is given the words as said, and their spans are folded back onto the
        // written words: one timing per word of the page.
        let written = words(segment);
        let spoken = aoide_text::pronounce(&written.join(" "), &host.rules(book_id));
        let said: Vec<String> = spoken.text.split_whitespace().map(str::to_owned).collect();
        let spans = aligner.align(&said, &pcm.samples, pcm.sample_rate).await?;
        let spans = spoken.fold(&spans, written.len());
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

    /// Stops an aligner Aoide started.
    pub fn stop(&self) {
        self.server.stop(&AlignerServer(self))
    }
}

/// The aligner service's view of the rest of the app: it is wanted while work waits.
struct AlignerServer<'a>(&'a Aligner);

impl ServerHost for AlignerServer<'_> {
    fn now_ms(&self) -> i64 {
        super::now_ms()
    }

    fn wanted(&self) -> bool {
        !lock(&self.0.aligning).queue.is_empty()
    }

    fn server_changed(&self, _view: Option<ServerView>) {}
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

    fn rules(&self, id: &str) -> Arc<[aoide_text::Rule]> {
        self.pronunciations.rules(id).unwrap_or_else(|error| {
            tracing::warn!(event = "align.rules", book = crate::logging::book(id), %error, "Cannot read the pronunciations");
            Arc::new([])
        })
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
        fn rules(&self, _: &str) -> Arc<[aoide_text::Rule]> {
            Arc::new([])
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
        let aligner = Aligner::new(
            super::super::tests::library_with(&["b"]),
            "/nowhere".into(),
            Path::new("/nowhere"),
            Path::new("/nowhere"),
        );
        aligner.want(vec![speech("k")], &Host { settings: None });
        assert_eq!(aligner.waiting(), 0);

        let host = Host { settings: set_up() };
        aligner.want(vec![speech("k"), speech("k")], &host);
        aligner.want(vec![speech("k")], &host);
        assert_eq!(aligner.waiting(), 1);
    }

    #[tokio::test]
    async fn speech_whose_audio_is_gone_is_not_asked_for_again() {
        let aligner = Aligner::new(
            super::super::tests::library_with(&["b"]),
            "/nowhere".into(),
            Path::new("/nowhere"),
            Path::new("/nowhere"),
        );
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
            Path::new("/nowhere"),
            Path::new("/nowhere"),
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

    /// An aligner that places each word it is given 100 ms after the one before; it keeps the
    /// words of each request.
    async fn scripted_aligner() -> (String, Arc<Mutex<Vec<Vec<String>>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&asked);
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0u8; 65536];
                let body = loop {
                    let n = socket.read(&mut buffer).await.unwrap_or(0);
                    request.extend_from_slice(&buffer[..n]);
                    let text = String::from_utf8_lossy(&request).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length: usize = text
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length || n == 0 {
                            break request[end + 4..].to_vec();
                        }
                    }
                };
                let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let words: Vec<String> =
                    serde_json::from_value(json["words"].clone()).unwrap_or_default();
                let spans: Vec<[u32; 2]> = (0..words.len() as u32)
                    .map(|i| [i * 100, i * 100 + 100])
                    .collect();
                lock(&kept).push(words);
                let reply = serde_json::json!({ "words": spans }).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (address, asked)
    }

    /// A host with one book, said with `rules`; it keeps what was aligned.
    struct Speaking {
        settings: Option<AlignerSettings>,
        book: Arc<BookModel>,
        rules: Arc<[aoide_text::Rule]>,
        aligned: Mutex<Vec<(usize, Vec<aoide_text::WordEstimate>)>>,
    }

    impl AlignHost for Speaking {
        fn settings(&self) -> Option<AlignerSettings> {
            self.settings.clone()
        }
        fn book(&self, _: &str) -> Result<Arc<BookModel>, String> {
            Ok(Arc::clone(&self.book))
        }
        fn rules(&self, _: &str) -> Arc<[aoide_text::Rule]> {
            Arc::clone(&self.rules)
        }
        fn recording_of(&self, _: &str) -> Option<Arc<RecordingPlan>> {
            None
        }
        fn aligned(&self, _: &str, index: usize, timings: Vec<aoide_text::WordEstimate>) {
            lock(&self.aligned).push((index, timings));
        }
    }

    #[tokio::test]
    async fn speech_said_another_way_is_aligned_by_its_spoken_words_and_kept_per_written_word() {
        let dir = tempfile::tempdir().unwrap();
        let library = super::super::tests::library_with(&["b"]);
        let book = Arc::new(
            BookModel::parse("b", &super::super::tests::epub(), &Default::default()).unwrap(),
        );
        let index = (0..book.total_entries())
            .find(|i| book.segment(*i).unwrap().display.starts_with("Gulls"))
            .unwrap();
        let audio = dir.path().join("audio");
        std::fs::create_dir_all(&audio).unwrap();
        let bytes = codec::encode(&aoide_audio::Pcm {
            sample_rate: 24_000,
            samples: vec![0.2; 24_000],
        })
        .unwrap();
        std::fs::write(audio.join("k.ogg"), &bytes).unwrap();
        lock(&library)
            .record_segment(&CachedSegment {
                key: "k".into(),
                book_id: BookId("b".into()),
                file: "k.ogg".into(),
                duration_ms: 1_000,
                bytes: bytes.len() as u64,
                word_timings: None,
                created_at: 0,
            })
            .unwrap();
        let (base_url, asked) = scripted_aligner().await;
        let aligner = Aligner::new(
            Arc::clone(&library),
            audio,
            dir.path(),
            &dir.path().join("logs"),
        );
        let host = Speaking {
            settings: Some(AlignerSettings {
                base_url,
                network: Default::default(),
                launch: None,
            }),
            book: Arc::clone(&book),
            rules: Arc::new([aoide_text::Rule {
                written: "gulls".into(),
                spoken: "the sea gulls".into(),
                match_case: false,
            }]),
            aligned: Mutex::new(Vec::new()),
        };

        aligner.want(
            vec![Alignable::Speech {
                book: "b".into(),
                index,
                key: "k".into(),
            }],
            &host,
        );
        aligner.step(&host, &mut None).await;

        let asked = lock(&asked).clone();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0][..4], ["the", "sea", "gulls", "argued"]);
        let written = book.segment(index).unwrap().words.len();
        assert_eq!(asked[0].len(), written + 2);
        // One timing per written word: "Gulls" spans the three spoken words.
        let stored = lock(&library)
            .segment("k")
            .unwrap()
            .unwrap()
            .word_timings
            .unwrap();
        assert_eq!(stored.len(), written);
        let span = |t: &aoide_library::WordTiming| (t.start_ms, t.end_ms);
        assert_eq!(span(&stored[0]), (0, 300));
        assert_eq!(span(&stored[1]), (300, 400));
        assert_eq!(
            span(&stored[written - 1]),
            (written as u32 * 100 + 100, written as u32 * 100 + 200)
        );
        let aligned = lock(&host.aligned);
        assert_eq!(aligned.len(), 1);
        assert_eq!((aligned[0].0, aligned[0].1.len()), (index, written));
    }

    fn launching(script: &str) -> Option<AlignerSettings> {
        Some(AlignerSettings {
            launch: Some(super::super::engine_process::process_tests::script(script)),
            ..set_up().unwrap()
        })
    }

    fn aligner_over(dir: &std::path::Path) -> Arc<Aligner> {
        Arc::new(Aligner::new(
            super::super::tests::library_with(&["b"]),
            "/nowhere".into(),
            dir,
            &dir.join("logs"),
        ))
    }

    #[tokio::test]
    async fn an_aligner_aoide_starts_runs_as_its_server_and_is_stopped_with_its_group() {
        use super::super::engine_process::process_tests::{alive_in, until};
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let aligner = aligner_over(dir.path());
        let host = Arc::new(Host {
            settings: launching(&format!("echo $$ > '{}'; exec sleep 300", pid.display())),
        });
        let serving = {
            let aligner = Arc::clone(&aligner);
            tokio::spawn(async move { aligner.serve().await })
        };

        aligner.want(vec![speech("k")], &*host);
        aligner.step(&*host, &mut None).await;
        until("the aligner to be launched", || {
            alive_in(&pid) == Some(true)
        })
        .await;
        assert!(dir.path().join("logs/aligner.log").exists());

        aligner.stop();
        assert_eq!(alive_in(&pid), Some(false));
        serving.abort();
    }

    #[tokio::test]
    async fn an_aligner_that_ends_at_once_is_noticed_and_not_launched_again() {
        use super::super::engine_process::process_tests::until;
        use super::super::engine_process::ServerState;
        let dir = tempfile::tempdir().unwrap();
        let marks = dir.path().join("launched");
        let aligner = aligner_over(dir.path());
        let host = Arc::new(Host {
            settings: launching(&format!("echo x >> '{}'; exit 3", marks.display())),
        });
        let serving = {
            let aligner = Arc::clone(&aligner);
            tokio::spawn(async move { aligner.serve().await })
        };

        aligner.want(vec![speech("k")], &*host);
        aligner.step(&*host, &mut None).await;
        until("the failure", || {
            aligner.server_view().map(|v| v.state) == Some(ServerState::Failed)
        })
        .await;
        aligner.step(&*host, &mut None).await;
        tokio::time::sleep(Duration::from_millis(200)).await;

        assert_eq!(aligner.server_view().unwrap().exit_code, Some(3));
        let launched = std::fs::read_to_string(&marks).unwrap();
        assert_eq!(launched.lines().count(), 1);
        serving.abort();
    }
}
