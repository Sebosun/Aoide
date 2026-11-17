//! A book's details as the user corrected them by hand: title, authors, cover, publication
//! date. The edit is kept apart from what the file (or a lookup) gave, which is never
//! overwritten, so each field can go back to the file's; it outlives the book, as its voice
//! settings do. [`apply`] is the one rule every reader of an item's title, authors and cover
//! goes through.

use std::path::Path;

use aoide_library::{BookId, BookKey, BookRecord, Library};
use serde::{Deserialize, Serialize};

use super::lock;

/// What the user changed; each field `None` while it is the file's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DetailsEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    /// The chosen image's file name in the covers folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published: Option<String>,
    /// The series' name; empty when the user took the book out of any series.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_position: Option<f32>,
}

/// A detail that can be corrected by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum DetailField {
    Title,
    Authors,
    Cover,
    Published,
    /// Its series or its place in it.
    Series,
}

impl DetailsEdit {
    /// The fields the user corrected.
    pub fn edited(&self) -> Vec<DetailField> {
        [
            (self.title.is_some(), DetailField::Title),
            (self.authors.is_some(), DetailField::Authors),
            (self.cover.is_some(), DetailField::Cover),
            (self.published.is_some(), DetailField::Published),
            (
                self.series.is_some() || self.series_position.is_some(),
                DetailField::Series,
            ),
        ]
        .into_iter()
        .filter_map(|(edited, field)| edited.then_some(field))
        .collect()
    }

    /// The publication date shown: the edit, else what a lookup stored.
    pub fn published(&self, looked_up: Option<String>) -> Option<String> {
        self.published.clone().or(looked_up)
    }
}

/// `record` as it is shown: each corrected field from the edit, the rest as the file gave it.
/// An own cover lives in `covers`.
pub fn apply(mut record: BookRecord, edit: &DetailsEdit, covers: &Path) -> BookRecord {
    if let Some(title) = &edit.title {
        record.title = Some(title.clone());
    }
    if let Some(authors) = &edit.authors {
        record.authors = authors.clone();
    }
    if let Some(cover) = &edit.cover {
        record.cover_path = Some(covers.join(cover));
    }
    if let Some(series) = &edit.series {
        record.series = (!series.is_empty()).then(|| series.clone());
    }
    if let Some(position) = edit.series_position {
        record.series_position = Some(position);
    }
    if record.series.is_none() {
        record.series_position = None;
    }
    record
}

/// A change to an item's corrected details: an absent field stays as it is, `null` goes back
/// to the file's. A blank title or date is `null`; `cover` is the path of an image to use.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct DetailsPatch {
    #[serde(deserialize_with = "super::settings::present")]
    #[cfg_attr(test, ts(optional))]
    pub title: Option<Option<String>>,
    #[serde(deserialize_with = "super::settings::present")]
    #[cfg_attr(test, ts(optional))]
    pub authors: Option<Option<Vec<String>>>,
    #[serde(deserialize_with = "super::settings::present")]
    #[cfg_attr(test, ts(optional))]
    pub cover: Option<Option<String>>,
    #[serde(deserialize_with = "super::settings::present")]
    #[cfg_attr(test, ts(optional))]
    pub published: Option<Option<String>>,
    /// Its series' name; an empty name takes it out of any series.
    #[serde(deserialize_with = "super::settings::present")]
    #[cfg_attr(test, ts(optional))]
    pub series: Option<Option<String>>,
    #[serde(deserialize_with = "super::settings::present")]
    #[cfg_attr(test, ts(optional))]
    pub series_position: Option<Option<f32>>,
}

/// What a cover is kept as when the user picks it.
const OWN_COVER_TYPES: [(image::ImageFormat, &str); 5] = [
    (image::ImageFormat::Jpeg, "jpg"),
    (image::ImageFormat::Png, "png"),
    (image::ImageFormat::WebP, "webp"),
    (image::ImageFormat::Gif, "gif"),
    (image::ImageFormat::Bmp, "bmp"),
];

/// Copies the image at `path` into `covers` as item `id`'s own cover (`{id}-own.{ext}`), in
/// place of one it had in another type; returns the file's name. Anything that does not read
/// as an image is refused.
fn keep_own_cover(covers: &Path, id: &str, path: &Path) -> Result<String, String> {
    const REFUSED: &str =
        "That file is not a picture Aoide can show (JPEG, PNG, WebP, GIF or BMP).";
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let format = image::guess_format(&bytes).map_err(|_| REFUSED)?;
    let ext = OWN_COVER_TYPES
        .iter()
        .find(|(f, _)| *f == format)
        .map(|(_, ext)| *ext)
        .ok_or(REFUSED)?;
    image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| REFUSED)?;
    std::fs::create_dir_all(covers).map_err(|e| e.to_string())?;
    let name = format!("{id}-own.{ext}");
    std::fs::write(covers.join(&name), &bytes).map_err(|e| e.to_string())?;
    drop_own_covers(covers, id, Some(&name));
    Ok(name)
}

