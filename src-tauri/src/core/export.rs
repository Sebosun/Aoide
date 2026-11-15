//! M4B audiobook export: generates any missing audio, then streams the book's cached segments
//! through ffmpeg with chapter markers and the cover.

use super::*;

use std::io::Write;
use std::process::{Command, Stdio};

use super::recording::RecordingPlan;

/// Sample rate of the exported audio.
const EXPORT_RATE: u32 = 24_000;

/// Where an M4B export is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum ExportStage {
    Generating,
    Encoding,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ExportProgress {
    pub book_id: String,
    pub stage: ExportStage,
    pub done: usize,
    pub total: usize,
    pub path: Option<String>,
    pub error: Option<String>,
}

/// A chapter mark in milliseconds from the start of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChapterMark {
    pub title: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '=' | ';' | '#' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// An ffmpeg `FFMETADATA1` document with book tags and chapters.
pub fn ffmetadata(
    title: &str,
    authors: &[String],
    comment: &str,
    chapters: &[ChapterMark],
) -> String {
    let artist = authors.join(", ");
    let mut out = String::from(";FFMETADATA1\n");
    out.push_str(&format!("title={}\n", escape(title)));
    out.push_str(&format!("album={}\n", escape(title)));
    if !artist.is_empty() {
        out.push_str(&format!("artist={}\n", escape(&artist)));
        out.push_str(&format!("album_artist={}\n", escape(&artist)));
    }
    out.push_str("genre=Audiobook\n");
    out.push_str(&format!("comment={}\n", escape(comment)));
    for chapter in chapters {
        out.push_str(&format!(
            "\n[CHAPTER]\nTIMEBASE=1/1000\nSTART={}\nEND={}\ntitle={}\n",
            chapter.start_ms,
            chapter.end_ms,
            escape(&chapter.title)
        ));
    }
    out
}

/// A safe file name for the exported book.
pub fn file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_start_matches('.');
    let stem: String = if cleaned.is_empty() {
        "Audiobook"
    } else {
        cleaned
    }
    .chars()
    .take(120)
    .collect();
    format!("{stem}.m4b")
}

/// Samples for `ms` milliseconds at the export rate.
fn samples_for(ms: u64) -> usize {
    (ms * u64::from(EXPORT_RATE)).div_ceil(1000) as usize
}

/// What exporting asks of the rest of the app.
pub(super) trait ExportHost: Sync {
    /// Makes all of book `id`'s speech (resuming its preparation if paused).
    fn prepare<'a>(&'a self, id: &'a str) -> aoide_tts::BoxFuture<'a, Result<(), String>>;
    fn prepare_view(&self, id: &str) -> Option<PrepareView>;
    /// Book `id` and the speech that reads it.
    fn speech_plan(&self, id: &str) -> Result<(Arc<BookModel>, Plan), String>;
    /// The speech engine's name, for the audiobook's credits.
    fn engine_name(&self) -> String;
    /// Book `id` and its synced recording.
    fn recording_plan(&self, id: &str) -> Result<(Arc<BookModel>, Arc<RecordingPlan>), String>;
}

/// Writes export files, on a blocking thread.
pub(super) struct Encoder {
    pub(super) library: Arc<Mutex<Library>>,
    library_dir: PathBuf,
    audio_dir: PathBuf,
    events: Arc<dyn Events>,
}

/// Exports books as audiobooks and read-along EPUBs, one at a time.
pub(super) struct Exports {
    encoder: Arc<Encoder>,
    /// The book being exported.
    exporting: Mutex<Option<String>>,
}

/// Holds the export slot; dropping it frees the slot.
struct Slot<'a>(&'a Mutex<Option<String>>);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *lock(self.0) = None;
    }
}

impl Exports {
    pub fn new(library: Arc<Mutex<Library>>, paths: &Paths, events: Arc<dyn Events>) -> Self {
        Self {
            encoder: Arc::new(Encoder {
                library,
                library_dir: paths.library_dir.clone(),
                audio_dir: paths.audio_dir.clone(),
                events,
            }),
            exporting: Mutex::new(None),
        }
    }

    /// The book being exported, if any.
    pub fn exporting(&self) -> Option<String> {
        lock(&self.exporting).clone()
    }

