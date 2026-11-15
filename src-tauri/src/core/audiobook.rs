//! Audiobooks on their own: a recording with no EPUB, kept in the library as an item of kind
//! `Audiobook`. Its recording sits in the sync stage `Standalone` (nothing to sync with) and it
//! plays straight through on its timeline. The files are only read, never copied or changed.

use std::path::{Path, PathBuf};

use aoide_library::{BookId, BookKey, ItemKind, NewBook};

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

    /// Gives audiobook `audiobook` the EPUB it reads, book `book` (already in the library, with
    /// no recording): the book takes its recording (which starts syncing with the text), its
    /// bookmarks, collections and listening time, and keeps its place until the sync reaches
    /// it; the audiobook item is removed.
    pub fn merge_audiobook(&self, audiobook: &str, book: &str) -> Result<(), String> {
        if self.kind_of(audiobook) != Some(ItemKind::Audiobook) {
            return Err("That is not an audiobook on its own.".into());
        }
        if self.kind_of(book) != Some(ItemKind::Book) {
            return Err("That is not a book with text.".into());
        }
        if self.recording_stage(book).is_some() {
            return Err("That book already has a recording.".into());
        }
        if self.playback.is_book(audiobook) {
            self.stop();
        }
        let merged = lock(&self.library)
            .merge_audiobook(&BookId(audiobook.into()), &BookId(book.into()))
            .map_err(|e| e.to_string());
        tracing::info!(
            event = "library.merge",
            book = crate::logging::book(book),
            from = crate::logging::book(audiobook),
            outcome = if merged.is_ok() { "ok" } else { "failed" },
            error = merged.as_ref().err(),
            "Give an audiobook its text"
        );
        // On failure nothing moved, and the audiobook plays on as it was.
        merged?;
        // Its recording was standalone, so no sync work runs for it.
        self.recordings.forget_recording(audiobook);
        self.shelf.prefer_recording(book)?;
        self.recordings.text_attached(self, book)?;
        self.emit(CoreEvent::LibraryChanged);
        Ok(())
    }

    /// The place book `id` kept from the audiobook it was merged from becomes its progress
    /// once the sync has reached it: the first synced sentence whose audio goes on past it
    /// (once all of it is synced, the last one with audio).
    pub(super) fn settle_pending_place(&self, id: &str, ready: bool) {
        let pending = lock(&self.library)
            .book_value::<u64>(&BookId(id.into()), BookKey::PendingPlace)
            .ok()
            .flatten();
        let Some(at) = pending else { return };
        let Ok(book) = self.load_book(id) else { return };
        let Ok((plan, synced)) = self.recordings.playable(self, &book) else {
            return;
        };
        let clips = &plan.clips[..synced.min(plan.clips.len())];
        let heard = |c: &&aoide_sync::Clip| !c.audio.is_empty();
        let index = match clips.iter().position(|c| heard(&c) && c.audio.end > at) {
            Some(index) => index,
            None if ready => match clips.iter().rposition(|c| heard(&c)) {
                Some(index) => index,
                None => return,
            },
            None => return,
        };
        self.drop_pending_place(id, "settled");
        let stored = self.store_place(super::playback::Place::Entry { book, index });
        if let Err(error) = stored {
            self.report("progress.save", "Could not save your place", error);
        }
    }

    /// Forgets book `id`'s pending place, if it has one: it was `outcome` ("settled" into its
    /// progress, or "dropped" since the user read or listened first).
    pub(super) fn drop_pending_place(&self, id: &str, outcome: &str) {
        let book = BookId(id.into());
        let library = lock(&self.library);
        if !matches!(
            library.book_value::<u64>(&book, BookKey::PendingPlace),
            Ok(Some(_))
        ) {
            return;
        }
        if let Err(error) = library.remove_book_value(&book, BookKey::PendingPlace) {
            tracing::warn!(event = "progress.pending", book = crate::logging::book(id), %error, "Could not forget a pending place");
            return;
        }
        tracing::info!(
            event = "progress.pending",
            book = crate::logging::book(id),
            outcome,
            "Place kept from an audiobook"
        );
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
