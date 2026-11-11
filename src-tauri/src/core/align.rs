//! Exact word timings from the optional local aligner service.
//!
//! Segments are queued when they are loaded for playback and have no stored timings. One
//! segment is aligned at a time; results are stored with the cached audio, so each segment is
//! aligned once. Until then (and for words the aligner cannot place) the pause-fitted estimate
//! is used.

use super::*;

const IDLE: Duration = Duration::from_millis(150);
/// How long to wait after the aligner could not be reached.
const RETRY: Duration = Duration::from_secs(20);

impl Core {
    /// Queues a provided segment for alignment if an aligner is configured.
    pub(super) fn queue_alignment(&self, book: &str, index: usize, key: &str) {
        let mut extras = lock(&self.extras);
        if extras.align_done.contains(key) || extras.align_queue.iter().any(|q| q.2 == key) {
            return;
        }
        // Playhead-near work first; keep the queue short.
        extras
            .align_queue
            .push_back((book.to_owned(), index, key.to_owned()));
        while extras.align_queue.len() > 32 {
            extras.align_queue.pop_front();
        }
    }

    pub(super) async fn aligner_loop(self: Arc<Self>) {
        let mut offline_until: Option<Instant> = None;
        loop {
            tokio::time::sleep(IDLE).await;
            let Some(settings) = self.app_settings().aligner else {
                lock(&self.extras).align_queue.clear();
                continue;
            };
            if offline_until.is_some_and(|t| Instant::now() < t) {
                continue;
            }
            let Some((book_id, index, key)) = lock(&self.extras).align_queue.pop_front() else {
                continue;
            };
            let aligner = match aoide_aligner::Aligner::new(aoide_aligner::Options {
                base_url: settings.base_url.clone(),
                network: settings.network,
                ..Default::default()
            }) {
                Ok(aligner) => aligner,
                Err(error) => {
                    eprintln!("aoide: aligner settings: {error}");
                    continue;
                }
            };
            match self.align_one(&aligner, &book_id, index, &key).await {
                Ok(()) => {
                    lock(&self.extras).align_done.insert(key);
                }
                Err(aoide_aligner::Error::Unavailable(_) | aoide_aligner::Error::Timeout) => {
                    // Try again later; start the service if Aoide may.
                    lock(&self.extras)
                        .align_queue
                        .push_front((book_id, index, key));
                    offline_until = Some(Instant::now() + RETRY);
                    if let Some(launch) = &settings.launch {
                        self.launch_aligner(launch);
                    }
                }
                Err(error) => {
                    eprintln!("aoide: alignment failed for {key}: {error}");
                    lock(&self.extras).align_done.insert(key);
                }
            }
        }
    }

    async fn align_one(
        &self,
        aligner: &aoide_aligner::Aligner,
        book_id: &str,
        index: usize,
        key: &str,
    ) -> Result<(), aoide_aligner::Error> {
        let other = |m: String| aoide_aligner::Error::InvalidInput(m);
        let Some(mut cached) = lock(&self.library).segment(key).ok().flatten() else {
            return Ok(()); // Evicted meanwhile.
        };
        let book = self.load_book(book_id).map_err(other)?;
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
        let bytes = std::fs::read(self.paths.audio_dir.join(&cached.file))
            .map_err(|e| other(e.to_string()))?;
        let pcm = codec::decode(&bytes).map_err(|e| other(e.to_string()))?;
        let chars: Vec<char> = segment.display.chars().collect();
        let words: Vec<String> = segment
            .words
            .iter()
            .map(|r| {
                chars[r.start - segment.range.start..r.end - segment.range.start]
                    .iter()
                    .collect()
            })
            .collect();
        let spans = aligner.align(&words, &pcm.samples, pcm.sample_rate).await?;
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
        if let Some(session) = lock(&self.session)
            .as_mut()
            .filter(|s| s.target.book.id == book_id)
        {
            session.aligned(index, timings);
        }
        Ok(())
    }

    fn launch_aligner(&self, launch: &LaunchCommand) {
        let mut extras = lock(&self.extras);
        if extras.aligner_launched {
            return;
        }
        extras.aligner_launched = true;
        let mut command = std::process::Command::new(&launch.program);
        command
            .args(&launch.args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = &launch.working_dir {
            command.current_dir(dir);
        }
        match command.spawn() {
            Ok(child) => extras.aligner_child = Some(child),
            Err(error) => eprintln!("aoide: could not launch the aligner: {error}"),
        }
    }

    /// Stops an aligner Aoide started.
    pub(super) fn stop_aligner(&self) {
        if let Some(mut child) = lock(&self.extras).aligner_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
