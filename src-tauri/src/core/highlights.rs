//! Highlights: passages of a book's text the user marked, with a note. The core cuts the text
//! from the chapter (the interface sends a range), joins highlights that overlap or touch, and
//! keeps them on their text when the chapter changes under them.

use std::sync::{Arc, Mutex};

use aoide_library::{BookId, Highlight, ItemKind, Library, NewHighlight};
use serde::Serialize;

use super::item::Needs;
use super::{lock, now_ms, Core};
use crate::book::BookModel;

/// Pressed within this long of a sentence starting, the highlight key means the sentence before
/// it, the one just heard.
pub(super) const JUST_HEARD_MS: u32 = 1_000;

/// The longest passage a highlight holds, in characters.
pub(super) const MAX_CHARS: usize = 2_000;

/// How a highlight was asked for, as `highlight.added` logs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum From {
    Selection,
    Spoken,
}

impl From {
    fn name(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Spoken => "spoken",
        }
    }
}

/// A highlight as the reader and the Notes list show it, in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct HighlightView {
    pub id: i64,
    pub chapter: usize,
    pub chapter_title: String,
    /// Characters of the chapter's display text (Unicode scalar values), as `SegmentView.starts`.
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub note: String,
    /// Its text is no longer in the chapter: listed, not drawn.
    pub detached: bool,
}

/// Where `highlight`'s text is in `book` now: its stored range when the text is still there,
/// else the place in its chapter nearest to it where the text is found, else `None`.
fn anchor(book: &BookModel, highlight: &Highlight) -> Option<(usize, usize)> {
    let text = &book.chapters.get(highlight.chapter as usize)?.text;
    let (start, end) = (highlight.start as usize, highlight.end as usize);
    if cut(text, start, end.min(text.chars().count()).max(start)) == highlight.text {
        return Some((start, end));
    }
    let length = highlight.text.chars().count();
    text.match_indices(highlight.text.as_str())
        .map(|(at, _)| text[..at].chars().count())
        .min_by_key(|at| at.abs_diff(start))
        .map(|at| (at, at + length))
}

/// Characters `start..end` of `text` (Unicode scalar values).
fn cut(text: &str, start: usize, end: usize) -> String {
    text.chars().skip(start).take(end - start).collect()
}

/// The highlights of every book.
pub(super) struct Highlights {
    library: Arc<Mutex<Library>>,
}

