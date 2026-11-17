//! Highlights: passages of a book's text the user marked, with a note. The core cuts the text
//! from the chapter (the interface sends a range), joins highlights that overlap or touch, and
//! keeps them on their text when the chapter changes under them.

use std::sync::{Arc, Mutex};

use aoide_library::{BookId, Highlight, Library, NewHighlight};

use super::{lock, now_ms};
use crate::book::BookModel;

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
    ) -> Result<Highlight, String> {
        let text = &book
            .chapters
            .get(chapter)
            .ok_or_else(|| format!("the book has no chapter {chapter}"))?
            .text;
        let length = text.chars().count();
        if start >= end || end > length {
            return Err("that passage is not in the chapter".into());
        }
        let joined: Vec<Highlight> = self
            .stored(&book.id)?
            .into_iter()
            .filter(|h| {
                h.chapter as usize == chapter && h.start as usize <= end && h.end as usize >= start
            })
            .collect();
        let start = joined.iter().map(|h| h.start as usize).fold(start, usize::min);
        let end = joined.iter().map(|h| h.end as usize).fold(end, usize::max);
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
        library
            .highlight(added)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "the highlight was not saved".into())
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
            .unwrap();
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
            .unwrap();
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
}
