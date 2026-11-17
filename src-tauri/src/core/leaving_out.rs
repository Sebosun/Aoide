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
fn chapter_of(book: &BookModel, index: usize) -> Option<Range<usize>> {
    let (chapter, _) = book.locate(index)?;
    entries(book, &[chapter]).pop()
}

/// Playback is sent to entry `index` of `target`: its chapter plays even if it is left out.
/// Returns the entries that play now.
pub(super) fn admit(target: &mut super::Target, index: usize) -> Vec<usize> {
    match chapter_of(&target.book, index) {
        Some(chapter) => target.narration.admit(chapter),
        None => Vec::new(),
    }
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

/// Whole-book preparation and export make only the chapters not in `left_out`.
pub(super) fn narrated_only(
    plan: &mut super::preparation::Plan,
    book: &BookModel,
    left_out: &[usize],
) {
    plan.only(|index| {
        book.locate(index)
            .map_or(true, |(c, _)| !left_out.contains(&c))
    });
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
        let before = self.book_settings(id);
        let taken_back = before.left_out.iter().any(|c| !left_out.contains(c));
        let settings = super::BookSettings { left_out, ..before };
        self.store_book_settings(id, settings).await?;
        if taken_back {
            self.plan(super::PlanEvent::ChaptersTakenBack(id.into()));
            // A prepared book shows as prepared no longer.
            self.emit(super::CoreEvent::LibraryChanged);
        }
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
    use super::super::tests::{book_with_matter, until};
    use super::super::{Core, CoreEvent, PrepareStage, ReadState};

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

    /// The spoken segments of chapter `chapter`.
    fn spoken(core: &Core, id: &str, chapter: usize) -> Vec<String> {
        let book = core.load_book(id).unwrap();
        book.chapters[chapter]
            .segments
            .iter()
            .filter(|s| !s.speech.is_empty())
            .map(|s| s.id.clone())
            .collect()
    }

    #[tokio::test]
    async fn listening_to_a_book_with_its_front_matter_left_out_starts_at_chapter_one() {
        let dir = tempfile::tempdir().unwrap();
        let (core, id, engine) = started(dir.path()).await;
        core.set_left_out(&id, vec![0]).await.unwrap();

        core.play(&id, None, None).await.unwrap();
        until("chapter one to be said", || {
            asked_for(&core, &id, &engine, 1)
        })
        .await;
        assert!(!asked_for(&core, &id, &engine, 0), "{:?}", engine.texts());
        core.stop();
    }

    #[tokio::test]
    async fn a_sentence_clicked_in_a_chapter_left_out_plays_it_and_then_carries_on() {
        let dir = tempfile::tempdir().unwrap();
        let (core, id, engine) = started(dir.path()).await;
        core.set_left_out(&id, vec![0, 2]).await.unwrap();

        let contents = spoken(&core, &id, 0);
        core.play(&id, Some(0), Some(contents[1].clone()))
            .await
            .unwrap();
        assert_eq!(core.player_state().segment_id, Some(contents[1].clone()));
        until("the contents to be said", || {
            asked_for(&core, &id, &engine, 0)
        })
        .await;
        until("playback to reach the notes", || {
            core.player_state().chapter == 3
        })
        .await;
        assert!(
            !asked_for(&core, &id, &engine, 2),
            "the lamp is still left out"
        );

        // In the same session: sent into the lamp, it is said.
        let lamp = spoken(&core, &id, 2);
        core.play(&id, Some(2), Some(lamp[0].clone()))
            .await
            .unwrap();
        until("the lamp to be said", || asked_for(&core, &id, &engine, 2)).await;
        until("playback to reach the notes again", || {
            core.player_state().chapter == 3
        })
        .await;
        core.stop();
    }

    fn stage(core: &Core, id: &str) -> Option<PrepareStage> {
        core.prepare_view(id).map(|v| v.state)
    }

    #[tokio::test]
    async fn preparing_the_book_leaves_its_chapters_left_out_out_and_is_done_without_them() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Scripted::new("main", EngineStatus::Ready(None), &["A.wav"], false);
        let (core, events, id) = start_holding(
            dir.path(),
            &[&engine],
            "voiceId = \"A.wav\"",
            &book_with_matter(),
        )
        .await;
        core.set_left_out(&id, vec![0, 3]).await.unwrap();
        engine.hold_synthesis();

        core.prepare_book(&id).await.unwrap();
        let narrated = spoken(&core, &id, 1).len() + spoken(&core, &id, 2).len();
        until("the plan", || {
            core.prepare_view(&id).is_some_and(|v| v.total > 0)
        })
        .await;
        assert_eq!(core.prepare_view(&id).unwrap().total, narrated);
        engine.let_go();
        until("the book to be prepared", || {
            stage(&core, &id) == Some(PrepareStage::Done)
        })
        .await;
        assert!(asked_for(&core, &id, &engine, 1) && asked_for(&core, &id, &engine, 2));
        assert!(!asked_for(&core, &id, &engine, 0) && !asked_for(&core, &id, &engine, 3));

        // Asked for, a chapter left out is still prepared.
        core.prepare_chapter(&id, 3).await.unwrap();
        until("the notes to be made", || asked_for(&core, &id, &engine, 3)).await;
        assert!(!asked_for(&core, &id, &engine, 0));
        until("the notes to be prepared", || {
            stage(&core, &id) == Some(PrepareStage::Done)
        })
        .await;

        // Leaving out more keeps it prepared.
        let told = events.count(CoreEvent::LIBRARY_CHANGED);
        core.set_left_out(&id, vec![0, 2, 3]).await.unwrap();
        assert_eq!(stage(&core, &id), Some(PrepareStage::Done));
        assert_eq!(events.count(CoreEvent::LIBRARY_CHANGED), told);
        core.set_left_out(&id, vec![0, 3]).await.unwrap();
        // Taken back in, the contents are not made yet: the book is not prepared, and the
        // library says so.
        let told = events.count(CoreEvent::LIBRARY_CHANGED);
        core.set_left_out(&id, vec![3]).await.unwrap();
        assert_eq!(stage(&core, &id), None);
        assert_eq!(
            events.count(CoreEvent::LIBRARY_CHANGED),
            told + 1,
            "the library shows the book is no longer prepared"
        );
        core.prepare_book(&id).await.unwrap();
        until("the book to be prepared again", || {
            stage(&core, &id) == Some(PrepareStage::Done)
        })
        .await;
        assert!(asked_for(&core, &id, &engine, 0));
    }

    #[tokio::test]
    async fn an_export_has_no_audio_for_the_chapters_left_out() {
        use super::super::export::ExportHost;
        let dir = tempfile::tempdir().unwrap();
        let (core, id, _engine) = started(dir.path()).await;
        core.set_left_out(&id, vec![0, 3]).await.unwrap();

        let (book, plan) = ExportHost::speech_plan(&*core, &id).unwrap();
        let exported: Vec<usize> = (0..book.total_entries())
            .filter(|i| plan.items[*i].speakable)
            .filter_map(|i| book.locate(i).map(|(c, _)| c))
            .collect();
        assert!(exported.contains(&1) && exported.contains(&2));
        assert!(exported.iter().all(|c| *c == 1 || *c == 2), "{exported:?}");
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
