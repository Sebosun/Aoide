//! Read-along EPUB export: the book's text, as Aoide reads it, with its synced recording as
//! EPUB 3 Media Overlays, so other reading apps highlight each sentence as the narrator reads it.
//! The source book is left as it is; the export is a new file in the Exports folder.

use super::*;

use std::io::Write as _;
use std::ops::Range;
use std::process::{Command, Stdio};

use aoide_epub::BlockKind;
use aoide_sync::Clip;

use super::export::{ExportProgress, ExportStage};
use super::recording::RecordingPlan;

/// The class reading apps put on the sentence being read.
const ACTIVE_CLASS: &str = "-epub-media-overlay-active";

/// A recording file as it goes into the EPUB.
pub struct OverlayAudio {
    /// Path inside the package folder, e.g. `audio/01.m4a`.
    pub href: String,
    pub media_type: &'static str,
    pub duration_us: u64,
}

/// The cover image as it goes into the EPUB.
pub struct OverlayCover {
    pub href: String,
    pub media_type: &'static str,
}

/// How a recording file goes into the EPUB: as it is when reading apps play it, otherwise
/// re-encoded to AAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Carry {
    Copy {
        extension: &'static str,
        media_type: &'static str,
    },
    Encode,
}

/// EPUB core audio types: MP3, AAC in MP4, and Opus in Ogg.
fn carry(codec: &str, path: &Path) -> Carry {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match (codec, extension.as_str()) {
        ("mp3", _) => Carry::Copy {
            extension: "mp3",
            media_type: "audio/mpeg",
        },
        ("aac", "m4a" | "m4b" | "mp4") => Carry::Copy {
            extension: "m4a",
            media_type: "audio/mp4",
        },
        ("opus", "ogg" | "opus" | "oga") => Carry::Copy {
            extension: "opus",
            media_type: "audio/ogg",
        },
        _ => Carry::Encode,
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if c.is_control() && !matches!(c, '\n' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}

/// A SMIL clock value, `h:mm:ss.fff`.
fn clock(ms: u64) -> String {
    format!(
        "{}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}

/// The id of a segment's span: segment ids (`3:12:0`) are not valid XML ids.
fn span_id(segment_id: &str) -> String {
    format!("s{}", segment_id.replace(':', "-"))
}

fn chapter_name(index: usize) -> String {
    format!("chapter-{:03}", index + 1)
}

fn chapter_title(book: &BookModel, index: usize) -> String {
    let title = book.chapters[index].title.trim();
    if title.is_empty() {
        format!("Chapter {}", index + 1)
    } else {
        title.to_owned()
    }
}

/// A stretch of the recording in one file: (file, start, end) in milliseconds into that file.
fn in_files(audio: &[OverlayAudio], range: Range<u64>) -> Vec<(usize, u64, u64)> {
    let mut out = Vec::new();
    let mut start_ms = 0;
    for (index, file) in audio.iter().enumerate() {
        let end_ms = start_ms + file.duration_us / 1_000;
        let from = range.start.max(start_ms);
        let to = range.end.min(end_ms);
        if from < to {
            out.push((index, from - start_ms, to - start_ms));
        }
        start_ms = end_ms;
    }
    out
}

/// A chapter's XHTML: its blocks, each spoken segment in a span the overlay points at.
fn chapter_document(book: &BookModel, index: usize, language: &str) -> String {
    let chapter = &book.chapters[index];
    let mut body = String::new();
    let mut in_list = false;
    for (block, kind) in chapter.block_kinds.iter().enumerate() {
        let segments: Vec<&aoide_text::Segment> = chapter
            .segments
            .iter()
            .filter(|s| s.block == block && !s.id.contains(":n"))
            .collect();
        if segments.is_empty() {
            continue;
        }
        let list_item = matches!(kind, BlockKind::ListItem);
        if list_item != in_list {
            body.push_str(if list_item { "<ul>\n" } else { "</ul>\n" });
            in_list = list_item;
        }
        let tag = match kind {
            BlockKind::Heading { level } => format!("h{}", level.clamp(&1, &6)),
            BlockKind::Paragraph => "p".into(),
            BlockKind::ListItem => "li".into(),
            BlockKind::Preformatted => "pre".into(),
        };
        let inner: Vec<String> = segments
            .iter()
            .map(|segment| {
                format!(
                    "<span id=\"{}\">{}</span>",
                    span_id(&segment.id),
                    escape(&segment.display)
                )
            })
            .collect();
        body.push_str(&format!("<{tag}>{}</{tag}>\n", inner.join(" ")));
    }
    if in_list {
        body.push_str("</ul>\n");
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         lang=\"{lang}\" xml:lang=\"{lang}\">\n<head>\n<meta charset=\"UTF-8\"/>\n\
         <title>{title}</title>\n<link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/>\n\
         </head>\n<body>\n<section epub:type=\"chapter\">\n{body}</section>\n</body>\n</html>\n",
        lang = escape(language),
        title = escape(&chapter_title(book, index)),
    )
}

/// A chapter's overlay and its length, or `None` when the recording has none of it.
fn chapter_overlay(
    book: &BookModel,
    index: usize,
    clips: &[Clip],
    audio: &[OverlayAudio],
) -> Option<(String, u64)> {
    let chapter = &book.chapters[index];
    let name = chapter_name(index);
    let mut pars = String::new();
    let mut count = 0;
    let mut duration = 0;
    for (local, segment) in chapter.segments.iter().enumerate() {
        if segment.id.contains(":n") || segment.speech.is_empty() {
            continue;
        }
        let Some(clip) = clips.get(chapter.first_entry + local) else {
            continue;
        };
        for (file, begin, end) in in_files(audio, clip.audio.clone()) {
            count += 1;
            duration += end - begin;
            pars.push_str(&format!(
                "<par id=\"p{count}\"><text src=\"{name}.xhtml#{}\"/>\
                 <audio src=\"{}\" clipBegin=\"{}\" clipEnd=\"{}\"/></par>\n",
                span_id(&segment.id),
                audio[file].href,
                clock(begin),
                clock(end)
            ));
        }
    }
    (count > 0).then(|| {
        (
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <smil xmlns=\"http://www.w3.org/ns/SMIL\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
                 version=\"3.0\">\n<body>\n<seq epub:textref=\"{name}.xhtml\" epub:type=\"chapter\">\n\
                 {pars}</seq>\n</body>\n</smil>\n"
            ),
            duration,
        )
    })
}

/// Every text document of the read-along EPUB, by path inside the package folder.
pub fn read_along_documents(
    book: &BookModel,
    clips: &[Clip],
    audio: &[OverlayAudio],
    cover: Option<&OverlayCover>,
    modified: &str,
) -> Vec<(String, String)> {
    let language = book.language.as_deref().unwrap_or("en");
    let mut documents = Vec::new();
    let mut manifest = String::new();
    let mut durations = String::new();
    let mut spine = String::new();
    let mut nav = String::new();
    let mut total = 0;
    for index in 0..book.chapters.len() {
        if book.chapters[index].segments.is_empty() {
            continue;
        }
        let name = chapter_name(index);
        documents.push((
            format!("{name}.xhtml"),
            chapter_document(book, index, language),
        ));
        let overlay = chapter_overlay(book, index, clips, audio);
        let overlay_attribute = if let Some((smil, duration)) = overlay {
            documents.push((format!("{name}.smil"), smil));
            manifest.push_str(&format!(
                "<item id=\"mo-{name}\" href=\"{name}.smil\" media-type=\"application/smil+xml\"/>\n"
            ));
            durations.push_str(&format!(
                "<meta property=\"media:duration\" refines=\"#mo-{name}\">{}</meta>\n",
                clock(duration)
            ));
            total += duration;
            format!(" media-overlay=\"mo-{name}\"")
        } else {
            String::new()
        };
        manifest.push_str(&format!(
            "<item id=\"{name}\" href=\"{name}.xhtml\" media-type=\"application/xhtml+xml\"{overlay_attribute}/>\n"
        ));
        spine.push_str(&format!("<itemref idref=\"{name}\"/>\n"));
        nav.push_str(&format!(
            "<li><a href=\"{name}.xhtml\">{}</a></li>\n",
            escape(&chapter_title(book, index))
        ));
    }
    for (index, file) in audio.iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"audio-{:02}\" href=\"{}\" media-type=\"{}\"/>\n",
            index + 1,
            file.href,
            file.media_type
        ));
    }
    if let Some(cover) = cover {
        manifest.push_str(&format!(
            "<item id=\"cover-image\" href=\"{}\" media-type=\"{}\" properties=\"cover-image\"/>\n",
            cover.href, cover.media_type
        ));
    }
    let creators: String = book
        .authors
        .iter()
        .map(|a| format!("<dc:creator>{}</dc:creator>\n", escape(a)))
        .collect();
    let package = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" \
         xml:lang=\"{lang}\">\n<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         <dc:identifier id=\"uid\">urn:aoide:read-along:{id}</dc:identifier>\n\
         <dc:title>{title}</dc:title>\n<dc:language>{lang}</dc:language>\n{creators}\
         <meta property=\"dcterms:modified\">{modified}</meta>\n{durations}\
         <meta property=\"media:duration\">{total}</meta>\n\
         <meta property=\"media:active-class\">{ACTIVE_CLASS}</meta>\n</metadata>\n\
         <manifest>\n<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
         <item id=\"style\" href=\"style.css\" media-type=\"text/css\"/>\n{manifest}</manifest>\n\
         <spine>\n{spine}</spine>\n</package>\n",
        lang = escape(language),
        id = escape(&book.id),
        title = escape(&book.title),
        total = clock(total),
    );
    let nav = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" \
         lang=\"{lang}\" xml:lang=\"{lang}\">\n<head>\n<meta charset=\"UTF-8\"/>\n<title>{title}</title>\n\
         </head>\n<body>\n<nav epub:type=\"toc\" id=\"toc\">\n<h1>Contents</h1>\n<ol>\n{nav}</ol>\n</nav>\n\
         </body>\n</html>\n",
        lang = escape(language),
        title = escape(&book.title),
    );
    let style = format!(
        "body {{ line-height: 1.5; }}\n.{ACTIVE_CLASS} {{ background-color: #fde68a; color: #1c1917; }}\n"
    );
    documents.push(("package.opf".into(), package));
    documents.push(("nav.xhtml".into(), nav));
    documents.push(("style.css".into(), style));
    documents
}

