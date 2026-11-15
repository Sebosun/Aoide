//! Audiobooks on their own: a recording with no EPUB, kept in the library as an item of kind
//! `Audiobook`. Its recording sits in the sync stage `Standalone` (nothing to sync with) and it
//! plays straight through on its timeline. The files are only read, never copied or changed.

use std::path::{Path, PathBuf};

use aoide_library::{BookId, ItemKind, NewBook};

use super::recordings::Recordings;
use super::{lock, now_ms, Core, CoreEvent, ImportResult};

/// Where an audiobook item says it is: the one file or folder chosen, else the folder holding
/// its first file.
fn item_path(chosen: &[PathBuf], files: &[PathBuf]) -> PathBuf {
    match chosen {
        [one] => one.clone(),
        _ => files
            .first()
            .and_then(|f| f.parent())
            .map_or_else(PathBuf::new, Path::to_owned),
    }
}

/// What a text-only operation says about an audiobook.
pub(super) const NO_TEXT: &str = "Not available for an audiobook: it has no text.";

impl Core {
    /// Refuses item `id` when it is an audiobook, which has no text.
    pub(super) fn require_text(&self, id: &str) -> Result<(), String> {
        match self.kind_of(id) {
            Some(ItemKind::Audiobook) => Err(NO_TEXT.into()),
            _ => Ok(()),
        }
    }

    /// Adds the audiobook at `paths` (an M4B, audio files, or a folder of them) as an item with
    /// no text. DRM-protected and unreadable audio is refused with the reasons the pairing
    /// gives. The same files added again, from anywhere, are the same item.
    pub fn add_audiobook(&self, paths: Vec<String>) -> ImportResult {
        let shown = paths.first().cloned().unwrap_or_default();
        let result = self.store_audiobook(paths.into_iter().map(PathBuf::from).collect());
        let (book_id, error) = match result {
            Ok(id) => (Some(id), None),
            Err(error) => (None, Some(error)),
        };
        tracing::info!(
            event = "library.audiobook",
            book = book_id.as_deref().map(crate::logging::book),
            outcome = if error.is_some() { "refused" } else { "ok" },
            error = error.as_deref(),
            "Add an audiobook"
        );
        ImportResult {
            path: shown,
            book_id,
            error,
        }
    }

    fn store_audiobook(&self, chosen: Vec<PathBuf>) -> Result<String, String> {
        let found = aoide_recording::inspect(&chosen);
        if let Some(refusal) = Recordings::refusal(&found) {
            return Err(refusal);
        }
        let files: Vec<PathBuf> = found.files.iter().map(|f| f.path.clone()).collect();
        let id = aoide_recording::fingerprint(&files).map_err(|e| e.to_string())?;
        let path = item_path(&chosen, &files);
        let title = found
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| aoide_discovery::title_from_name(&path));
        lock(&self.library)
            .upsert_book(
                &NewBook {
                    id: BookId(id.clone()),
                    kind: ItemKind::Audiobook,
                    path,
                    title: Some(title),
                    authors: found.author.clone().into_iter().collect(),
                    language: None,
                    cover_path: None,
                    error: None,
                },
                now_ms(),
            )
            .map_err(|e| e.to_string())?;
        self.shelf.prefer_recording(&id)?;
        self.recordings.add_standalone(self, &id, found)?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(id)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use aoide_library::ItemKind;
    use aoide_tts::testing::FakeEngine;

    use super::super::facets::AudioSource;
    use super::super::syncing::Stage;
    use super::super::tests::{library_folder, start_with, until};
    use super::super::{Core, ReadingMode, Shape};

    /// The recording crate's tagged two-file set ("The Lantern Keeper" by "A. Writer"), copied
    /// into `dir`.
    pub(in crate::core) fn tagged_set(dir: &Path) -> std::path::PathBuf {
        let set =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/recording/tests/fixtures/set");
        let to = dir.join("Audio/Lantern_Keeper (Unabridged)");
        std::fs::create_dir_all(&to).unwrap();
        for name in ["a-second.mp3", "b-first.mp3"] {
            std::fs::copy(set.join(name), to.join(name)).unwrap();
        }
        to
    }