    fn claim(&self, id: &str) -> Result<Slot<'_>, String> {
        let mut exporting = lock(&self.exporting);
        if exporting.is_some() {
            return Err("another export is already running".into());
        }
        *exporting = Some(id.to_owned());
        Ok(Slot(&self.exporting))
    }

    /// Exports `id` as an M4B into the Exports folder next to the library folder, making its
    /// speech first.
    pub async fn audiobook(&self, id: &str, host: &dyn ExportHost) -> Result<String, String> {
        let result = {
            let _slot = self.claim(id)?;
            self.make_audiobook(id, host).await
        };
        self.encoder.finished(id, &result);
        result
    }

    async fn make_audiobook(&self, id: &str, host: &dyn ExportHost) -> Result<String, String> {
        which_ffmpeg()?;
        // Generate everything first, reusing whole-book preparation.
        host.prepare(id).await?;
        loop {
            match host.prepare_view(id) {
                Some(view) if view.state == PrepareStage::Done => break,
                Some(view) => {
                    self.encoder
                        .progress(id, ExportStage::Generating, view.done, view.total)
                }
                None => return Err("export cancelled: preparation was stopped".into()),
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
        let (book, plan) = host.speech_plan(id)?;
        if let Some(missing) = plan.items.iter().find(|i| i.speakable && !i.cached) {
            return Err(format!("audio for {} is missing; try again", missing.key));
        }
        let encoder = Arc::clone(&self.encoder);
        let engine = host.engine_name();
        tokio::task::spawn_blocking(move || encoder.encode_m4b(&book, &plan, &engine))
            .await
            .map_err(|e| e.to_string())?
    }

    /// Exports `id` with its synced recording as a read-along EPUB into the Exports folder.
    pub async fn read_along(&self, id: &str, host: &dyn ExportHost) -> Result<String, String> {
        let result = {
            let _slot = self.claim(id)?;
            match host.recording_plan(id) {
                Ok((book, plan)) => {
                    let encoder = Arc::clone(&self.encoder);
                    tokio::task::spawn_blocking(move || encoder.write_read_along(&book, &plan))
                        .await
                        .map_err(|e| e.to_string())
                        .and_then(|r| r)
                }
                Err(error) => Err(error),
            }
        };
        self.encoder.finished(id, &result);
        result
    }
}

impl Encoder {
    /// The Exports folder, next to the library folder.
    pub(super) fn folder(&self) -> PathBuf {
        self.library_dir
            .parent()
            .unwrap_or(&self.library_dir)
            .join("Exports")
    }

    pub(super) fn progress(&self, id: &str, stage: ExportStage, done: usize, total: usize) {
        self.send(ExportProgress {
            book_id: id.into(),
            stage,
            done,
            total,
            path: None,
            error: None,
        });
    }

    /// Tells the window how the export of `id` ended.
    fn finished(&self, id: &str, result: &Result<String, String>) {
        self.send(ExportProgress {
            book_id: id.into(),
            stage: if result.is_ok() {
                ExportStage::Done
            } else {
                ExportStage::Error
            },
            done: usize::from(result.is_ok()),
            total: usize::from(result.is_ok()),
            path: result.as_ref().ok().cloned(),
            error: result.as_ref().err().cloned(),
        });
    }

    fn send(&self, progress: ExportProgress) {
        let event = CoreEvent::ExportProgress(progress);
        match event.payload() {
            Ok(value) => self.events.emit(event.name(), value),
            Err(error) => {
                tracing::error!(event = "app.emit", name = event.name(), %error, "Cannot send an event to the window")
            }
        }
    }

    /// Writes `book` as an M4B from the audio `plan` lists, said by `engine`. Returns its path.
    fn encode_m4b(&self, book: &BookModel, plan: &Plan, engine: &str) -> Result<String, String> {
        let id = book.id.as_str();
        // Exact per-entry lengths so chapter marks match the audio.
        let segments: Vec<Option<CachedSegment>> = {
            let library = lock(&self.library);
            plan.items
                .iter()
                .map(|item| {
                    item.speakable
                        .then(|| library.segment(&item.key).ok().flatten())
                        .flatten()
                })
                .collect()
        };
        let mut chapters = Vec::new();
        let mut cursor_ms = 0u64;
        for (index, chapter) in book.chapters.iter().enumerate() {
            let start = cursor_ms;
            for local in 0..chapter.segments.len() {
                let global = chapter.first_entry + local;
                let audio = segments[global].as_ref().map_or(0, |s| s.duration_ms);
                cursor_ms += audio + u64::from(chapter.segments[local].pause_after_ms);
            }
            if cursor_ms > start {
                chapters.push(ChapterMark {
                    title: if chapter.title.trim().is_empty() {
                        format!("Chapter {}", index + 1)
                    } else {
                        chapter.title.clone()
                    },
                    start_ms: start,
                    end_ms: cursor_ms,
                });
            }
        }
        if chapters.is_empty() {
            return Err("the book has no audio to export".into());
        }

        let exports = self.folder();
        std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
        let output = exports.join(file_name(&book.title));
        let partial = output.with_extension("m4b.part");
        let work = std::env::temp_dir().join(format!("aoide-export-{id}"));
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let metadata_path = work.join("chapters.txt");
        std::fs::write(
            &metadata_path,
            ffmetadata(
                &book.title,
                &book.authors,
                &format!("Narrated by Aoide using {engine}"),
                &chapters,
            ),
        )
        .map_err(|e| e.to_string())?;
        let cover = lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .and_then(|r| r.cover_path)
            .filter(|p| {
                p.exists()
                    && matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("jpg" | "jpeg" | "png")
                    )
            });

        let mut command = Command::new("ffmpeg");
        command.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "s16le"]);
        command.args(["-ar", &EXPORT_RATE.to_string(), "-ac", "1", "-i", "pipe:0"]);
        command.arg("-i").arg(&metadata_path);
        if let Some(cover) = &cover {
            command.arg("-i").arg(cover);
        }
        command.args(["-map", "0:a", "-map_metadata", "1", "-map_chapters", "1"]);
        if cover.is_some() {
            command.args([
                "-map",
                "2:v",
                "-c:v",
                "copy",
                "-disposition:v:0",
                "attached_pic",
            ]);
        }
        command.args([
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-movflags",
            "+faststart",
            "-f",
            "ipod",
        ]);
        command.arg(&partial);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
        let mut stdin = child.stdin.take().ok_or("ffmpeg has no input")?;

        let write_result = (|| -> Result<(), String> {
            let total = book.chapters.len();
            for (index, chapter) in book.chapters.iter().enumerate() {
                self.progress(id, ExportStage::Encoding, index, total);
                let mut pcm: Vec<i16> = Vec::new();
                for (local, segment) in chapter.segments.iter().enumerate() {
                    let global = chapter.first_entry + local;
                    if let Some(cached) = &segments[global] {
                        let bytes = std::fs::read(self.audio_dir.join(&cached.file))
                            .map_err(|e| format!("cannot read audio: {e}"))?;
                        let decoded = codec::decode(&bytes).map_err(|e| e.to_string())?;
                        let decoded = aoide_audio::resample(&decoded, EXPORT_RATE);
                        let wanted = samples_for(cached.duration_ms);
                        pcm.extend(
                            decoded
                                .samples
                                .iter()
                                .chain(std::iter::repeat(&0.0))
                                .take(wanted)
                                .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
                        );
                    }
                    pcm.resize(
                        pcm.len() + samples_for(u64::from(segment.pause_after_ms)),
                        0,
                    );
                }
                let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
                stdin
                    .write_all(&bytes)
                    .map_err(|e| format!("ffmpeg stopped accepting audio: {e}"))?;
            }
            Ok(())
        })();
        drop(stdin);
        let finished = child.wait_with_output().map_err(|e| e.to_string())?;
        let _ = std::fs::remove_dir_all(&work);
        if let Err(error) = write_result {
            let _ = std::fs::remove_file(&partial);
            let detail = String::from_utf8_lossy(&finished.stderr);
            return Err(format!("{error} {}", detail.trim()).trim().to_owned());
        }
        if !finished.status.success() {
            let _ = std::fs::remove_file(&partial);
            let detail = String::from_utf8_lossy(&finished.stderr);
            let detail: String = detail.trim().chars().take(400).collect();
            tracing::warn!(event = "proc.exit", program = "ffmpeg", code = finished.status.code(), stderr_tail = %detail, "ffmpeg failed");
            return Err(format!("ffmpeg failed: {detail}"));
        }
        std::fs::rename(&partial, &output).map_err(|e| e.to_string())?;
        Ok(output.to_string_lossy().into_owned())
    }
}