/// Deletes item `id`'s own covers in `covers` (and their thumbnails), but `keep`.
fn drop_own_covers(covers: &Path, id: &str, keep: Option<&str>) {
    for (_, ext) in OWN_COVER_TYPES {
        let name = format!("{id}-own.{ext}");
        if keep != Some(name.as_str()) {
            let _ = std::fs::remove_file(covers.join(&name));
        }
        for thumb in ["jpg", "png"] {
            let _ = std::fs::remove_file(
                covers
                    .join(super::item::THUMBNAILS)
                    .join(format!("{id}-own.{thumb}")),
            );
        }
    }
}

fn blank_is_none(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

impl super::Core {
    /// Corrects item `id`'s details by hand (any item: a book, an article, an audiobook).
    /// The file is not changed; a field set back to `null` is the file's again.
    pub fn edit_details(&self, id: &str, patch: DetailsPatch) -> Result<(), String> {
        let book = BookId(id.into());
        if lock(&self.library)
            .book(&book)
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("No such item.".into());
        }
        let mut edit = edit_of(&lock(&self.library), &book);
        let mut fields = Vec::new();
        if let Some(title) = patch.title {
            edit.title = blank_is_none(title);
            fields.push("title");
        }
        if let Some(authors) = patch.authors {
            edit.authors = authors.map(|names| {
                names
                    .into_iter()
                    .map(|n| n.trim().to_owned())
                    .filter(|n| !n.is_empty())
                    .collect()
            });
            fields.push("authors");
        }
        if let Some(published) = patch.published {
            edit.published = blank_is_none(published);
            fields.push("published");
        }
        if let Some(series) = patch.series {
            edit.series = series.map(|name| name.trim().to_owned());
            fields.push("series");
        }
        if let Some(position) = patch.series_position {
            if position.is_some_and(|p| !p.is_finite()) {
                return Err("A place in a series is a number.".into());
            }
            edit.series_position = position;
            fields.push("series_position");
        }
        if let Some(cover) = patch.cover {
            edit.cover = match cover {
                Some(path) => Some(keep_own_cover(
                    &self.paths.covers_dir,
                    id,
                    Path::new(&path),
                )?),
                None => {
                    drop_own_covers(&self.paths.covers_dir, id, None);
                    None
                }
            };
            fields.push("cover");
        }
        {
            let library = lock(&self.library);
            let stored = if edit == DetailsEdit::default() {
                library.remove_book_value(&book, BookKey::Details)
            } else {
                library.set_book_value(&book, BookKey::Details, &edit)
            };
            stored.map_err(|e| e.to_string())?;
        }
        tracing::info!(
            event = "details.edited",
            book = crate::logging::book(id),
            fields = fields.join(","),
            "Corrected an item's details"
        );
        // Its title and authors are read again into the loaded book.
        self.shelf.unload(id);
        self.emit(super::CoreEvent::LibraryChanged);
        Ok(())
    }

    /// Item `id`'s record as it is shown: corrected details applied.
    pub(crate) fn shown_record(&self, id: &str) -> Option<BookRecord> {
        shown(
            &lock(&self.library),
            &BookId(id.into()),
            &self.paths.covers_dir,
        )
    }
}

/// Book `id`'s edit, or none (one that cannot be read is logged and treated as none).
pub fn edit_of(library: &Library, id: &BookId) -> DetailsEdit {
    library
        .book_value(id, BookKey::Details)
        .unwrap_or_else(|error| {
            tracing::warn!(event = "library.read", book = crate::logging::book(&id.0), %error, "Could not read the book's corrected details");
            None
        })
        .unwrap_or_default()
}

