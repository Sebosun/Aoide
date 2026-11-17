//! A book's pictures, copied out of it into a folder of its own (`pictures/<book id>/`) when a
//! chapter that shows them is opened, so the reader can draw them. The book is only read.

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use super::lock;

/// A picture larger than this on its longest side is scaled down to it.
const LARGEST_SIDE: u32 = 2048;
/// A picture smaller than this on its longest side is decoration (a rule, an ornament).
const SMALLEST_SIDE: u32 = 32;

pub(super) struct Pictures {
    dir: PathBuf,
    /// What each picture became, by book and resource: its file, or `None` (left out).
    settled: Mutex<HashMap<(String, String), Option<PathBuf>>>,
}

impl Pictures {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            settled: Mutex::default(),
        }
    }

    /// Where book `id`'s pictures are.
    pub fn folder(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    /// The file of each of book `id`'s `resources`, copied with `read` (a resource's bytes, from
    /// the book) the first time it is asked for. `None`: left out (tiny, not a picture, or not
    /// in the book).
    pub fn files(
        &self,
        id: &str,
        resources: &[&str],
        read: &dyn Fn(&str) -> Result<Vec<u8>, String>,
    ) -> Vec<Option<PathBuf>> {
        let started = Instant::now();
        let (mut count, mut bytes) = (0, 0);
        let files = resources
            .iter()
            .map(|resource| {
                let key = (id.to_owned(), (*resource).to_owned());
                if let Some(settled) = lock(&self.settled).get(&key) {
                    return settled.clone();
                }
                let file = read(resource)
                    .and_then(|source| store(&source, resource, &self.folder(id)))
                    .unwrap_or_else(|error| {
                        tracing::warn!(event = "pictures.extracted", book = crate::logging::book(id), outcome = "failed", %error, "Could not copy a picture");
                        None
                    });
                if let Some(file) = &file {
                    count += 1;
                    bytes += std::fs::metadata(file).map_or(0, |m| m.len());
                }
                lock(&self.settled).insert(key, file.clone());
                file
            })
            .collect();
        if count > 0 {
            tracing::info!(
                event = "pictures.extracted",
                book = crate::logging::book(id),
                count,
                bytes,
                took_ms = started.elapsed().as_millis() as u64,
                outcome = "ok",
                "Copied pictures out of a book"
            );
        }
        files
    }

    /// Book `id` was removed, or its file changed: its pictures go.
    pub fn forget(&self, id: &str) {
        lock(&self.settled).retain(|(book, _), _| book != id);
        let folder = self.folder(id);
        if folder.exists() {
            if let Err(error) = std::fs::remove_dir_all(&folder) {
                tracing::warn!(event = "pictures.removed", book = crate::logging::book(id), %error, "Could not remove a book's pictures");
            }
        }
    }
}