impl Core {
    /// Exports `id` as an M4B into the Exports folder next to the library folder.
    pub async fn export_book(&self, id: &str) -> Result<String, String> {
        self.require_text(id)?;
        self.exports.audiobook(id, self).await
    }

    /// Exports `id` with its synced recording as a read-along EPUB into the Exports folder.
    pub async fn export_read_along(&self, id: &str) -> Result<String, String> {
        self.require_text(id)?;
        self.exports.read_along(id, self).await
    }
}

impl ExportHost for Core {
    fn prepare<'a>(&'a self, id: &'a str) -> aoide_tts::BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if self
                .prepare_view(id)
                .is_some_and(|view| view.state == PrepareStage::Paused)
            {
                self.set_prepare_paused(id, false);
            }
            self.prepare_book(id).await
        })
    }

    fn prepare_view(&self, id: &str) -> Option<PrepareView> {
        Core::prepare_view(self, id)
    }

    fn speech_plan(&self, id: &str) -> Result<(Arc<BookModel>, Plan), String> {
        let book = self.load_book(id)?;
        let plan = Core::speech_plan(self, &book)?;
        Ok((book, plan))
    }

    fn engine_name(&self) -> String {
        self.engine.profile_name()
    }

    fn recording_plan(&self, id: &str) -> Result<(Arc<BookModel>, Arc<RecordingPlan>), String> {
        if self.recording_stage(id) != Some(super::syncing::Stage::Ready) {
            return Err("the recording is not synced with the book yet".into());
        }
        let book = self.load_book(id)?;
        // Only the recording's clips: what it lacks stays without audio in a read-along book.
        let (plan, _) = self.recordings.playable(self, &book)?;
        Ok((book, plan))
    }
}

