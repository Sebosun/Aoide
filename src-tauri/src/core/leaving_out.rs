//! Chapters left out of the narration: a listening choice, kept in the book's settings
//! ([`super::BookSettings::left_out`]). Their entries get [`super::narration::Fate::LeftOut`],
//! so playback passes over them, whole-book preparation and export leave them out and the
//! listening estimates do not count them. They are still there to read. Front and back matter
//! is only proposed ([`aoide_text::front_and_back_matter`]); nothing is left out until the user
//! agrees.
//!
//! This is not the parser's excluded sections (`ExcludedView`): that is extraction, this is a
//! choice about what is heard.

use std::ops::Range;

use crate::book::{BookModel, ChapterSummary};

/// The entries of `book`'s chapters in `chapters`.
pub(super) fn entries(book: &BookModel, chapters: &[usize]) -> Vec<Range<usize>> {
    chapters
        .iter()
        .filter_map(|c| book.chapters.get(*c))
        .map(|c| c.first_entry..c.first_entry + c.segments.len())
        .collect()
}

/// The entries of the chapter entry `index` is in.
pub(super) fn chapter_of(book: &BookModel, index: usize) -> Option<Range<usize>> {
    let (chapter, _) = book.locate(index)?;
    entries(book, &[chapter]).pop()
}

/// The chapter `delta` narrated chapters away from `from`, of `count`: as far as there are
/// narrated chapters that way, and `from` itself when there are none.
pub(super) fn skip(
    from: usize,
    delta: i64,
    count: usize,
    narrated: impl Fn(usize) -> bool,
) -> usize {
    let mut target = from;
    let mut remaining = delta.unsigned_abs();
    let mut chapter = from;
    while remaining > 0 {
        chapter = match (delta > 0).then(|| chapter + 1) {
            Some(after) if after < count => after,
            Some(_) => break,
            None => match chapter.checked_sub(1) {
                Some(before) => before,
                None => break,
            },
        };
        if narrated(chapter) {
            target = chapter;
            remaining -= 1;
        }
    }
    target
}

/// `target` passes over the chapters in `left_out`.
pub(super) fn leave_out(target: &mut super::Target, left_out: &[usize]) {
    let ranges = entries(&target.book, left_out);
    target.narration.leave_out(&ranges);
}

/// The chapters of `book` in `chosen` that it has, in order, each once.
pub(super) fn within(book: &BookModel, chosen: &[usize]) -> Vec<usize> {
    let mut chapters: Vec<usize> = chosen
        .iter()
        .copied()
        .filter(|c| *c < book.chapters.len())
        .collect();
    chapters.sort_unstable();
    chapters.dedup();
    chapters
}

/// The book's chapters as listed, each marked left out (in `left_out`) and suggested (its
/// title names front or back matter).
pub(super) fn chapters(book: &BookModel, left_out: &[usize]) -> Vec<ChapterSummary> {
    let suggested = suggested(book);
    let mut chapters = book.chapter_summaries();
    for chapter in &mut chapters {
        chapter.left_out = left_out.contains(&chapter.index);
        chapter.suggested = suggested.contains(&chapter.index);
    }
    chapters
}

/// The chapters of `book` whose titles name front or back matter.
pub(super) fn suggested(book: &BookModel) -> Vec<usize> {
    let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
    aoide_text::front_and_back_matter(&titles, book.language.as_deref())
}

/// Words read aloud in the chapters not in `left_out`.
pub(super) fn narrated_words(book: &BookModel, left_out: &[usize]) -> usize {
    (0..book.chapters.len())
        .filter(|c| !left_out.contains(c))
        .map(|c| book.words_in(c))
        .sum()
}

impl super::Core {
    /// Leaves `chapters` of book `id` out of the narration (and only those), saved with its
    /// other settings: the book listened to plays on by the new choice, and its preparation
    /// is planned again.
    pub async fn set_left_out(
        &self,
        id: &str,
        chapters: Vec<usize>,
    ) -> Result<super::BookView, String> {
        let book = self.load_book(id)?;
        let left_out = within(&book, &chapters);
        let count = left_out.len();
        let settings = super::BookSettings {
            left_out,
            ..self.book_settings(id)
        };
        self.store_book_settings(id, settings).await?;
        tracing::info!(
            event = "narration.left_out",
            book = crate::logging::book(id),
            chapters = count,
            "Chapters left out of the narration"
        );
        self.book_overview(id)
    }
}

#[cfg(test)]
mod tests {
    use aoide_tts::EngineStatus;

    use super::super::engine_tests::{start_holding, Scripted};
    use super::super::tests::{epub_of_chapters, until};
    use super::super::{Core, ReadState};

    /// A book with front and back matter around two chapters.
    pub(in crate::core) fn book_with_matter() -> Vec<u8> {
        epub_of_chapters(&[
            ("Contents", "The Quay. The Lamp."),
            (
                "The Quay",
                "Gulls argued over the nets. The tide came in slowly.",
            ),
            (
                "The Lamp",
                "Someone lit the lamp at dusk. It burned until morning.",
            ),
            ("Notes", "Quay: a stone landing where boats tie up."),
        ])
    }

