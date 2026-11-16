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

/// Deletes item `id`'s cover in `dir`, in whichever type it was kept.
pub(super) fn drop_covers(dir: &Path, id: &str) {
    for ext in COVER_EXTENSIONS {
        let _ = std::fs::remove_file(dir.join(format!("{id}.{ext}")));
    }
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
}