pub(super) fn which_ffmpeg() -> Result<(), String> {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|_| "ffmpeg is not installed; install it to export audiobooks".to_owned())
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| "ffmpeg did not run".to_owned())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmetadata_lists_tags_and_escaped_chapters() {
        let text = ffmetadata(
            "A = B; #1",
            &["Mira Vale".into(), "Ellis Reed".into()],
            "Narrated by Aoide",
            &[
                ChapterMark {
                    title: "One\nTwo".into(),
                    start_ms: 0,
                    end_ms: 1500,
                },
                ChapterMark {
                    title: "Back\\slash".into(),
                    start_ms: 1500,
                    end_ms: 4000,
                },
            ],
        );
        assert!(text.starts_with(";FFMETADATA1\n"));
        assert!(text.contains("title=A \\= B\\; \\#1\n"));
        assert!(text.contains("artist=Mira Vale, Ellis Reed\n"));
        assert!(text.contains("[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=1500\ntitle=One Two\n"));
        assert!(text.contains("START=1500\nEND=4000\ntitle=Back\\\\slash\n"));
    }

    #[test]
    fn file_names_are_safe_and_never_empty() {
        assert_eq!(file_name("A/B: C?"), "A B C.m4b");
        assert_eq!(file_name("..."), "Audiobook.m4b");
        assert_eq!(file_name("  "), "Audiobook.m4b");
    }

    #[test]
    fn sample_counts_round_up() {
        assert_eq!(samples_for(1000), 24_000);
        assert_eq!(samples_for(1), 24);
    }

    use super::super::tests::{library_with, paths, Recorder};

    struct Waiting;

    impl ExportHost for Waiting {
        fn prepare<'a>(&'a self, _: &'a str) -> aoide_tts::BoxFuture<'a, Result<(), String>> {
            Box::pin(std::future::pending())
        }
        fn prepare_view(&self, _: &str) -> Option<PrepareView> {
            None
        }
        fn speech_plan(&self, _: &str) -> Result<(Arc<BookModel>, Plan), String> {
            Err("no plan".into())
        }
        fn engine_name(&self) -> String {
            "Test".into()
        }
        fn recording_plan(&self, _: &str) -> Result<(Arc<BookModel>, Arc<RecordingPlan>), String> {
            Err("the recording is not synced with the book yet".into())
        }
    }

    #[tokio::test]
    async fn one_export_runs_at_a_time_and_a_failed_one_is_reported_and_frees_the_slot() {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Recorder::default());
        let exports = Arc::new(Exports::new(
            library_with(&["a", "b"]),
            &paths(dir.path()),
            Arc::clone(&events) as Arc<dyn Events>,
        ));
        let running = Arc::clone(&exports);
        let first = tokio::spawn(async move { running.read_along("a", &Waiting).await });
        let _ = first.await;
        assert_eq!(exports.exporting(), None, "a failed export frees the slot");

        let slot = exports.claim("a").unwrap();
        assert_eq!(
            exports.read_along("b", &Waiting).await.unwrap_err(),
            "another export is already running"
        );
        drop(slot);
        assert!(exports.read_along("b", &Waiting).await.is_err());

        let sent = events.payloads(CoreEvent::EXPORT_PROGRESS);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(sent[0]["stage"], "error");
        assert_eq!(
            sent[0]["error"],
            "the recording is not synced with the book yet"
        );
    }
}