    async fn core(root: &Path) -> Arc<Core> {
        library_folder(root, "");
        let (core, _events) = start_with(
            root,
            &FakeEngine::new(),
            Arc::new(|_| Err("no speech recognition in this test".into())),
        );
        until("the library scan", || {
            core.list_books().is_ok_and(|b| b.len() == 1)
        })
        .await;
        core
    }

    #[tokio::test]
    async fn an_audiobook_is_added_on_its_own_and_once() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        let folder = tagged_set(dir.path());

        let added = core.add_audiobook(vec![folder.to_string_lossy().into()]);
        assert_eq!(added.error, None);
        let id = added.book_id.unwrap();
        let books = core.list_books().unwrap();
        assert_eq!(books.len(), 2);
        let item = books.iter().find(|b| b.id == id).unwrap();
        assert_eq!(item.kind, ItemKind::Audiobook);
        assert_eq!(item.title, "The Lantern Keeper");
        assert_eq!(item.authors, ["A. Writer"]);
        assert_eq!(item.audio, AudioSource::Recording);
        assert_eq!(
            (item.mode, item.shape),
            (ReadingMode::Listen, Shape::Listen)
        );
        assert_eq!(core.recording(&id).unwrap().stage, Stage::Standalone);
        assert!(!core.sync_step(), "nothing to sync");

        // The same files again, picked one by one this time: the same item.
        let files: Vec<String> = ["b-first.mp3", "a-second.mp3"]
            .iter()
            .map(|f| folder.join(f).to_string_lossy().into_owned())
            .collect();
        assert_eq!(core.add_audiobook(files).book_id, Some(id.clone()));
        assert_eq!(core.list_books().unwrap().len(), 2);

