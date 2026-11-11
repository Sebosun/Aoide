//! M4B audiobook export: generates any missing audio, then streams the book's cached segments
//! through ffmpeg with chapter markers and the cover.

use super::*;

use std::io::Write;
use std::process::{Command, Stdio};

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

impl Core {
    /// Exports `id` as an M4B into the Exports folder next to the library folder.
    pub async fn export_book(self: &Arc<Self>, id: &str) -> Result<String, String> {
        {
            let mut extras = lock(&self.extras);
            if extras.exporting.is_some() {
                return Err("another export is already running".into());
            }
            extras.exporting = Some(id.to_owned());
        }
        let result = self.export_inner(id).await;
        lock(&self.extras).exporting = None;
        let progress = match &result {
            Ok(path) => ExportProgress {
                book_id: id.into(),
                stage: ExportStage::Done,
                done: 1,
                total: 1,
                path: Some(path.clone()),
                error: None,
            },
            Err(error) => ExportProgress {
                book_id: id.into(),
                stage: ExportStage::Error,
                done: 0,
                total: 0,
                path: None,
                error: Some(error.clone()),
            },
        };
        self.emit(CoreEvent::ExportProgress(progress));
        result
    }

    fn emit_export(&self, id: &str, stage: ExportStage, done: usize, total: usize) {
        self.emit(CoreEvent::ExportProgress(ExportProgress {
            book_id: id.into(),
            stage,
            done,
            total,
            path: None,
            error: None,
        }));
    }

    async fn export_inner(self: &Arc<Self>, id: &str) -> Result<String, String> {
        which_ffmpeg()?;
        // Generate everything first, reusing whole-book preparation.
        if self
            .prepare_view(id)
            .is_some_and(|view| view.state == PrepareStage::Paused)
        {
            self.set_prepare_paused(id, false);
        }
        self.prepare_book(id).await?;
        loop {
            match self.prepare_view(id) {
                Some(view) if view.state == PrepareStage::Done => break,
                Some(view) => self.emit_export(id, ExportStage::Generating, view.done, view.total),
                None => return Err("export cancelled: preparation was stopped".into()),
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
        let book = self.load_book(id)?;
        let target = self.build_target(Arc::clone(&book))?;
        if let Some(missing) = target.plan.items.iter().find(|i| i.speakable && !i.cached) {
            return Err(format!("audio for {} is missing; try again", missing.key));
        }
        let core = Arc::clone(self);
        let id = id.to_owned();
        tokio::task::spawn_blocking(move || core.encode_m4b(&id, &book, &target))
            .await
            .map_err(|e| e.to_string())?
    }

    fn encode_m4b(&self, id: &str, book: &BookModel, target: &Target) -> Result<String, String> {
        // Exact per-entry lengths so chapter marks match the audio.
        let segments: Vec<Option<CachedSegment>> = {
            let library = lock(&self.library);
            target
                .plan
                .items
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

        let exports = self
            .paths
            .library_dir
            .parent()
            .unwrap_or(&self.paths.library_dir)
            .join("Exports");
        std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
        let output = exports.join(file_name(&book.title));
        let partial = output.with_extension("m4b.part");
        let work = std::env::temp_dir().join(format!("aoide-export-{id}"));
        std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
        let metadata_path = work.join("chapters.txt");
        let profile = lock(&self.engine).profile.name.clone();
        std::fs::write(
            &metadata_path,
            ffmetadata(
                &book.title,
                &book.authors,
                &format!("Narrated by Aoide using {profile}"),
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
                self.emit_export(id, ExportStage::Encoding, index, total);
                let mut pcm: Vec<i16> = Vec::new();
                for (local, segment) in chapter.segments.iter().enumerate() {
                    let global = chapter.first_entry + local;
                    if let Some(cached) = &segments[global] {
                        let bytes = std::fs::read(self.paths.audio_dir.join(&cached.file))
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
            return Err(format!("ffmpeg failed: {detail}"));
        }
        std::fs::rename(&partial, &output).map_err(|e| e.to_string())?;
        Ok(output.to_string_lossy().into_owned())
    }
}

fn which_ffmpeg() -> Result<(), String> {
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
}
