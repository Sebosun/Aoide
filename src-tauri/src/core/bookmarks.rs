//! Bookmarks: a saved place in a book, with an excerpt and an optional note.

use super::*;

use aoide_library::Bookmark;

fn excerpt(text: &str) -> String {
    const MAX: usize = 160;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX {
        return text;
    }
    let cut: String = text.chars().take(MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}…")
}

/// The reader's bookmarks.
pub(super) struct Bookmarks {
    library: Arc<Mutex<Library>>,
}

impl Bookmarks {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        Self { library }
    }

    pub fn list(&self, id: &str) -> Result<Vec<Bookmark>, String> {
        lock(&self.library)
            .bookmarks(&BookId(id.into()))
            .map_err(|e| e.to_string())
    }

    /// Bookmarks entry `playing` of `book` when it is playing, else the book's saved position.
    pub fn add(
        &self,
        book: &BookModel,
        playing: Option<usize>,
        note: &str,
    ) -> Result<Bookmark, String> {
        let id = BookId(book.id.clone());
        let index = match playing {
            Some(index) => index.min(book.total_entries().saturating_sub(1)),
            None => {
                let progress = lock(&self.library)
                    .progress(&id)
                    .map_err(|e| e.to_string())?;
                progress
                    .and_then(|p| {
                        p.segment_id
                            .as_deref()
                            .and_then(|s| book.index_of(s))
                            .or_else(|| book.index_at(p.chapter as usize, p.char_offset as usize))
                    })
                    .or_else(|| book.chapter_start(0))
                    .unwrap_or(0)
            }
        };
        let (chapter, local) = book.locate(index).ok_or("the book has no text")?;
        let segment = &book.chapters[chapter].segments[local];
        let library = lock(&self.library);
        let bookmark_id = library
            .add_bookmark(
                &id,
                chapter as u32,
                segment.range.start as u32,
                Some(&segment.id),
                &excerpt(&segment.display),
                note.trim(),
                now_ms() / 1000,
            )
            .map_err(|e| e.to_string())?;
        library
            .bookmarks(&id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|b| b.id == bookmark_id)
            .ok_or_else(|| "bookmark was not saved".into())
    }

    /// Bookmarks `position_ms` in `part` of audiobook `id`, titled by the part.
    pub fn add_time(
        &self,
        id: &str,
        part: u32,
        position_ms: u64,
        title: &str,
        note: &str,
    ) -> Result<Bookmark, String> {
        let id = BookId(id.into());
        let library = lock(&self.library);
        let bookmark_id = library
            .add_time_bookmark(&id, part, position_ms, title, now_ms() / 1000)
            .map_err(|e| e.to_string())?;
        if !note.trim().is_empty() {
            library
                .update_bookmark_note(bookmark_id, note.trim())
                .map_err(|e| e.to_string())?;
        }
        library
            .bookmarks(&id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|b| b.id == bookmark_id)
            .ok_or_else(|| "bookmark was not saved".into())
    }

    pub fn update_note(&self, bookmark: i64, note: &str) -> Result<(), String> {
        let found = lock(&self.library)
            .update_bookmark_note(bookmark, note.trim())
            .map_err(|e| e.to_string())?;
        found
            .then_some(())
            .ok_or_else(|| "bookmark not found".into())
    }

    pub fn delete(&self, bookmark: i64) -> Result<(), String> {
        lock(&self.library)
            .delete_bookmark(bookmark)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

impl Core {
    pub fn bookmarks(&self, id: &str) -> Result<Vec<Bookmark>, String> {
        self.bookmarks.list(id)
    }

    /// Bookmarks the playhead when `id` is playing, else the book's saved position.
    pub fn add_bookmark(&self, id: &str, note: &str) -> Result<Bookmark, String> {
        if self.item_kind(id) == Some(aoide_library::ItemKind::Audiobook) {
            let (part, position_ms, title) = self.audiobook_place(id)?;
            return self
                .bookmarks
                .add_time(id, part as u32, position_ms, &title, note);
        }
        let playing = self
            .playback
            .is_book(id)
            .then(|| self.playback.position().index);
        let book = self.load_book(id)?;
        self.bookmarks.add(&book, playing, note)
    }

    pub fn update_bookmark_note(&self, bookmark: i64, note: &str) -> Result<(), String> {
        self.bookmarks.update_note(bookmark, note)
    }

    pub fn delete_bookmark(&self, bookmark: i64) -> Result<(), String> {
        self.bookmarks.delete(bookmark)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{epub, library_with};
    use super::*;

    fn book() -> BookModel {
        BookModel::parse("b", &epub(), &Default::default()).unwrap()
    }

    #[test]
    fn a_bookmark_while_playing_marks_the_playhead() {
        let bookmarks = Bookmarks::new(library_with(&["b"]));
        let book = book();
        let last = book.total_entries() - 1;
        let mark = bookmarks.add(&book, Some(last), "  a note ").unwrap();
        let (chapter, _) = book.locate(last).unwrap();
        assert_eq!(mark.chapter, chapter as u32);
        assert_eq!(mark.note, "a note");
        assert_eq!(bookmarks.list("b").unwrap().len(), 1);
    }

    #[test]
    fn a_bookmark_of_a_book_not_playing_marks_its_start_when_nothing_is_saved() {
        let bookmarks = Bookmarks::new(library_with(&["b"]));
        let mark = bookmarks.add(&book(), None, "").unwrap();
        assert_eq!(mark.chapter, 0);
        bookmarks.delete(mark.id).unwrap();
        assert!(bookmarks.list("b").unwrap().is_empty());
    }

    #[test]
    fn excerpt_collapses_whitespace_and_cuts_at_a_word() {
        assert_eq!(super::excerpt("A  short\n line."), "A short line.");
        let long = "word ".repeat(60);
        let cut = super::excerpt(&long);
        assert!(cut.ends_with("word…"), "{cut}");
        assert!(cut.chars().count() <= 161);
    }
}