/// Book `id`'s record as it is shown, edit applied.
pub fn shown(library: &Library, id: &BookId, covers: &Path) -> Option<BookRecord> {
    let record = library.book(id).ok()??;
    Some(apply(record, &edit_of(library, id), covers))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use aoide_library::{BookStatus, ItemKind};

    use super::*;

    fn record() -> BookRecord {
        BookRecord {
            id: BookId("b".into()),
            path: "untitled_final(2).pdf".into(),
            status: BookStatus::Present,
            title: Some("untitled_final(2)".into()),
            authors: vec!["Unknown".into()],
            language: None,
            cover_path: Some("/data/covers/b.jpg".into()),
            added_at: 0,
            last_opened_at: None,
            finished_at: None,
            word_count: None,
            kind: ItemKind::Book,
            shelf: "library".into(),
            archived_at: None,
            series: None,
            series_position: None,
        }
    }

    #[test]
    fn each_corrected_field_wins_and_the_rest_are_the_files() {
        let covers = Path::new("/data/covers");
        type Shown = (Option<String>, Vec<String>, Option<PathBuf>);
        let shown = |edit: &DetailsEdit| -> Shown {
            let r = apply(record(), edit, covers);
            (r.title, r.authors, r.cover_path)
        };
        let file: Shown = (
            Some("untitled_final(2)".into()),
            vec!["Unknown".into()],
            Some("/data/covers/b.jpg".into()),
        );
        let cases: [(DetailsEdit, Shown, Vec<DetailField>); 4] = [
            (DetailsEdit::default(), file.clone(), vec![]),
            (
                DetailsEdit {
                    title: Some("The Quay Lights".into()),
                    ..DetailsEdit::default()
                },
                (
                    Some("The Quay Lights".into()),
                    file.1.clone(),
                    file.2.clone(),
                ),
                vec![DetailField::Title],
            ),
            (
                DetailsEdit {
                    authors: Some(vec!["Ada Brightwater".into(), "Tom Keel".into()]),
                    cover: Some("b-own.png".into()),
                    ..DetailsEdit::default()
                },
                (
                    file.0.clone(),
                    vec!["Ada Brightwater".into(), "Tom Keel".into()],
                    Some("/data/covers/b-own.png".into()),
                ),
                vec![DetailField::Authors, DetailField::Cover],
            ),
            (
                // No authors at all is a correction too.
                DetailsEdit {
                    authors: Some(vec![]),
                    ..DetailsEdit::default()
                },
                (file.0.clone(), vec![], file.2.clone()),
                vec![DetailField::Authors],
            ),
        ];
        for (edit, expected, edited) in cases {
            assert_eq!(shown(&edit), expected, "{edit:?}");
            assert_eq!(edit.edited(), edited, "{edit:?}");
        }
    }

    #[test]
    fn a_corrected_series_wins_and_an_empty_one_is_none() {
        let covers = Path::new("/data/covers");
        let mut file = record();
        file.series = Some("Harbour Lights".into());
        file.series_position = Some(2.0);
        let shown = |edit: DetailsEdit| {
            let r = apply(file.clone(), &edit, covers);
            (r.series, r.series_position, edit.edited())
        };
        assert_eq!(
            shown(DetailsEdit::default()),
            (Some("Harbour Lights".into()), Some(2.0), vec![])
        );
        assert_eq!(
            shown(DetailsEdit {
                series_position: Some(3.0),
                ..DetailsEdit::default()
            }),
            (
                Some("Harbour Lights".into()),
                Some(3.0),
                vec![DetailField::Series]
            )
        );
        assert_eq!(
            shown(DetailsEdit {
                series: Some("The Lamp Room".into()),
                ..DetailsEdit::default()
            }),
            (
                Some("The Lamp Room".into()),
                Some(2.0),
                vec![DetailField::Series]
            )
        );
        // In no series, by hand.
        assert_eq!(
            shown(DetailsEdit {
                series: Some(String::new()),
                ..DetailsEdit::default()
            }),
            (None, None, vec![DetailField::Series])
        );
    }

    #[test]
    fn the_publication_date_is_the_edit_else_the_looked_up_one() {
        let none = DetailsEdit::default();
        assert_eq!(none.published(None), None);
        assert_eq!(none.published(Some("1998".into())).as_deref(), Some("1998"));
        let edit = DetailsEdit {
            published: Some("2001".into()),
            ..DetailsEdit::default()
        };
        assert_eq!(edit.published(Some("1998".into())).as_deref(), Some("2001"));
        assert_eq!(edit.edited(), [DetailField::Published]);
    }
}

#[cfg(test)]
mod core_tests {
    use super::super::article_tests::Pages;
    use super::super::tests::{library_folder, start_fetching};
    use super::*;
    use aoide_tts::testing::FakeEngine;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn patch(json: serde_json::Value) -> DetailsPatch {
        serde_json::from_value(json).unwrap()
    }