impl Highlights {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        Self { library }
    }

    fn stored(&self, book: &str) -> Result<Vec<Highlight>, String> {
        lock(&self.library)
            .highlights(&BookId(book.into()))
            .map_err(|e| e.to_string())
    }

    /// Book `book`'s highlights as stored, in reading order, for when its text cannot be read
    /// (its file is gone): not anchored, and without chapter titles.
    pub fn unanchored(&self, book: &str) -> Result<Vec<HighlightView>, String> {
        Ok(self
            .stored(book)?
            .into_iter()
            .map(|h| HighlightView {
                id: h.id,
                chapter: h.chapter as usize,
                chapter_title: String::new(),
                start: h.start as usize,
                end: h.end as usize,
                text: h.text,
                note: h.note,
                detached: false,
            })
            .collect())
    }

    /// Book `book`'s highlights in reading order, each anchored on its text (see [`anchor`]): one
    /// whose text moved is moved with it, and the new place stored; one whose text is gone is
    /// `detached`.
    pub fn list(&self, book: &BookModel) -> Result<Vec<HighlightView>, String> {
        let mut views = Vec::new();
        for highlight in self.stored(&book.id)? {
            let chapter = highlight.chapter as usize;
            let place = anchor(book, &highlight);
            match place {
                Some((start, end)) if start != highlight.start as usize => {
                    lock(&self.library)
                        .move_highlight(highlight.id, start as u32, end as u32)
                        .map_err(|e| e.to_string())?;
                }
                Some(_) => {}
                None => tracing::info!(
                    event = "highlight.detached",
                    book = crate::logging::book(&book.id),
                    chapter,
                    "A highlight's text is no longer in its chapter"
                ),
            }
            let (start, end) = place.unwrap_or((highlight.start as usize, highlight.end as usize));
            views.push(HighlightView {
                id: highlight.id,
                chapter,
                chapter_title: book
                    .chapters
                    .get(chapter)
                    .map(|c| c.title.clone())
                    .unwrap_or_default(),
                start,
                end,
                text: highlight.text,
                note: highlight.note,
                detached: place.is_none(),
            });
        }
        views.sort_by_key(|h| (h.chapter, h.start, h.id));
        Ok(views)
    }

    /// Highlights the sentence at playlist entry `index` of `book`, `offset_ms` into its audio;
    /// in its first [`JUST_HEARD_MS`], the sentence before it in the same chapter (the key is
    /// pressed after hearing it). Silent entries and spoken footnotes are passed over.
    pub fn add_spoken(
        &self,
        book: &BookModel,
        index: usize,
        offset_ms: u32,
    ) -> Result<Added, String> {
        let (chapter, local) = book.locate(index).ok_or("nothing is playing")?;
        let segments = &book.chapters[chapter].segments;
        let in_text = |s: &aoide_text::Segment| !s.speech.is_empty() && !s.range.is_empty();
        let at = in_text(&segments[local]).then_some(local);
        let before = segments[..local].iter().rposition(in_text);
        let chosen = if offset_ms < JUST_HEARD_MS {
            before.or(at)
        } else {
            at.or(before)
        };
        let segment = &segments[chosen.ok_or("there is no sentence here to highlight")?];
        self.add(
            book,
            chapter,
            segment.range.start,
            segment.range.end,
            From::Spoken,
        )
    }

    /// Highlights characters `start..end` of chapter `chapter` of `book`, joined with the
    /// highlights it overlaps or touches. Refused outside the chapter, and for more than
    /// [`MAX_CHARS`] characters.
    pub fn add(
        &self,
        book: &BookModel,
        chapter: usize,
        start: usize,
        end: usize,
        from: From,
    ) -> Result<Added, String> {
        let text = &book
            .chapters
            .get(chapter)
            .ok_or_else(|| format!("the book has no chapter {chapter}"))?
            .text;
        let length = text.chars().count();
        if start >= end || end > length {
            return Err("that passage is not in the chapter".into());
        }
        let joined: Vec<HighlightView> = self
            .list(book)?
            .into_iter()
            .filter(|h| !h.detached && h.chapter == chapter && h.start <= end && h.end >= start)
            .collect();
        let start = joined.iter().map(|h| h.start).fold(start, usize::min);
        let end = joined.iter().map(|h| h.end).fold(end, usize::max);
        let chars = end - start;
        if chars > MAX_CHARS {
            return Err(format!(
                "a highlight holds at most {MAX_CHARS} characters; this is {chars}"
            ));
        }
        let note = joined
            .iter()
            .map(|h| h.note.as_str())
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let replacing: Vec<i64> = joined.iter().map(|h| h.id).collect();
        let id = BookId(book.id.clone());
        let library = lock(&self.library);
        let added = library
            .add_highlight(
                &id,
                &NewHighlight {
                    chapter: chapter as u32,
                    start: start as u32,
                    end: end as u32,
                    text: cut(text, start, end),
                    note,
                },
                &replacing,
                now_ms() / 1000,
            )
            .map_err(|e| e.to_string())?;
        tracing::info!(
            event = "highlight.added",
            book = crate::logging::book(&book.id),
            chapter,
            chars,
            from = from.name(),
            "Highlighted a passage"
        );
        let highlight = library
            .highlight(added)
            .map_err(|e| e.to_string())?
            .ok_or("the highlight was not saved")?;
        Ok(Added {
            highlight,
            joined: !replacing.is_empty(),
        })
    }

    /// Writes highlight `id`'s note; returns its book.
    pub fn update_note(&self, id: i64, note: &str) -> Result<String, String> {
        let library = lock(&self.library);
        let found = library.highlight(id).map_err(|e| e.to_string())?;
        let found = found.ok_or("that highlight is not there any more")?;
        library
            .update_highlight_note(id, note.trim())
            .map_err(|e| e.to_string())?;
        Ok(found.book_id.0)
    }

    /// Removes highlight `id`; returns its book.
    pub fn delete(&self, id: i64) -> Result<String, String> {
        let library = lock(&self.library);
        let found = library.highlight(id).map_err(|e| e.to_string())?;
        let found = found.ok_or("that highlight is not there any more")?;
        library.delete_highlight(id).map_err(|e| e.to_string())?;
        tracing::info!(
            event = "highlight.removed",
            book = crate::logging::book(&found.book_id.0),
            chapter = found.chapter,
            "Removed a highlight"
        );
        Ok(found.book_id.0)
    }
}

