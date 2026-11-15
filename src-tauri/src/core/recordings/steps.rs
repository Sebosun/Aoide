//! The sync job's steps: checking a recording, transcribing it a block at a time (after a quick
//! check that it is the book), and matching what was heard to the text.

use super::*;

/// Recognises speech in 16 kHz samples.
type Hear<'a> = dyn Fn(&[f32]) -> Result<Vec<aoide_transcribe::Word>, aoide_transcribe::Error> + 'a;

impl Recordings {
    /// Checking: works out where the recording stands from what is stored.
    pub(super) fn check_recording(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
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
            live.changes = next_change();
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

    pub(super) fn transcriber_for(
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
        let started = Instant::now();
        let transcriber: Arc<dyn Transcriber> = Arc::from((self.transcribers)(&choice)?);
        match &choice {
            TranscriberChoice::Whisper {
                model,
                use_gpu,
                threads,
            } => tracing::info!(
                event = "sync.transcriber",
                engine = "whisper",
                model = %crate::logging::file(model),
                gpu = use_gpu,
                threads,
                took_ms = crate::logging::took(started),
                "Loaded the speech recognition"
            ),
            TranscriberChoice::Server { model, .. } => tracing::info!(
                event = "sync.transcriber",
                engine = "server",
                %model,
                took_ms = crate::logging::took(started),
                "Using a recognition server"
            ),
        }
        lock(&self.state).transcriber = Some((choice, Arc::clone(&transcriber)));
        Ok(transcriber)
    }

    /// Transcribing: the quick check first, then one block.
    pub(super) fn transcribe_block(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
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

        if done == 0
            && !checked
            && !self.quick_check(host, id, total, &mut decoded.reader, &hear)?
        {
            return Ok(());
        }
        self.next_block(host, id, transcriber.id(), done, total, decoded, &hear)
    }

    /// Transcribes the block of `id` that starts at `done`, reading on from `decoded`, and
    /// stores what was heard. The next block's audio is decoded while this one is recognised.
    #[allow(clippy::too_many_arguments)]
    fn next_block(
        &self,
        host: &dyn SyncHost,
        id: &str,
        transcriber: &str,
        done: u64,
        total: u64,
        mut decoded: Decoded,
        hear: &Hear<'_>,
    ) -> Result<(), String> {
        let block_started = Instant::now();
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
                tracing::info!(
                    event = "sync.abort",
                    book = crate::logging::book(id),
                    from_ms = done,
                    "Stopped during a block"
                );
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
                    transcriber: transcriber.to_owned(),
                    words: serde_json::to_string(&words_json).map_err(|e| e.to_string())?,
                    levels,
                },
            )
            .map_err(|e| e.to_string())?;
        let took_ms = crate::logging::took(block_started);
        tracing::info!(
            event = "sync.block",
            book = crate::logging::book(id),
            from_ms = done,
            to_ms = end,
            of_ms = total,
            words = words_json.len(),
            took_ms,
            rtf = (end - done) as f64 / took_ms.max(1) as f64,
            "Transcribed a block"
        );
        {
            let mut state = lock(&self.state);
            if let Some(live) = state.live.get_mut(id) {
                live.transcribed_ms = end;
                live.changes = next_change();
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

    /// Listens to a few short samples of `id` and looks for them in the book's text, before
    /// transcribing all of it. Returns whether to go on: not when the samples were not found
    /// (the recording is marked so) or the check was stopped.
    fn quick_check(
        &self,
        host: &dyn SyncHost,
        id: &str,
        total: u64,
        reader: &mut Reader,
        hear: &Hear<'_>,
    ) -> Result<bool, String> {
        let book = host.load_book(id)?;
        let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
        let mut found = 0;
        for at in SAMPLE_AT {
            let from = ((total as f64 * at) as u64).min(total.saturating_sub(SAMPLE_MS));
            let to = (from + SAMPLE_MS).min(total);
            let samples = reader
                .read(from..to, SAMPLE_RATE)
                .map_err(|e| e.to_string())?;
            let words = match hear(&samples) {
                Ok(words) => words,
                Err(aoide_transcribe::Error::Aborted) => {
                    tracing::info!(
                        event = "sync.abort",
                        book = crate::logging::book(id),
                        "Stopped during the quick check"
                    );
                    return Ok(false);
                }
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
        tracing::info!(
            event = "sync.check",
            book = crate::logging::book(id),
            found,
            of = SAMPLE_AT.len(),
            passed = found >= 2,
            "Quick check heard the samples"
        );
        if found < 2 {
            self.sync_event(host, id, SyncEvent::SampleFailed, None);
            host.checked(id, false);
            return Ok(false);
        }
        if let Some(live) = lock(&self.state).live.get_mut(id) {
            live.sample_checked = true;
        }
        host.checked(id, true);
        Ok(true)
    }

    pub(super) fn heard_words(&self, id: &str) -> Result<Vec<HeardWord>, String> {
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
    pub(super) fn match_recording(&self, host: &dyn SyncHost, id: &str) -> Result<(), String> {
        let (row, _) = self.stored_recording(id)?;
        let book = host.load_book(id)?;
        let texts: Vec<&str> = book.chapters.iter().map(|c| c.text.as_str()).collect();
        let heard = self.heard_words(id)?;
        let started = Instant::now();
        let SyncMap { chapters, report } = match_book(&texts, &heard);
        let review = poor(&report) && !row.accepted;
        tracing::info!(
            event = "sync.match",
            book = crate::logging::book(id),
            words = report.words,
            matched_pct = report.matched_words * 100 / report.words.max(1),
            review,
            took_ms = crate::logging::took(started),
            "Matched the transcript to the text"
        );
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
}