    /// Core with the book with matter, its id and its engine.
    async fn started(
        root: &std::path::Path,
    ) -> (std::sync::Arc<Core>, String, std::sync::Arc<Scripted>) {
        let engine = Scripted::new("main", EngineStatus::Ready(None), &["A.wav"], false);
        let (core, _events, id) =
            start_holding(root, &[&engine], "voiceId = \"A.wav\"", &book_with_matter()).await;
        (core, id, engine)
    }

    /// Whether the engine was asked for anything of chapter `chapter`'s text.
    fn asked_for(core: &Core, id: &str, engine: &Scripted, chapter: usize) -> bool {
        let book = core.load_book(id).unwrap();
        let model = &book.chapters[chapter];
        let texts = engine.texts();
        model
            .segments
            .iter()
            .filter(|s| !s.speech.is_empty())
            .any(|s| texts.iter().any(|t| t.contains(&s.speech)))
    }

    #[tokio::test]
    async fn playback_reaching_a_chapter_left_out_carries_on_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let (core, id, engine) = started(dir.path()).await;
        core.set_left_out(&id, vec![2]).await.unwrap();

        core.play(&id, Some(1), None).await.unwrap();
        until("playback to reach the notes", || {
            core.player_state().chapter == 3
        })
        .await;
        assert!(!asked_for(&core, &id, &engine, 2), "{:?}", engine.texts());
        assert!(asked_for(&core, &id, &engine, 3));
        core.stop();
    }

    #[tokio::test]
    async fn the_last_chapter_left_out_ends_the_book_where_it_begins() {
        let dir = tempfile::tempdir().unwrap();
        let (core, id, engine) = started(dir.path()).await;
        core.set_left_out(&id, vec![3]).await.unwrap();

        core.play(&id, Some(2), None).await.unwrap();
        until("the book to be finished", || {
            core.list_books().unwrap()[0].read_state == ReadState::Finished
        })
        .await;
        assert!(!asked_for(&core, &id, &engine, 3), "{:?}", engine.texts());
        core.stop();
    }

    #[tokio::test]
    async fn skipping_chapters_steps_over_those_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let (core, id, _engine) = started(dir.path()).await;
        core.set_left_out(&id, vec![0, 2, 3]).await.unwrap();

        core.play(&id, Some(1), None).await.unwrap();
        core.pause();
        core.skip_chapter(1).await.unwrap();
        assert_eq!(core.player_state().chapter, 1, "nothing narrated after it");
        core.skip_chapter(-1).await.unwrap();
        assert_eq!(core.player_state().chapter, 1, "nothing narrated before it");
        core.set_left_out(&id, vec![2]).await.unwrap();
        core.skip_chapter(1).await.unwrap();
        assert_eq!(core.player_state().chapter, 3);
        core.skip_chapter(-1).await.unwrap();
        assert_eq!(core.player_state().chapter, 1);
        core.stop();
    }

    fn marks(core: &super::super::Core, id: &str) -> Vec<(bool, bool)> {
        core.book_overview(id)
            .unwrap()
            .chapters
            .iter()
            .map(|c| (c.left_out, c.suggested))
            .collect()
    }

    #[tokio::test]
    async fn chapters_left_out_are_listed_and_kept_when_the_book_is_removed_and_added_again() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Scripted::new("main", EngineStatus::Ready(None), &["A.wav"], false);
        let (core, _events, id) = start_holding(
            dir.path(),
            &[&engine],
            "voiceId = \"A.wav\"",
            &book_with_matter(),
        )
        .await;
        assert_eq!(
            marks(&core, &id),
            [(false, true), (false, false), (false, false), (false, true)],
            "front and back matter is proposed, nothing is left out yet"
        );
        let all_words = core.book_overview(&id).unwrap().word_count;

        let view = core.set_left_out(&id, vec![3, 0, 0, 9]).await.unwrap();
        assert_eq!(
            view.chapters_left_out, 2,
            "out of range and repeated ones dropped"
        );
        assert_eq!(core.book_settings(&id).left_out, [0, 3]);
        assert_eq!(
            marks(&core, &id),
            [(true, true), (false, false), (false, false), (true, true)]
        );
        assert_eq!(view.word_count, all_words, "they are still there to read");
        assert!(view.narrated_word_count < all_words);

        core.remove_book(&id).unwrap();
        let file = dir.path().join("Books/quay-lights.epub");
        let added = core.import_files(vec![file.to_string_lossy().into_owned()]);
        assert_eq!(added[0].book_id.as_deref(), Some(id.as_str()), "{added:?}");
        assert_eq!(
            marks(&core, &id),
            [(true, true), (false, false), (false, false), (true, true)]
        );

        let older = super::super::BookSettings {
            left_out: vec![],
            ..core.book_settings(&id)
        };
        core.save_book_settings(&id, older).await.unwrap();
        assert_eq!(
            core.book_settings(&id).left_out,
            [0, 3],
            "the voice settings saved from an older form leave them as they are"
        );

        core.set_left_out(&id, vec![]).await.unwrap();
        assert_eq!(core.book_overview(&id).unwrap().chapters_left_out, 0);
    }
}