/// A highlight just made.
pub(super) struct Added {
    pub highlight: Highlight,
    /// It took the place of highlights it overlapped or touched.
    pub joined: bool,
}

/// What the highlight key made: the highlight, whether removing it undoes the key (it joined
/// no other highlight, whose passage and note removing it would lose too), and the book's
/// highlights now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct HighlightAdded {
    pub id: i64,
    pub can_undo: bool,
    pub highlights: Vec<HighlightView>,
}

impl Core {
    /// Book `id`'s highlights in reading order; none for an audiobook, which has no text.
    pub fn highlights(&self, id: &str) -> Result<Vec<HighlightView>, String> {
        if self.item_kind(id) == Some(ItemKind::Audiobook) {
            return Ok(Vec::new());
        }
        self.highlights.list(&*self.load_book(id)?)
    }

    /// Highlights characters `start..end` of chapter `chapter` of book `id` (a selection in
    /// the reader); returns the book's highlights.
    pub fn add_highlight(
        &self,
        id: &str,
        chapter: usize,
        start: usize,
        end: usize,
    ) -> Result<Vec<HighlightView>, String> {
        self.require(id, Needs::Text)?;
        let book = self.load_book(id)?;
        self.highlights
            .add(&book, chapter, start, end, From::Selection)?;
        self.highlights.list(&book)
    }

    /// Highlights the sentence just spoken in book `id`, which must be the one playing.
    pub fn highlight_spoken(&self, id: &str) -> Result<HighlightAdded, String> {
        self.require(id, Needs::Text)?;
        let book = self
            .playback
            .book()
            .filter(|book| book.id == id)
            .ok_or("this book is not being listened to")?;
        let at = self.playback.position();
        let added = self.highlights.add_spoken(&book, at.index, at.offset_ms)?;
        Ok(HighlightAdded {
            id: added.highlight.id,
            can_undo: !added.joined,
            highlights: self.highlights.list(&book)?,
        })
    }

    /// Writes a highlight's note; returns its book's highlights.
    pub fn update_highlight_note(
        &self,
        highlight: i64,
        note: &str,
    ) -> Result<Vec<HighlightView>, String> {
        let book = self.highlights.update_note(highlight, note)?;
        self.highlights(&book)
    }

