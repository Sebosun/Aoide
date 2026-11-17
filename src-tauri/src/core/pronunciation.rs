//! The pronunciation lists: how to say a written word, for one book or for all books. The page
//! keeps the book's spelling; the rules go into each request through [`super::voicing::Voicing`].

use std::sync::{Arc, Mutex};

use aoide_library::{BookId, Library, Pronunciation};
use aoide_text::Rule;

use super::{lock, now_ms};

/// Words separated by single spaces, if any has a letter or digit.
fn words(text: &str) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    text.chars().any(char::is_alphanumeric).then_some(text)
}

/// The pronunciation lists.
pub(super) struct Pronunciations {
    library: Arc<Mutex<Library>>,
}

impl Pronunciations {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        Self { library }
    }

    /// Book `book`'s list, or with `None` the list for all books, oldest first.
    pub fn list(&self, book: Option<&str>) -> Result<Vec<Pronunciation>, String> {
        let book = book.map(|b| BookId(b.into()));
        lock(&self.library)
            .pronunciations(book.as_ref())
            .map_err(|e| e.to_string())
    }

    /// Adds `written` to book `book`'s list (with `None`, the list for all books), or changes
    /// how the list says it.
    pub fn save(
        &self,
        book: Option<&str>,
        written: &str,
        spoken: &str,
        match_case: bool,
    ) -> Result<Pronunciation, String> {
        let written = words(written).ok_or("say which word is written")?;
        let spoken = words(spoken).ok_or("say how the word is said")?;
        let book = book.map(|b| BookId(b.into()));
        lock(&self.library)
            .save_pronunciation(book.as_ref(), &written, &spoken, match_case, now_ms())
            .map_err(|e| e.to_string())
    }

    /// Removes entry `id` from its list and returns it.
    pub fn remove(&self, id: i64) -> Result<Pronunciation, String> {
        let library = lock(&self.library);
        let entry = library
            .pronunciation(id)
            .map_err(|e| e.to_string())?
            .ok_or("no such pronunciation")?;
        library
            .delete_pronunciation(id)
            .map_err(|e| e.to_string())?;
        Ok(entry)
    }

    /// The rules book `book` is said with: its own list, then the list for all books.
    pub fn rules(&self, book: &str) -> Result<Arc<[Rule]>, String> {
        let entries = lock(&self.library)
            .pronunciations_for(&BookId(book.into()))
            .map_err(|e| e.to_string())?;
        Ok(entries
            .into_iter()
            .map(|p| Rule {
                written: p.written,
                spoken: p.spoken,
                match_case: p.match_case,
            })
            .collect())
    }
}

impl super::Core {
    /// Book `book`'s pronunciation list, or with `None` the list for all books.
    pub fn pronunciations(&self, book: Option<&str>) -> Result<Vec<Pronunciation>, String> {
        self.pronunciations.list(book)
    }

    /// Adds a word to a pronunciation list, or changes how it is said.
    pub fn save_pronunciation(
        &self,
        book: Option<&str>,
        written: &str,
        spoken: &str,
        match_case: bool,
    ) -> Result<Pronunciation, String> {
        self.pronunciations.save(book, written, spoken, match_case)
    }

    pub fn delete_pronunciation(&self, id: i64) -> Result<(), String> {
        self.pronunciations.remove(id).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::library_with;
    use super::*;

    fn said(list: Vec<Pronunciation>) -> Vec<(String, String)> {
        list.into_iter().map(|p| (p.written, p.spoken)).collect()
    }

    #[test]
    fn a_word_is_added_to_a_books_list_or_the_one_for_all_books_changed_and_removed() {
        let lists = Pronunciations::new(library_with(&["b"]));
        let xu = lists
            .save(Some("b"), " Dr.   Xu ", "doctor  shoe", false)
            .unwrap();
        assert_eq!(
            (xu.written.as_str(), xu.spoken.as_str()),
            ("Dr. Xu", "doctor shoe")
        );
        lists.save(None, "SQL", "sequel", true).unwrap();

        let changed = lists.save(Some("b"), "Dr. Xu", "doctor zoo", true).unwrap();
        assert_eq!(changed.id, xu.id);
        assert_eq!(
            said(lists.list(Some("b")).unwrap()),
            [("Dr. Xu".into(), "doctor zoo".into())]
        );
        assert_eq!(
            said(lists.list(None).unwrap()),
            [("SQL".into(), "sequel".into())]
        );

        let removed = lists.remove(xu.id).unwrap();
        assert_eq!(removed.book_id, Some(BookId("b".into())));
        assert!(lists.list(Some("b")).unwrap().is_empty());
        assert!(lists.remove(xu.id).is_err());
    }

    #[test]
    fn a_word_needs_a_letter_or_digit_on_both_sides() {
        let lists = Pronunciations::new(library_with(&["b"]));
        assert!(lists.save(Some("b"), "  ", "something", false).is_err());
        assert!(lists.save(Some("b"), "Xu", " … ", false).is_err());
        assert!(lists.list(Some("b")).unwrap().is_empty());
    }

    #[test]
    fn a_books_rules_come_before_the_ones_for_all_books() {
        let lists = Pronunciations::new(library_with(&["a", "b"]));
        lists.save(None, "Xu", "zoo", false).unwrap();
        lists.save(Some("a"), "Xu", "shoe", true).unwrap();
        lists.save(Some("b"), "Li", "lee", false).unwrap();
        let rules = lists.rules("a").unwrap();
        assert_eq!(
            rules.as_ref(),
            [
                Rule {
                    written: "Xu".into(),
                    spoken: "shoe".into(),
                    match_case: true
                },
                Rule {
                    written: "Xu".into(),
                    spoken: "zoo".into(),
                    match_case: false
                },
            ]
        );
        assert_eq!(aoide_text::pronounce("Xu.", &rules).text, "shoe.");
    }
}
