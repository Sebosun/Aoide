//! What an item is (a book, an article, an audiobook) and what that allows, and the cover each
//! item keeps. Every operation that needs text, or a book, asks [`Core::require`]; every cover
//! is kept by [`store_cover`], whatever it came from.

use std::path::{Path, PathBuf};

use aoide_library::{BookId, ItemKind};

use super::{lock, Core};

/// What a text-only operation says about an audiobook.
pub(super) const NO_TEXT: &str = "Not available for an audiobook: it has no text.";

/// What an operation needs of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Needs {
    /// Text to read or speak: a book or an article.
    Text,
    /// A book (an EPUB): paired with a recording, looked up in a book catalogue.
    Book,
}

/// Whether an item of `kind` allows what `needs` asks; the refusal says why not.
pub(super) fn allows(kind: ItemKind, needs: Needs) -> Result<(), &'static str> {
    match (kind, needs) {
        (ItemKind::Audiobook, _) => Err(NO_TEXT),
        (ItemKind::Article, Needs::Book) => Err(super::articles::NOT_FOR_ARTICLES),
        _ => Ok(()),
    }
}

impl Core {
    /// What item `id` is; `None` when it is not in the library.
    pub(super) fn item_kind(&self, id: &str) -> Option<ItemKind> {
        let record = lock(&self.library).book(&BookId(id.into())).ok()??;
        Some(record.kind)
    }

    /// Refuses item `id` when it is not what an operation `needs`. An item that is not in the
    /// library is left for the operation to refuse.
    pub(super) fn require(&self, id: &str, needs: Needs) -> Result<(), String> {
        match self.item_kind(id) {
            Some(kind) => allows(kind, needs).map_err(str::to_owned),
            None => Ok(()),
        }
    }
}

// ---------------------------------------------------------------- covers

/// The file types a cover is kept as.
pub(super) const COVER_EXTENSIONS: [&str; 6] = ["jpg", "png", "gif", "webp", "svg", "bmp"];

/// What to do with a cover whose media type is not an image type Aoide knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Untyped {
    /// Keep it as a JPEG: a cover a book or audiobook file declares is an image, often with a
    /// loosely written type.
    Jpeg,
    /// Do not keep it: an image fetched from the web may be anything.
    Refused,
}

/// The file extension for an image of `media_type`, when it is one Aoide keeps.
pub(super) fn cover_extension(media_type: &str) -> Option<&'static str> {
    match media_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" | "image/jpg" | "image/pjpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

/// Keeps `bytes` (an image of `media_type`) as item `id`'s cover in `dir`, as `<id>.<ext>`,
/// in place of any cover it had in another type. Returns the file; `None` when the type is
/// refused or the file cannot be written. A cover already kept with the same bytes is not
/// written again.
pub(super) fn store_cover(
    dir: &Path,
    id: &str,
    media_type: &str,
    bytes: &[u8],
    untyped: Untyped,
) -> Option<PathBuf> {
    let ext = cover_extension(media_type).or(match untyped {
        Untyped::Jpeg => Some("jpg"),
        Untyped::Refused => None,
    })?;
    let target = dir.join(format!("{id}.{ext}"));
    let kept = std::fs::read(&target).is_ok_and(|old| old == bytes);
    if !kept {
        std::fs::create_dir_all(dir).ok()?;
        std::fs::write(&target, bytes).ok()?;
    }
    for other in COVER_EXTENSIONS.iter().filter(|e| **e != ext) {
        let _ = std::fs::remove_file(dir.join(format!("{id}.{other}")));
    }
    Some(target)
}

/// Deletes item `id`'s cover in `dir`, in whichever type it was kept, and its thumbnail.
pub(super) fn drop_covers(dir: &Path, id: &str) {
    for ext in COVER_EXTENSIONS {
        let _ = std::fs::remove_file(dir.join(format!("{id}.{ext}")));
    }
    for ext in ["jpg", "png"] {
        let _ = std::fs::remove_file(dir.join(THUMBNAILS).join(format!("{id}.{ext}")));
    }
}

// ---------------------------------------------------------------- thumbnails

