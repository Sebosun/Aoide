//! Inspecting audio files and attaching them to a book, or adding them as an audiobook of their own.

use super::*;

impl Recordings {
    pub(in crate::core) fn inspect_recording(&self, paths: Vec<String>) -> InspectionView {
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
    pub(in crate::core) fn attach_recording(
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
    pub(in crate::core) fn attach_recording_as_listed(
        &self,
        host: &dyn SyncHost,
        id: &str,
        files: Vec<String>,
    ) -> Result<RecordingView, String> {
        let files: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
        self.attach(host, id, aoide_recording::inspect_as_listed(&files))
    }

    pub(super) fn attach(
        &self,
        host: &dyn SyncHost,
        id: &str,
        found: aoide_recording::Inspection,
    ) -> Result<RecordingView, String> {
        self.store(host, id, found, SyncStage::Checking)
    }

    /// Keeps `found` as the recording of audiobook `id`, which has no text: it plays as it is
    /// and is not synced (stage `Standalone`) until it is given an EPUB.
    pub(in crate::core) fn add_standalone(
        &self,
        host: &dyn SyncHost,
        id: &str,
        found: aoide_recording::Inspection,
    ) -> Result<RecordingView, String> {
        self.store(host, id, found, SyncStage::Standalone)
    }

    /// Why `found` cannot be a recording, in the words the pairing uses: a file is protected,
    /// unreadable or of an unknown kind, or there is no audio at all.
    pub(in crate::core) fn refusal(found: &aoide_recording::Inspection) -> Option<String> {
        if let Some(problem) = found.problems.first() {
            return Some(format!(
                "{}: {}",
                file_name(&problem.path),
                problem_text(&problem.problem)
            ));
        }
        found
            .files
            .is_empty()
            .then(|| "No audio files were found there.".into())
    }

    /// Stores `found` as item `id`'s recording, in `stage`.
    pub(super) fn store(
        &self,
        host: &dyn SyncHost,
        id: &str,
        found: aoide_recording::Inspection,
        stage: SyncStage,
    ) -> Result<RecordingView, String> {
        let book_id = BookId(id.into());
        lock(&self.library)
            .book(&book_id)
            .map_err(|e| e.to_string())?
            .ok_or("book not found")?;
        if let Some(refusal) = Self::refusal(&found) {
            return Err(refusal);
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
            parts: parts_of(&found),
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
                    stage: stage.name().into(),
                    error: None,
                    accepted: false,
                    added_at: now,
                })
                .map_err(|e| e.to_string())?;
        }
        lock(&self.state).live.insert(
            id.to_owned(),
            Live {
                stage,
                added_at: now,
                total_ms: found.total_ms(),
                transcribed_ms: 0,
                pace_from: None,
                abort: Arc::new(AtomicBool::new(false)),
                sample_checked: false,
                changes: next_change(),
            },
        );
        host.emit(CoreEvent::LibraryChanged);
        self.emit_recording(host, id);
        if stage != SyncStage::Standalone {
            self.wake_sync();
        }
        self.view(host, id)
            .ok_or_else(|| "the recording was not saved".into())
    }
}
