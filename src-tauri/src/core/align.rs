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

/// Audio kept around a recording clip's words when it is aligned.
const RECORDING_PAD_MS: u64 = 200;

impl Core {
    /// Recording clips just given to the player: word times aligned earlier in this run apply
    /// at once; the rest are queued for the aligner.
    pub(super) fn align_recording_clips(&self) {
        let (book, plan, fresh) = {
            let mut session = lock(&self.session);
            let Some(session) = session.as_mut() else {
                return;
            };
            let fresh = session.take_to_align();
            let Some(plan) = session.recording().cloned() else {
                return;
            };
            (session.target.book.id.clone(), plan, fresh)
        };
        if fresh.is_empty() {
            return;
        }
        let keyed: Vec<(usize, String)> = fresh
            .into_iter()
            .filter_map(|index| Some((index, plan.align_key(&book, index)?)))
            .collect();
        // The session and extras locks are never held together.
        let (known, unknown): (Vec<_>, Vec<_>) = {
            let extras = lock(&self.extras);
            keyed
                .into_iter()
                .map(|(index, key)| {
                    let timings = extras.recording_aligned.get(&key).cloned();
                    (index, key, timings)
                })
                .partition(|(_, _, timings)| timings.is_some())
        };
        if let Some(session) = lock(&self.session)
            .as_mut()
            .filter(|s| s.target.book.id == book)
        {
            for (index, _, timings) in known {
                session.aligned(index, timings.unwrap_or_default());
            }
        }
        if self.app_settings().aligner.is_some() {
            for (index, key, _) in unknown {
                self.queue_alignment(&book, index, &key);
            }
        }
    }

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
        if key.starts_with("rec:") {
            return self.align_recording(aligner, book_id, index, key).await;
        }
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

    /// Exact word times for a clip of the recording being listened to, from its spoken stretch
    /// decoded straight from the audiobook. Kept for this run only.
    async fn align_recording(
        &self,
        aligner: &aoide_aligner::Aligner,
        book_id: &str,
        index: usize,
        key: &str,
    ) -> Result<(), aoide_aligner::Error> {
        let other = |m: String| aoide_aligner::Error::InvalidInput(m);
        let plan = lock(&self.session)
            .as_ref()
            .filter(|s| s.target.book.id == book_id)
            .and_then(|s| s.recording().cloned());
        // Listening moved on, or the sync map changed under it.
        let Some(plan) = plan.filter(|p| p.align_key(book_id, index).as_deref() == Some(key))
        else {
            return Ok(());
        };
        let clip = &plan.clips[index];
        let text = clip.text.clone().ok_or_else(|| other("no text".into()))?;
        let book = self.load_book(book_id).map_err(other)?;
        let segment = book
            .segment(index)
            .ok_or_else(|| other("segment moved".into()))?;
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
            .align(&words, &samples, aoide_transcribe::SAMPLE_RATE)
            .await?;
        // From the decoded stretch to the clip.
        let shift = (from - clip.audio.start) as u32;
        let spans: Vec<Option<(u32, u32)>> = spans
            .into_iter()
            .map(|span| span.map(|(s, e)| (s + shift, e + shift)))
            .collect();
        let timings = aoide_text::merge_alignment(&spans, &plan.word_times(index));
        lock(&self.extras)
            .recording_aligned
            .insert(key.to_owned(), timings.clone());
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