/// How wide the thumbnails covers are shown from are, in pixels: a library card or the item page
/// shows a cover about 210 px wide, and twice that on a HiDPI screen. A cover the window shrinks
/// many times over (they come up to 2000 px and more) is drawn with its lettering broken up; one
/// shrunk here, with a filter made for it, is not.
pub(super) const THUMBNAIL_WIDTH: u32 = 480;
/// The folder beside the covers that holds their thumbnails, named after the cover's file.
const THUMBNAILS: &str = "thumbs";

/// The file the window shows `cover` from: its thumbnail, made when the cover is wider than
/// [`THUMBNAIL_WIDTH`] and made again when the cover is newer; else the cover itself (a small
/// one, an SVG, or a file that cannot be read as a picture). Making one is logged.
pub(super) fn shown_cover(cover: &Path) -> PathBuf {
    match thumbnail(cover) {
        Ok(Some(thumb)) => thumb,
        Ok(None) => cover.to_path_buf(),
        Err(error) => {
            tracing::warn!(
                event = "cover.thumbnail",
                file = %cover.file_name().unwrap_or_default().to_string_lossy(),
                outcome = "failed",
                %error,
                "Could not make a cover's thumbnail; showing the cover"
            );
            cover.to_path_buf()
        }
    }
}

/// `cover`'s thumbnail, made if missing or older than the cover; `None` when the cover is shown
/// as it is.
fn thumbnail(cover: &Path) -> Result<Option<PathBuf>, String> {
    let Some(dir) = cover.parent() else {
        return Ok(None);
    };
    let Some(stem) = cover.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
        return Ok(None);
    };
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let made = ["jpg", "png"]
        .map(|ext| dir.join(THUMBNAILS).join(format!("{stem}.{ext}")))
        .into_iter()
        .find(|thumb| thumb.exists());
    if let Some(thumb) = &made {
        if modified(thumb) >= modified(cover) {
            return Ok(Some(thumb.clone()));
        }
    }
    // A header read: no decoding for a cover small enough, or one that is not a picture.
    let Ok((width, _)) = image::image_dimensions(cover) else {
        return Ok(None);
    };
    if width <= THUMBNAIL_WIDTH {
        if let Some(thumb) = made {
            let _ = std::fs::remove_file(thumb);
        }
        return Ok(None);
    }
    let started = std::time::Instant::now();
    let picture = image::open(cover).map_err(|e| e.to_string())?;
    let small = picture.resize(
        THUMBNAIL_WIDTH,
        u32::MAX,
        image::imageops::FilterType::Lanczos3,
    );
    std::fs::create_dir_all(dir.join(THUMBNAILS)).map_err(|e| e.to_string())?;
    // A cover with see-through parts keeps them (PNG); any other is a JPEG, a tenth the size.
    let thumb = if small.color().has_alpha() {
        let thumb = dir.join(THUMBNAILS).join(format!("{stem}.png"));
        small.save(&thumb).map_err(|e| e.to_string())?;
        thumb
    } else {
        let thumb = dir.join(THUMBNAILS).join(format!("{stem}.jpg"));
        let file = std::fs::File::create(&thumb).map_err(|e| e.to_string())?;
        let mut encoder =
            image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 88);
        encoder
            .encode_image(&small.to_rgb8())
            .map_err(|e| e.to_string())?;
        thumb
    };
    if let Some(other) = made.filter(|m| *m != thumb) {
        let _ = std::fs::remove_file(other);
    }
    tracing::info!(
        event = "cover.thumbnail",
        file = %cover.file_name().unwrap_or_default().to_string_lossy(),
        from = width,
        to = THUMBNAIL_WIDTH,
        took_ms = crate::logging::took(started),
        outcome = "ok",
        "Made a cover's thumbnail"
    );
    Ok(Some(thumb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audiobooks_and_articles_have_no_book_operations_and_audiobooks_no_text() {
        assert_eq!(allows(ItemKind::Book, Needs::Book), Ok(()));
        assert_eq!(allows(ItemKind::Book, Needs::Text), Ok(()));
        assert_eq!(allows(ItemKind::Article, Needs::Text), Ok(()));
        assert_eq!(
            allows(ItemKind::Article, Needs::Book),
            Err(super::super::articles::NOT_FOR_ARTICLES)
        );
        assert_eq!(allows(ItemKind::Audiobook, Needs::Text), Err(NO_TEXT));
        assert_eq!(allows(ItemKind::Audiobook, Needs::Book), Err(NO_TEXT));
    }

    #[test]
    fn every_known_image_type_is_kept_in_its_own_type() {
        for (media_type, ext) in [
            ("image/jpeg", "jpg"),
            ("image/jpg", "jpg"),
            ("Image/PNG", "png"),
            ("image/gif", "gif"),
            ("image/webp", "webp"),
            ("image/svg+xml", "svg"),
            ("image/bmp", "bmp"),
        ] {
            assert_eq!(cover_extension(media_type), Some(ext), "{media_type}");
        }
        assert_eq!(cover_extension("text/html"), None);
    }

    #[test]
    fn an_item_keeps_one_cover_and_an_untyped_one_only_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let png = store_cover(dir.path(), "a", "image/png", b"png", Untyped::Refused).unwrap();
        assert_eq!(png, dir.path().join("a.png"));
        let jpg = store_cover(dir.path(), "a", "image/x-odd", b"jpg", Untyped::Jpeg).unwrap();
        assert_eq!(jpg, dir.path().join("a.jpg"));
        assert!(!png.exists(), "the cover it had in another type goes");
        assert_eq!(std::fs::read(&jpg).unwrap(), b"jpg");

        assert_eq!(
            store_cover(dir.path(), "a", "text/html", b"<html>", Untyped::Refused),
            None
        );
        assert!(jpg.exists(), "a refused image leaves the cover as it was");

        drop_covers(dir.path(), "a");
        assert!(!jpg.exists());
    }

    #[test]
    fn a_cover_that_cannot_be_written_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, b"").unwrap();
        assert_eq!(
            store_cover(&file, "a", "image/png", b"png", Untyped::Jpeg),
            None
        );
    }

    /// A cover `width` × `height`, as a PNG in `dir`.
    fn cover(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = dir.join(name);
        image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        })
        .save(&path)
        .unwrap();
        path
    }

    #[test]
    fn a_large_cover_is_shown_from_a_thumbnail_made_once_and_again_when_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let big = cover(dir.path(), "a.png", 1800, 2700);
        let shown = shown_cover(&big);
        assert_eq!(shown, dir.path().join("thumbs/a.jpg"));
        assert_eq!(
            image::image_dimensions(&shown).unwrap(),
            (THUMBNAIL_WIDTH, THUMBNAIL_WIDTH * 3 / 2),
            "the same shape, as wide as a card needs"
        );
        let made = std::fs::metadata(&shown).unwrap().modified().unwrap();
        assert_eq!(shown_cover(&big), shown);
        assert_eq!(
            std::fs::metadata(&shown).unwrap().modified().unwrap(),
            made,
            "not made again"
        );

        // A new cover, wider than tall.
        std::thread::sleep(std::time::Duration::from_millis(20));
        cover(dir.path(), "a.png", 1600, 900);
        assert_eq!(
            image::image_dimensions(shown_cover(&big)).unwrap(),
            (THUMBNAIL_WIDTH, 270)
        );
    }

    #[test]
    fn a_small_cover_or_one_that_is_not_a_picture_is_shown_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let small = cover(dir.path(), "b.png", 300, 450);
        assert_eq!(shown_cover(&small), small);
        let svg = dir.path().join("c.svg");
        std::fs::write(&svg, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        assert_eq!(shown_cover(&svg), svg);
        let broken = dir.path().join("d.jpg");
        std::fs::write(&broken, b"not a picture").unwrap();
        assert_eq!(shown_cover(&broken), broken);
        assert!(!dir.path().join("thumbs/d.jpg").exists());
    }

    #[test]
    fn dropping_an_items_covers_drops_their_thumbnails() {
        let dir = tempfile::tempdir().unwrap();
        let big = cover(dir.path(), "e.png", 1800, 2700);
        let thumb = shown_cover(&big);
        drop_covers(dir.path(), "e");
        assert!(!big.exists() && !thumb.exists());
    }
}