/// A safe file name for the read-along EPUB.
fn file_name(title: &str) -> String {
    let m4b = super::export::file_name(title);
    let stem = m4b.strip_suffix(".m4b").unwrap_or(&m4b);
    format!("{stem} (read-along).epub")
}

const CONTAINER: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\n\
<rootfiles>\n<rootfile full-path=\"OEBPS/package.opf\" media-type=\"application/oebps-package+xml\"/>\n\
</rootfiles>\n</container>\n";

impl Core {
    /// Exports `id` with its synced recording as a read-along EPUB into the Exports folder.
    pub async fn export_read_along(self: &Arc<Self>, id: &str) -> Result<String, String> {
        {
            let mut extras = lock(&self.extras);
            if extras.exporting.is_some() {
                return Err("another export is already running".into());
            }
            extras.exporting = Some(id.to_owned());
        }
        let core = Arc::clone(self);
        let owned = id.to_owned();
        let result = tokio::task::spawn_blocking(move || core.write_read_along(&owned))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        lock(&self.extras).exporting = None;
        self.emit(CoreEvent::ExportProgress(ExportProgress {
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
        }));
        result
    }

    fn write_read_along(&self, id: &str) -> Result<String, String> {
        if self.recording_stage(id) != Some(super::syncing::Stage::Ready) {
            return Err("the recording is not synced with the book yet".into());
        }
        let book = self.load_book(id)?;
        let target = self.build_recording_target(Arc::clone(&book))?;
        let Source::Recording(plan) = &target.source else {
            return Err("the book has no synced recording".into());
        };
        let plan: &RecordingPlan = plan;
        let carried: Vec<Carry> = plan
            .files
            .iter()
            .map(|f| carry(&f.codec, &f.path))
            .collect();
        if carried.contains(&Carry::Encode) {
            super::export::which_ffmpeg()?;
        }
        let audio: Vec<OverlayAudio> = plan
            .files
            .iter()
            .zip(&carried)
            .enumerate()
            .map(|(index, (file, carry))| {
                let (extension, media_type) = match carry {
                    Carry::Copy {
                        extension,
                        media_type,
                    } => (*extension, *media_type),
                    Carry::Encode => ("m4a", "audio/mp4"),
                };
                OverlayAudio {
                    href: format!("audio/{:02}.{extension}", index + 1),
                    media_type,
                    duration_us: file.duration_us(),
                }
            })
            .collect();
        let cover_path = lock(&self.library)
            .book(&BookId(id.into()))
            .ok()
            .flatten()
            .and_then(|r| r.cover_path)
            .filter(|p| p.exists());
        let cover = cover_path.as_ref().and_then(|path| {
            let extension = path.extension()?.to_str()?.to_ascii_lowercase();
            let media_type = match extension.as_str() {
                "jpg" | "jpeg" => "image/jpeg",
                "png" => "image/png",
                "webp" => "image/webp",
                _ => return None,
            };
            Some(OverlayCover {
                href: format!("cover.{extension}"),
                media_type,
            })
        });
        let modified = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let documents =
            read_along_documents(&book, &plan.clips, &audio, cover.as_ref(), &modified);

        let exports = self
            .paths
            .library_dir
            .parent()
            .unwrap_or(&self.paths.library_dir)
            .join("Exports");
        std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
        let output = exports.join(file_name(&book.title));
        let partial = output.with_extension("epub.part");
        let work = std::env::temp_dir().join(format!("aoide-read-along-{id}"));
        let written = (|| -> Result<(), String> {
            let file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            let deflated = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            let zip_error = |e: zip::result::ZipError| e.to_string();
            let io_error = |e: std::io::Error| e.to_string();
            zip.start_file("mimetype", stored).map_err(zip_error)?;
            zip.write_all(b"application/epub+zip").map_err(io_error)?;
            zip.start_file("META-INF/container.xml", deflated)
                .map_err(zip_error)?;
            zip.write_all(CONTAINER.as_bytes()).map_err(io_error)?;
            for (path, text) in &documents {
                zip.start_file(format!("OEBPS/{path}"), deflated)
                    .map_err(zip_error)?;
                zip.write_all(text.as_bytes()).map_err(io_error)?;
            }
            if let (Some(cover), Some(path)) = (&cover, &cover_path) {
                let bytes = std::fs::read(path).map_err(io_error)?;
                zip.start_file(format!("OEBPS/{}", cover.href), stored)
                    .map_err(zip_error)?;
                zip.write_all(&bytes).map_err(io_error)?;
            }
            let total = plan.files.len();
            for (index, (file, carry)) in plan.files.iter().zip(&carried).enumerate() {
                self.emit_export(id, ExportStage::Encoding, index, total);
                let source = match carry {
                    Carry::Copy { .. } => file.path.clone(),
                    Carry::Encode => {
                        std::fs::create_dir_all(&work).map_err(io_error)?;
                        let encoded = work.join(format!("{:02}.m4a", index + 1));
                        encode_aac(&file.path, &encoded)?;
                        encoded
                    }
                };
                let size = std::fs::metadata(&source).map_err(io_error)?.len();
                zip.start_file(
                    format!("OEBPS/{}", audio[index].href),
                    stored.large_file(size >= u64::from(u32::MAX)),
                )
                .map_err(zip_error)?;
                let mut reader = std::fs::File::open(&source)
                    .map_err(|e| format!("cannot read {}: {e}", file.path.display()))?;
                std::io::copy(&mut reader, &mut zip).map_err(io_error)?;
                if matches!(carry, Carry::Encode) {
                    let _ = std::fs::remove_file(&source);
                }
            }
            zip.finish()
                .map_err(zip_error)?
                .flush()
                .map_err(io_error)?;
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&work);
        if let Err(error) = written {
            let _ = std::fs::remove_file(&partial);
            return Err(error);
        }
        std::fs::rename(&partial, &output).map_err(|e| e.to_string())?;
        Ok(output.to_string_lossy().into_owned())
    }
}

/// Re-encodes a file reading apps cannot play (FLAC, WAV, Vorbis, …) to AAC in MP4.
fn encode_aac(from: &Path, to: &Path) -> Result<(), String> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(from)
        .args(["-vn", "-c:a", "aac", "-b:a", "96k", "-f", "mp4"])
        .arg(to)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start ffmpeg: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        let detail: String = detail.trim().chars().take(400).collect();
        Err(format!("ffmpeg could not re-encode the audio: {detail}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_reading_apps_play_is_copied_and_the_rest_re_encoded() {
        let copy = |codec, name| carry(codec, Path::new(name));
        assert_eq!(
            copy("aac", "book.M4B"),
            Carry::Copy {
                extension: "m4a",
                media_type: "audio/mp4"
            }
        );
        assert_eq!(
            copy("mp3", "01.mp3"),
            Carry::Copy {
                extension: "mp3",
                media_type: "audio/mpeg"
            }
        );
        assert_eq!(copy("aac", "raw.aac"), Carry::Encode);
        assert_eq!(copy("flac", "01.flac"), Carry::Encode);
        assert_eq!(copy("alac", "01.m4a"), Carry::Encode);
    }

    #[test]
    fn a_stretch_over_two_files_is_cut_at_the_boundary() {
        let audio = |ms: u64| OverlayAudio {
            href: String::new(),
            media_type: "audio/mpeg",
            duration_us: ms * 1_000,
        };
        let files = [audio(1_000), audio(2_000)];
        assert_eq!(
            in_files(&files, 800..1_300),
            [(0, 800, 1_000), (1, 0, 300)]
        );
        assert_eq!(clock(3_723_004), "1:02:03.004");
    }
}