        // A rescan of the library folder leaves it alone while its folder is there.
        core.rescan().unwrap();
        let status = |core: &Core| {
            let books = core.list_books().unwrap();
            books.into_iter().find(|b| b.id == id).unwrap().status
        };
        assert_eq!(status(&core), aoide_library::BookStatus::Present);
        let moved = dir.path().join("Audio/moved");
        std::fs::rename(&folder, &moved).unwrap();
        core.rescan().unwrap();
        assert_eq!(status(&core), aoide_library::BookStatus::Missing);
        std::fs::rename(&moved, &folder).unwrap();
        core.rescan().unwrap();
        assert_eq!(status(&core), aoide_library::BookStatus::Present);
    }

    #[tokio::test]
    async fn an_untagged_audiobook_is_named_after_its_file_and_drm_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        let folder = tagged_set(dir.path());
        let untagged = dir.path().join("Audio/Harbour_Notes [MP3].flac");
        std::fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../crates/recording/tests/fixtures/tone.flac"),
            &untagged,
        )
        .unwrap();
        let added = core.add_audiobook(vec![untagged.to_string_lossy().into()]);
        let id = added.book_id.expect("added");
        let item = core
            .list_books()
            .unwrap()
            .into_iter()
            .find(|b| b.id == id)
            .unwrap();
        assert_eq!(item.title, "Harbour Notes");

        // An old Audible file: refused with its reason, and nothing is added.
        let protected = folder.join("old.aa");
        let mut aa = vec![0u8; 64];
        aa[4..8].copy_from_slice(&0x5790_7536u32.to_be_bytes());
        std::fs::write(&protected, aa).unwrap();
        let refused = core.add_audiobook(vec![protected.to_string_lossy().into()]);
        assert_eq!(refused.book_id, None);
        assert!(refused.error.unwrap().contains("DRM"));
        let empty = dir.path().join("Empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(core
            .add_audiobook(vec![empty.to_string_lossy().into()])
            .error
            .is_some());
        assert_eq!(core.list_books().unwrap().len(), 2, "the book and the FLAC");
    }

    #[tokio::test]
    async fn text_only_operations_refuse_an_audiobook_and_change_nothing() {
        use super::super::recording::Narration;
        use super::NO_TEXT;

        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        let folder = tagged_set(dir.path());
        let id = core
            .add_audiobook(vec![folder.to_string_lossy().into()])
            .book_id
            .unwrap();
        let files = vec![folder.join("b-first.mp3").to_string_lossy().into_owned()];
        let refused = |result: Result<(), String>, what: &str| {
            assert_eq!(result, Err(NO_TEXT.to_string()), "{what}");
        };
        refused(core.chapter(&id, 0).map(drop), "chapter");
        refused(core.search_book(&id, "lantern").map(drop), "search_book");
        refused(core.prepare_book(&id).await, "prepare_book");
        refused(core.prepare_chapter(&id, 0).await, "prepare_chapter");
        refused(core.chapter_audio(&id).await.map(drop), "chapter_audio");
        refused(core.regenerate_book(&id).await, "regenerate_book");
        refused(core.export_book(&id).await.map(drop), "export_book");
        refused(
            core.export_read_along(&id).await.map(drop),
            "export_read_along",
        );
        refused(
            core.lookup_metadata(&id, false).await.map(drop),
            "lookup_metadata",
        );
        refused(
            core.set_reading_place(&id, 0, "0:0:0").map(drop),
            "set_reading_place",
        );
        refused(core.set_mode(&id, ReadingMode::Read).map(drop), "set_mode");
        refused(
            core.attach_recording(&id, files.clone()).map(drop),
            "attach_recording",
        );
        refused(
            core.attach_recording_as_listed(&id, files).map(drop),
            "attach_recording_as_listed",
        );
        refused(core.remove_recording(&id).await, "remove_recording");
        refused(
            core.set_narration(&id, Narration::Speech).await,
            "set_narration",
        );
        refused(core.play_recording_at(&id, 0).await, "play_recording_at");

        assert!(
            !core.engine_status().wanted,
            "nothing needed the speech engine"
        );
        assert!(core.prepare_view(&id).is_none());
        assert_eq!(core.recording(&id).unwrap().stage, Stage::Standalone);
        let view = core.book_overview(&id).unwrap();
        assert_eq!(
            (view.mode, view.shape),
            (ReadingMode::Listen, Shape::Listen)
        );
        assert_eq!(core.list_books().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn an_audiobook_shows_its_parts_and_takes_time_bookmarks() {
        let dir = tempfile::tempdir().unwrap();
        let core = core(dir.path()).await;
        let folder = tagged_set(dir.path());
        let id = core
            .add_audiobook(vec![folder.to_string_lossy().into()])
            .book_id
            .unwrap();
        let view = core.open_book(&id).unwrap();
        assert_eq!(view.title, "The Lantern Keeper");
        assert_eq!(view.word_count, 0);
        assert!(view.warnings.is_empty(), "{:?}", view.warnings);
        assert_eq!(view.chapters.len(), 2, "one part per file");
        assert!(view
            .chapters
            .iter()
            .all(|c| c.duration_ms.is_some_and(|ms| ms > 0) && !c.title.is_empty()));
        assert!(core.book_details(&id).is_ok());

        // Not playing: the bookmark is the saved place.
        let second = view.chapters[1].clone();
        let start = view.chapters[0].duration_ms.unwrap();
        core.seek_to(&id, start + 100).unwrap();
        let bookmark = core.add_bookmark(&id, " By the door ").unwrap();
        assert_eq!(
            (bookmark.chapter, bookmark.position_ms, bookmark.segment_id),
            (1, Some(start + 100), None)
        );
        assert_eq!(
            (bookmark.excerpt, bookmark.note),
            (second.title, "By the door".into())
        );
        assert_eq!(core.bookmarks(&id).unwrap().len(), 1);

        // A file gone: the view says so instead of failing.
        std::fs::remove_file(folder.join("a-second.mp3")).unwrap();
        let view = core.book_overview(&id).unwrap();
        assert!(view.chapters.is_empty());
        assert_eq!(view.warnings, ["a-second.mp3 is missing."]);
    }
}
