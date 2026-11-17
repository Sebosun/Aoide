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
    /// An export ended; `took` is how long it ran.
    fn happened(&self, happened: super::notices::Happened, took: Duration);
}

/// Writes export files, on a blocking thread.
pub(super) struct Encoder {
    pub(super) library: Arc<Mutex<Library>>,
    library_dir: PathBuf,
    audio_dir: PathBuf,
    /// Where an item's own cover is (see `details`).
    pub(super) covers_dir: PathBuf,
    events: Arc<dyn Events>,
}

/// Exports books as audiobooks and read-along EPUBs, one at a time.
pub(super) struct Exports {
    encoder: Arc<Encoder>,
    /// The book being exported.
    exporting: Mutex<Option<String>>,
    /// How long exports ran.
    clock: super::environment::Clock,
}

/// Holds the export slot; dropping it frees the slot.
struct Slot<'a>(&'a Mutex<Option<String>>);

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        *lock(self.0) = None;
    }
}

impl Exports {
    pub fn new(
        library: Arc<Mutex<Library>>,
        paths: &Paths,
        events: Arc<dyn Events>,
        clock: super::environment::Clock,
    ) -> Self {
        Self {
            encoder: Arc::new(Encoder {
                library,
                library_dir: paths.library_dir.clone(),
                audio_dir: paths.audio_dir.clone(),
                covers_dir: paths.covers_dir.clone(),
                events,
            }),
            exporting: Mutex::new(None),
            clock,
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
        let started = Instant::now();
        let since = self.clock.now();
        let result = {
            let _slot = self.claim(id)?;
            self.make_audiobook(id, host).await
        };
        log_export("m4b", id, started, &result);
        self.ended(id, &result, since, host);
        result
    }

    /// Tells the window and the host how the export of `id`, started at `since`, ended.
    fn ended(
        &self,
        id: &str,
        result: &Result<String, String>,
        since: Instant,
        host: &dyn ExportHost,
    ) {
        use super::notices::Happened;
        self.encoder.finished(id, result);
        let happened = match result {
            Ok(_) => Happened::Exported(id.into()),
            Err(_) => Happened::ExportFailed(id.into()),
        };
        host.happened(happened, self.clock.now() - since);
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
        let started = Instant::now();
        let since = self.clock.now();
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
        log_export("read-along", id, started, &result);
        self.ended(id, &result, since, host);
        result
    }
}

/// Whether any entry of `chapter` has audio. One without (a chapter left out of the
/// narration) is not in the export at all, not even its pauses.
fn has_audio(chapter: &crate::book::ChapterModel, segments: &[Option<CachedSegment>]) -> bool {
    (0..chapter.segments.len()).any(|local| segments[chapter.first_entry + local].is_some())
}

/// Chapter marks from each entry's exact length and pause, so they match the audio. Chapters
/// with no audio get none.
fn chapter_marks(book: &BookModel, segments: &[Option<CachedSegment>]) -> Vec<ChapterMark> {
    let mut chapters = Vec::new();
    let mut cursor_ms = 0u64;
    for (index, chapter) in book.chapters.iter().enumerate() {
        if !has_audio(chapter, segments) {
            continue;
        }
        let start = cursor_ms;
        for (local, segment) in chapter.segments.iter().enumerate() {
            let audio = segments[chapter.first_entry + local]
                .as_ref()
                .map_or(0, |s| s.duration_ms);
            cursor_ms += audio + u64::from(segment.pause_after_ms);
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
    chapters
}

/// ffmpeg reading mono PCM on its input, with the chapters from `metadata` and the `cover`
/// as the attached picture, writing an M4B to `output`.
fn ffmpeg_command(metadata: &Path, cover: Option<&Path>, output: &Path) -> Command {
    let mut command = Command::new("ffmpeg");
    command.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "s16le"]);
    command.args(["-ar", &EXPORT_RATE.to_string(), "-ac", "1", "-i", "pipe:0"]);
    command.arg("-i").arg(metadata);
    if let Some(cover) = cover {
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
    command.arg(output);
    command
}

/// Logs how an export ended.
fn log_export(kind: &str, id: &str, started: Instant, result: &Result<String, String>) {
    let book = crate::logging::book(id);
    let took_ms = crate::logging::took(started);
    match result {
        Ok(path) => {
            tracing::info!(event = "export.done", kind, book, took_ms, file = %crate::logging::file(Path::new(path)), outcome = "ok", "Exported a book")
        }
        Err(error) => {
            tracing::warn!(event = "export.done", kind, book, took_ms, outcome = "failed", %error, "Export failed")
        }
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
        let chapters = chapter_marks(book, &segments);
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
        let cover =
            super::details::shown(&lock(&self.library), &BookId(id.into()), &self.covers_dir)
                .and_then(|r| r.cover_path)
                .filter(|p| {
                    p.exists()
                        && matches!(
                            p.extension().and_then(|e| e.to_str()),
                            Some("jpg" | "jpeg" | "png")
                        )
                });

        let mut command = ffmpeg_command(&metadata_path, cover.as_deref(), &partial);
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
                let bytes: Vec<u8> = self
                    .chapter_pcm(chapter, &segments)?
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect();
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

    /// A chapter's audio at the export rate: each entry's speech, padded or cut to its
    /// recorded length, then its pause; nothing for a chapter with no audio.
    fn chapter_pcm(
        &self,
        chapter: &crate::book::ChapterModel,
        segments: &[Option<CachedSegment>],
    ) -> Result<Vec<i16>, String> {
        let mut pcm: Vec<i16> = Vec::new();
        if !has_audio(chapter, segments) {
            return Ok(pcm);
        }
        for (local, segment) in chapter.segments.iter().enumerate() {
            if let Some(cached) = &segments[chapter.first_entry + local] {
                let bytes = std::fs::read(self.audio_dir.join(&cached.file))
                    .map_err(|e| format!("cannot read audio: {e}"))?;
                let decoded = codec::decode(&bytes).map_err(|e| e.to_string())?;
                let decoded = aoide_audio::resample(&decoded, EXPORT_RATE);
                pcm.extend(
                    decoded
                        .samples
                        .iter()
                        .chain(std::iter::repeat(&0.0))
                        .take(samples_for(cached.duration_ms))
                        .map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16),
                );
            }
            pcm.resize(
                pcm.len() + samples_for(u64::from(segment.pause_after_ms)),
                0,
            );
        }
        Ok(pcm)
    }
}

impl Core {
    /// Exports `id` as an M4B into the Exports folder next to the library folder.
    pub async fn export_book(&self, id: &str) -> Result<String, String> {
        self.require(id, super::item::Needs::Text)?;
        self.exports.audiobook(id, self).await
    }

    /// Exports `id` with its synced recording as a read-along EPUB into the Exports folder.
    pub async fn export_read_along(&self, id: &str) -> Result<String, String> {
        self.require(id, super::item::Needs::Text)?;
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

    /// The chapters left out are not exported.
    fn speech_plan(&self, id: &str) -> Result<(Arc<BookModel>, Plan), String> {
        let book = self.load_book(id)?;
        let mut plan = Core::speech_plan(self, &book)?;
        let left_out = self.book_settings(id).left_out;
        super::leaving_out::narrated_only(&mut plan, &book, &left_out);
        Ok((book, plan))
    }

    fn engine_name(&self) -> String {
        self.engine.profile_name()
    }

    fn happened(&self, happened: super::notices::Happened, took: Duration) {
        self.tell(happened, took);
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
    use super::super::notices::Happened;
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

    #[test]
    fn an_export_leaves_the_pauses_set_after_paragraphs_breaks_and_chapters() {
        let dir = tempfile::tempdir().unwrap();
        let chapter = |body: &str| {
            format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>"#)
        };
        let bytes = super::super::tests::epub_with(
            chapter("<h1>The Quay</h1><p>Gulls argued over the nets all morning.</p><hr/><p>The tide came in slowly that evening.</p>"),
            chapter("<h1>The Lamp</h1><p>Someone lit the lamp at dusk, as always.</p><p>It burned until the grey morning came.</p>"),
        );
        let pauses = crate::core::settings::PauseSettings {
            paragraph_ms: 300,
            break_ms: 2000,
            heading_ms: 1000,
        };
        let layout = crate::book::Layout {
            read_notes: false,
            segments: pauses.segment_options(),
        };
        let book = BookModel::parse("b", &bytes, &layout).unwrap();
        assert_eq!(book.total_entries(), 6);
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.audio_dir).unwrap();
        let second = aoide_audio::Pcm {
            sample_rate: EXPORT_RATE,
            samples: vec![0.25; EXPORT_RATE as usize],
        };
        std::fs::write(
            paths.audio_dir.join("s.wav"),
            aoide_audio::codec::encode(&second).unwrap(),
        )
        .unwrap();
        let segments: Vec<Option<CachedSegment>> = (0..6)
            .map(|i| {
                Some(CachedSegment {
                    key: format!("k{i}"),
                    book_id: BookId("b".into()),
                    file: "s.wav".into(),
                    duration_ms: 1000,
                    bytes: 0,
                    word_timings: None,
                    created_at: 0,
                })
            })
            .collect();

        // Heading, the paragraph before the rule, the chapter's last paragraph.
        let one = 3 * 1000 + 1000 + 2000 + 1000;
        // Heading, a paragraph, the chapter's last paragraph.
        let two = 3 * 1000 + 1000 + 300 + 1000;
        let marks = chapter_marks(&book, &segments);
        assert_eq!(
            marks
                .iter()
                .map(|m| (m.start_ms, m.end_ms))
                .collect::<Vec<_>>(),
            [(0, one), (one, one + two)]
        );
        let exports = Exports::new(
            library_with(&["b"]),
            &paths,
            Arc::new(Recorder::default()),
            Default::default(),
        );
        for (chapter, ms) in book.chapters.iter().zip([one, two]) {
            let pcm = exports.encoder.chapter_pcm(chapter, &segments).unwrap();
            assert_eq!(pcm.len(), samples_for(ms));
            let pause = samples_for(1000);
            assert!(
                pcm[pcm.len() - pause..].iter().all(|s| *s == 0),
                "the chapter ends in silence"
            );
        }
    }

    #[test]
    fn a_chapter_left_out_has_no_mark_and_no_silence_in_an_export() {
        let dir = tempfile::tempdir().unwrap();
        let book =
            BookModel::parse("b", &super::super::tests::epub(), &Default::default()).unwrap();
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.audio_dir).unwrap();
        let second = aoide_audio::Pcm {
            sample_rate: EXPORT_RATE,
            samples: vec![0.25; EXPORT_RATE as usize],
        };
        std::fs::write(
            paths.audio_dir.join("s.wav"),
            aoide_audio::codec::encode(&second).unwrap(),
        )
        .unwrap();
        // The first chapter is left out: its entries have no audio.
        let first = book.chapters[1].first_entry;
        let segments: Vec<Option<CachedSegment>> = (0..book.total_entries())
            .map(|i| {
                (i >= first).then(|| CachedSegment {
                    key: format!("k{i}"),
                    book_id: BookId("b".into()),
                    file: "s.wav".into(),
                    duration_ms: 1000,
                    bytes: 0,
                    word_timings: None,
                    created_at: 0,
                })
            })
            .collect();
        let two: u64 = book.chapters[1]
            .segments
            .iter()
            .map(|s| 1000 + u64::from(s.pause_after_ms))
            .sum();
        assert_eq!(
            chapter_marks(&book, &segments),
            [ChapterMark {
                title: "The Lamp".into(),
                start_ms: 0,
                end_ms: two
            }]
        );
        let exports = Exports::new(
            library_with(&["b"]),
            &paths,
            Arc::new(Recorder::default()),
            Default::default(),
        );
        let left_out = exports.encoder.chapter_pcm(&book.chapters[0], &segments);
        assert!(left_out.unwrap().is_empty(), "not even its pauses");
        let pcm = exports.encoder.chapter_pcm(&book.chapters[1], &segments);
        assert_eq!(pcm.unwrap().len(), samples_for(two));
    }

    /// A host whose preparation never ends and that has no recording synced.
    #[derive(Default)]
    struct Waiting {
        happened: Mutex<Vec<Happened>>,
    }

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
        fn happened(&self, happened: Happened, _: Duration) {
            lock(&self.happened).push(happened);
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
            Default::default(),
        ));
        let host = Arc::new(Waiting::default());
        let running = Arc::clone(&exports);
        let first_host = Arc::clone(&host);
        let first = tokio::spawn(async move { running.read_along("a", &*first_host).await });
        let _ = first.await;
        assert_eq!(exports.exporting(), None, "a failed export frees the slot");

        let slot = exports.claim("a").unwrap();
        assert_eq!(
            exports.read_along("b", &*host).await.unwrap_err(),
            "another export is already running"
        );
        drop(slot);
        assert!(exports.read_along("b", &*host).await.is_err());
        assert_eq!(
            *lock(&host.happened),
            [
                Happened::ExportFailed("a".into()),
                Happened::ExportFailed("b".into())
            ],
            "an export refused for another running tells of nothing"
        );

        let sent = events.payloads(CoreEvent::EXPORT_PROGRESS);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(sent[0]["stage"], "error");
        assert_eq!(
            sent[0]["error"],
            "the recording is not synced with the book yet"
        );
    }
}