    /// Removes a highlight; returns its book's highlights.
    pub fn delete_highlight(&self, highlight: i64) -> Result<Vec<HighlightView>, String> {
        let book = self.highlights.delete(highlight)?;
        self.highlights(&book)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{epub, library_with};
    use super::*;

    fn book() -> BookModel {
        BookModel::parse("b", &epub(), &Default::default()).unwrap()
    }

    /// Where `words` start and end in chapter `chapter`'s text, in characters.
    fn range_of(book: &BookModel, chapter: usize, words: &str) -> (usize, usize) {
        let text = &book.chapters[chapter].text;
        let at = text.find(words).expect("the words are in the chapter");
        let start = text[..at].chars().count();
        (start, start + words.chars().count())
    }

    fn passages(highlights: &Highlights) -> Vec<(u32, String)> {
        highlights
            .stored("b")
            .unwrap()
            .into_iter()
            .map(|h| (h.chapter, h.text))
            .collect()
    }

    #[test]
    fn the_core_cuts_the_text_of_a_highlight_from_the_chapter() {
        let highlights = Highlights::new(library_with(&["b"]));
        let book = book();
        let (start, end) = range_of(&book, 1, "lit the lamp");
        let added = highlights
            .add(&book, 1, start, end, From::Selection)
            .unwrap()
            .highlight;
        assert_eq!(added.text, "lit the lamp");
        assert_eq!((added.start as usize, added.end as usize), (start, end));
        assert_eq!(passages(&highlights), [(1, "lit the lamp".into())]);
    }

    #[test]
    fn a_highlight_that_overlaps_or_touches_another_joins_it_and_keeps_its_note() {
        let highlights = Highlights::new(library_with(&["b"]));
        let book = book();
        let (start, end) = range_of(&book, 0, "Gulls argued");
        let first = highlights
            .add(&book, 0, start, end, From::Selection)
            .unwrap()
            .highlight;
        lock(&highlights.library)
            .update_highlight_note(first.id, "noisy")
            .unwrap();
        let (start, end) = range_of(&book, 0, "argued over");
        highlights
            .add(&book, 0, start, end, From::Selection)
            .unwrap();
        let (start, end) = range_of(&book, 0, " the nets.");
        let joined = highlights
            .add(&book, 0, start, end, From::Selection)
            .unwrap();
        assert!(joined.joined);
        let joined = joined.highlight;
        assert_eq!(joined.text, "Gulls argued over the nets.");
        assert_eq!(joined.note, "noisy");
        // Not touching: a highlight of its own.
        let (start, end) = range_of(&book, 0, "slowly");
        highlights
            .add(&book, 0, start, end, From::Selection)
            .unwrap();
        // The same words in another chapter are another passage.
        let (start, end) = range_of(&book, 1, "lamp");
        highlights
            .add(&book, 1, start, end, From::Selection)
            .unwrap();
        assert_eq!(
            passages(&highlights),
            [
                (0, "Gulls argued over the nets.".into()),
                (0, "slowly".into()),
                (1, "lamp".into())
            ]
        );
    }

    #[test]
    fn a_range_outside_the_chapter_or_too_long_is_refused() {
        let highlights = Highlights::new(library_with(&["b"]));
        let book = book();
        let length = book.chapters[0].text.chars().count();
        assert!(highlights
            .add(&book, 0, 0, length + 1, From::Selection)
            .is_err());
        assert!(highlights.add(&book, 0, 4, 4, From::Selection).is_err());
        assert!(highlights.add(&book, 9, 0, 1, From::Selection).is_err());
        let long = "word ".repeat(450);
        let long = BookModel::parse(
            "b",
            &super::super::tests::epub_of_chapters(&[("Long", &long)]),
            &Default::default(),
        )
        .unwrap();
        let length = long.chapters[0].text.chars().count();
        assert!(length > MAX_CHARS);
        assert!(highlights
            .add(&long, 0, 0, MAX_CHARS + 1, From::Selection)
            .is_err());
        assert!(highlights
            .add(&long, 0, 0, MAX_CHARS, From::Selection)
            .is_ok());
        assert_eq!(passages(&highlights).len(), 1);
    }

    #[test]
    fn a_highlight_stays_on_its_text_when_the_chapter_changes_under_it() {
        let highlights = Highlights::new(library_with(&["b"]));
        let was = book();
        for words in ["Gulls argued", "lit the lamp", "until morning"] {
            let chapter = usize::from(!words.starts_with("Gulls"));
            let (start, end) = range_of(&was, chapter, words);
            highlights
                .add(&was, chapter, start, end, From::Selection)
                .unwrap();
        }
        // Fetched again: the lamp is lit later in the chapter, the morning is gone.
        let chapter = |title: &str, body: &str| {
            format!(
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>{title}</h1><p>{body}</p></body></html>"#
            )
        };
        let now = BookModel::parse(
            "b",
            &super::super::tests::epub_with(
                chapter(
                    "The Quay",
                    "Gulls argued over the nets. The tide came in slowly.",
                ),
                chapter(
                    "The Lamp",
                    "At last someone lit the lamp at dusk. It burned all night.",
                ),
            ),
            &Default::default(),
        )
        .unwrap();
        let listed = highlights.list(&now).unwrap();
        let seen: Vec<(&str, bool)> = listed
            .iter()
            .map(|h| (h.text.as_str(), h.detached))
            .collect();
        assert_eq!(
            seen,
            [
                ("Gulls argued", false),
                ("lit the lamp", false),
                ("until morning", true)
            ]
        );
        assert_eq!(listed[1].chapter_title, "The Lamp");
        let moved = range_of(&now, 1, "lit the lamp");
        assert_eq!((listed[1].start, listed[1].end), moved);
        // Written back: the stored range is the new one.
        let stored = highlights.stored("b").unwrap();
        let lamp = stored.iter().find(|h| h.text == "lit the lamp").unwrap();
        assert_eq!((lamp.start as usize, lamp.end as usize), moved);
        // Unchanged where nothing moved.
        assert_eq!(
            (listed[0].start, listed[0].end),
            range_of(&was, 0, "Gulls argued")
        );
    }

    #[test]
    fn the_spoken_key_highlights_the_sentence_at_the_playhead_or_the_one_just_heard() {
        let highlights = Highlights::new(library_with(&["b"]));
        let document = |body: &str| {
            format!(r#"<html xmlns="http://www.w3.org/1999/xhtml"><body>{body}</body></html>"#)
        };
        let book = BookModel::parse(
            "b",
            &super::super::tests::epub_with(
                document("<h1>The Quay</h1><p>Gulls argued over the nets.</p><p>The tide came in slowly.</p>"),
                document("<h1>The Lamp</h1><p>Someone lit the lamp at dusk.</p>"),
            ),
            &Default::default(),
        )
        .unwrap();
        let entry = |id: &str| book.index_of(id).unwrap();
        let spoken = |index: usize, offset_ms: u32| {
            let added = highlights
                .add_spoken(&book, index, offset_ms)
                .unwrap()
                .highlight;
            lock(&highlights.library)
                .delete_highlight(added.id)
                .unwrap();
            added.text
        };
        // Well into a sentence: that sentence.
        assert_eq!(spoken(entry("0:2:0"), 2_500), "The tide came in slowly.");
        // In its first second: the one before, which was just heard.
        assert_eq!(spoken(entry("0:2:0"), 400), "Gulls argued over the nets.");
        // Not back into the chapter before: a chapter's first sentence is itself.
        assert_eq!(spoken(entry("1:0:0"), 400), "The Lamp");
        assert!(highlights.add_spoken(&book, 99, 0).is_err());
    }

    #[tokio::test]
    async fn the_reader_highlights_notes_and_removes_through_the_core() {
        use super::super::tests::{library_folder, start_with, until};
        let dir = tempfile::tempdir().unwrap();
        let id = library_folder(dir.path(), "");
        let (core, _events) = start_with(
            dir.path(),
            &aoide_tts::testing::FakeEngine::new(),
            Arc::new(|_| Err("no speech recognition in this test".into())),
        );
        until("the library scan", || {
            core.list_books().is_ok_and(|b| b.len() == 1)
        })
        .await;
        let book = core.load_book(&id).unwrap();
        let (start, end) = range_of(&book, 1, "lit the lamp");
        let listed = core.add_highlight(&id, 1, start, end).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].text.as_str(), listed[0].chapter_title.as_str()),
            ("lit the lamp", "The Lamp")
        );
        let noted = core
            .update_highlight_note(listed[0].id, " bright ")
            .unwrap();
        assert_eq!(noted[0].note, "bright");
        assert_eq!(core.highlights(&id).unwrap(), noted);

        // Not playing: nothing was just spoken.
        assert!(core.highlight_spoken(&id).is_err());
        core.play(&id, Some(0), None).await.unwrap();
        core.pause();
        let spoken = core.highlight_spoken(&id).unwrap();
        assert!(spoken.can_undo, "it joined nothing");
        assert_eq!(spoken.highlights.len(), 2);
        assert_eq!(spoken.highlights[0].id, spoken.id);
        assert_eq!(spoken.highlights[0].chapter, 0);

        let left = core.delete_highlight(spoken.id).unwrap();
        assert_eq!(left, noted);
        assert!(core.delete_highlight(spoken.id).is_err());
        assert!(core.update_highlight_note(spoken.id, "gone").is_err());
        assert!(core.add_highlight(&id, 1, end, end + 999).is_err());
    }
}