/// `source` (the picture at `resource`) written into `folder`, scaled down if huge; `None` when
/// it is decoration.
fn store(source: &[u8], resource: &str, folder: &Path) -> Result<Option<PathBuf>, String> {
    use sha2::{Digest, Sha256};
    let name = format!("{:x}", Sha256::digest(resource.as_bytes()));
    let name = &name[..16];
    let svg = Path::new(resource)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    if svg {
        // Drawn by the page as it is (in an <img>, where its scripts do not run).
        let file = folder.join(format!("{name}.svg"));
        std::fs::write(&file, source).map_err(|e| e.to_string())?;
        return Ok(Some(file));
    }
    let reader = image::ImageReader::new(Cursor::new(source))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let format = reader.format().ok_or("not a picture")?;
    let extension = format.extensions_str().first().copied().unwrap_or("img");
    let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
    if width.max(height) < SMALLEST_SIDE {
        return Ok(None);
    }
    if width.max(height) <= LARGEST_SIDE {
        let file = folder.join(format!("{name}.{extension}"));
        std::fs::write(&file, source).map_err(|e| e.to_string())?;
        return Ok(Some(file));
    }
    let picture = image::load_from_memory_with_format(source, format).map_err(|e| e.to_string())?;
    let smaller = picture.resize(
        LARGEST_SIDE,
        LARGEST_SIDE,
        image::imageops::FilterType::Triangle,
    );
    // A photograph stays a JPEG; anything else (line art, transparency) becomes a PNG.
    let (extension, format) = if format == image::ImageFormat::Jpeg {
        ("jpg", image::ImageFormat::Jpeg)
    } else {
        ("png", image::ImageFormat::Png)
    };
    let file = folder.join(format!("{name}.{extension}"));
    let smaller = if format == image::ImageFormat::Jpeg {
        image::DynamicImage::ImageRgb8(smaller.to_rgb8())
    } else {
        smaller
    };
    smaller
        .save_with_format(&file, format)
        .map_err(|e| e.to_string())?;
    Ok(Some(file))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A PNG of `width` by `height` pixels.
    pub fn png(width: u32, height: u32) -> Vec<u8> {
        let picture = image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(picture)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    fn book(resource: &str) -> Result<Vec<u8>, String> {
        match resource {
            "OPS/lamp.png" => Ok(png(300, 200)),
            "OPS/huge.png" => Ok(png(4000, 1000)),
            "OPS/dot.png" => Ok(png(8, 8)),
            "OPS/map.svg" => Ok(b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec()),
            "OPS/text.png" => Ok(b"not a picture".to_vec()),
            _ => Err("not in the book".into()),
        }
    }

    #[test]
    fn pictures_are_copied_once_scaled_down_if_huge_and_left_out_if_tiny() {
        let dir = tempfile::tempdir().unwrap();
        let pictures = Pictures::new(dir.path().join("pictures"));
        let asked = [
            "OPS/lamp.png",
            "OPS/huge.png",
            "OPS/dot.png",
            "OPS/map.svg",
            "OPS/text.png",
            "OPS/gone.png",
        ];
        let files = pictures.files("b1", &asked, &book);
        let [lamp, huge, dot, map, text, gone] = &files[..] else {
            panic!("{files:?}")
        };
        assert!((dot, text, gone) == (&None, &None, &None), "{files:?}");
        let lamp = lamp.as_ref().unwrap();
        assert!(lamp.starts_with(dir.path().join("pictures").join("b1")));
        assert_eq!(std::fs::read(lamp).unwrap(), png(300, 200), "as it was");
        assert_eq!(
            image::image_dimensions(huge.as_ref().unwrap()).unwrap(),
            (2048, 512)
        );
        assert!(map
            .as_ref()
            .unwrap()
            .extension()
            .is_some_and(|e| e == "svg"));

        // Asked again, nothing is read.
        let again = pictures.files("b1", &asked, &|_| panic!("read again"));
        assert_eq!(again, files);

        pictures.forget("b1");
        assert!(!dir.path().join("pictures").join("b1").exists());
        assert!(!lamp.exists());
    }
}

#[cfg(test)]
mod core_tests {
    use super::tests::png;
    use crate::book::InsertView;
    use crate::core::tests::{library_folder_holding, start, until};
    use aoide_tts::testing::FakeEngine;
    use std::io::Write;

    /// A one-chapter book with a picture, a captioned figure and an ornament.
    pub fn illustrated_epub() -> Vec<u8> {
        let chapter = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>The Lamp</h1><p>The keeper climbed the stair.</p><img src="images/stair.png" alt="A spiral stair"/><p>At the top the lamp waited.</p><figure><img src="images/lamp.png" alt="The great lamp"/><figcaption>The lamp, lit at dusk.</figcaption></figure><p>Then he slept.</p><img src="images/dot.png"/></body></html>"#;
        let files: Vec<(&str, Vec<u8>)> = vec![
            ("mimetype", b"application/epub+zip".to_vec()),
            ("META-INF/container.xml", br#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_vec()),
            ("OPS/book.opf", br#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>The Lamp</dc:title><dc:creator>Test Author</dc:creator><dc:language>en</dc:language></metadata><manifest><item id="one" href="one.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="one"/></spine></package>"#.to_vec()),
            ("OPS/one.xhtml", chapter.as_bytes().to_vec()),
            ("OPS/images/stair.png", png(120, 300)),
            ("OPS/images/lamp.png", png(300, 200)),
            ("OPS/images/dot.png", png(6, 6)),
        ];
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body) in files {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&body).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[tokio::test]
    async fn an_opened_chapter_shows_its_pictures_which_go_with_the_book() {
        let dir = tempfile::tempdir().unwrap();
        let id = library_folder_holding(
            dir.path(),
            "",
            "voiceId = \"Test.wav\"",
            &illustrated_epub(),
        );
        let (core, _) = start(dir.path(), &FakeEngine::new());
        until("the scan", || core.list_books().is_ok_and(|b| b.len() == 1)).await;

        let view = core.chapter(&id, 0).unwrap();
        let texts: Vec<String> = view
            .blocks
            .iter()
            .map(|b| {
                b.segments
                    .iter()
                    .flat_map(|s| s.words.clone())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        assert_eq!(texts[3], "The lamp, lit at dusk.", "the caption stays text");
        let pictures: Vec<(usize, String, Option<usize>)> = view
            .inserts
            .iter()
            .map(|insert| match insert {
                InsertView::Picture {
                    before_block,
                    alt,
                    caption_block,
                    ..
                } => (*before_block, alt.clone(), *caption_block),
            })
            .collect();
        assert_eq!(
            pictures,
            [
                (2, "A spiral stair".into(), None),
                (3, "The great lamp".into(), Some(3))
            ],
            "the ornament is left out"
        );
        let InsertView::Picture { path, .. } = &view.inserts[1];
        let file = std::path::PathBuf::from(path);
        assert_eq!(std::fs::read(&file).unwrap(), png(300, 200));
        assert!(core.storage_report().unwrap().pictures_bytes > 0);

        core.remove_book(&id).unwrap();
        assert!(!file.exists(), "removed with the book");
    }
}