    #[tokio::test]
    async fn a_corrected_title_and_authors_are_shown_everywhere_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let id = library_folder(dir.path(), "");
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), Pages::with(&[]));
        core.rescan().unwrap();
        let summary = |core: &super::super::Core| {
            core.list_books()
                .unwrap()
                .into_iter()
                .find(|b| b.id == id)
                .unwrap()
        };
        let file_title = summary(&core).title;
        assert!(summary(&core).edited.is_empty());

        core.edit_details(
            &id,
            patch(serde_json::json!({
                "title": "  untitled_final(2) given its name ",
                "authors": ["Ada Brightwater", " ", "Tom Keel"],
                "published": "1998",
            })),
        )
        .unwrap();
        let shown = summary(&core);
        assert_eq!(shown.title, "untitled_final(2) given its name");
        assert_eq!(shown.authors, ["Ada Brightwater", "Tom Keel"]);
        assert_eq!(
            shown.edited,
            [
                DetailField::Title,
                DetailField::Authors,
                DetailField::Published
            ]
        );
        // The loaded book (the reader, now playing, both exports, the lookup) says the same.
        let book = core.open_book(&id).unwrap();
        assert_eq!(book.title, "untitled_final(2) given its name");
        assert_eq!(book.authors, ["Ada Brightwater", "Tom Keel"]);
        assert_eq!(
            core.book_details(&id).unwrap().published.as_deref(),
            Some("1998")
        );

        // A rescan does not undo it; the file is not changed.
        core.rescan().unwrap();
        assert_eq!(summary(&core).title, "untitled_final(2) given its name");

        // Cleared, a field is the file's again; the others stay.
        core.edit_details(&id, patch(serde_json::json!({ "title": null })))
            .unwrap();
        let shown = summary(&core);
        assert_eq!(shown.title, file_title);
        assert_eq!(shown.authors, ["Ada Brightwater", "Tom Keel"]);
        assert_eq!(core.open_book(&id).unwrap().title, file_title);

        // Removed and added again, the book keeps its corrections, as it keeps its voice.
        let file = core.shelf.file(&id).unwrap();
        let copy = dir.path().join("again.epub");
        std::fs::copy(&file, &copy).unwrap();
        core.remove_book(&id).unwrap();
        core.import_files(vec![copy.to_string_lossy().into_owned()]);
        assert_eq!(summary(&core).authors, ["Ada Brightwater", "Tom Keel"]);

        assert!(core
            .edit_details("no-such-book", DetailsPatch::default())
            .is_err());
    }

    #[tokio::test]
    async fn an_own_cover_is_set_replaced_and_cleared_back_to_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let id = library_folder(dir.path(), "");
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), Pages::with(&[]));
        core.rescan().unwrap();
        let cover = |core: &super::super::Core| {
            core.list_books()
                .unwrap()
                .into_iter()
                .find(|b| b.id == id)
                .unwrap()
                .cover_path
        };
        let file_cover = cover(&core);
        let image = dir.path().join("lantern.png");
        std::fs::write(&image, png(3, 4)).unwrap();
        let set = |path: &Path| {
            core.edit_details(
                &id,
                patch(serde_json::json!({ "cover": path.to_string_lossy() })),
            )
        };
        set(&image).unwrap();
        let own = dir.path().join(format!("data/covers/{id}-own.png"));
        assert!(own.exists(), "kept in the covers folder");
        assert_eq!(
            cover(&core).as_deref(),
            Some(own.to_string_lossy().as_ref())
        );
        assert_eq!(
            core.shelf.cover(&id).as_deref(),
            Some(own.to_string_lossy().as_ref())
        );

        // Another image replaces it, in its own type.
        let jpeg = dir.path().join("lantern.jpg");
        image::load_from_memory(&png(2, 2))
            .unwrap()
            .save(&jpeg)
            .unwrap();
        set(&jpeg).unwrap();
        assert!(!own.exists());
        assert!(dir
            .path()
            .join(format!("data/covers/{id}-own.jpg"))
            .exists());

        // Something that is not an image is refused, and the cover stays.
        let text = dir.path().join("notes.png");
        std::fs::write(&text, "not a picture").unwrap();
        assert!(set(&text).is_err());
        assert!(dir
            .path()
            .join(format!("data/covers/{id}-own.jpg"))
            .exists());

        core.edit_details(&id, patch(serde_json::json!({ "cover": null })))
            .unwrap();
        assert_eq!(cover(&core), file_cover);
        assert!(!dir
            .path()
            .join(format!("data/covers/{id}-own.jpg"))
            .exists());
    }

    #[tokio::test]
    async fn an_articles_corrected_title_survives_a_refetch() {
        use super::super::article_tests::{LIGHTS, LIGHTS_URL};
        use super::super::tests::until;
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let id = core.save_article(LIGHTS_URL).unwrap();
        let ready = |core: &super::super::Core| {
            core.article(&id)
                .is_ok_and(|a| a.stage == super::super::fetching::Stage::Ready)
        };
        until("fetched", || ready(&core)).await;
        core.edit_details(
            &id,
            patch(serde_json::json!({ "title": "Lights on the Quay" })),
        )
        .unwrap();
        core.refetch_article(&id, super::super::ArticleSource::Http)
            .unwrap();
        until("fetched again", || pages.asks() == 2 && ready(&core)).await;
        let title = core
            .list_books()
            .unwrap()
            .into_iter()
            .find(|b| b.id == id)
            .unwrap()
            .title;
        assert_eq!(title, "Lights on the Quay");
    }
}
