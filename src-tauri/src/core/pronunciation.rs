//! The pronunciation lists: how to say a written word, for one book or for all books. The page
//! keeps the book's spelling; the rules go into each request through [`super::voicing::Voicing`].

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use aoide_library::{BookId, BookKey, Library, Pronunciation};
use aoide_text::Rule;
use serde::Serialize;

use super::{lock, now_ms, PlanEvent};
use crate::book::BookModel;

/// What saving a pronunciation would change in a book: the sentences said differently, and how
/// many of them have audio made already (made again when the book is listened to or prepared).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AffectedView {
    pub sentences: usize,
    pub cached: usize,
}

/// A change to a pronunciation list.
pub(super) enum Edit<'a> {
    /// Adds `written` to book `book`'s list (`None`: the list for all books), or changes it.
    Save {
        book: Option<&'a str>,
        written: &'a str,
        spoken: &'a str,
        match_case: bool,
    },
    Remove(i64),
}

fn rules(entries: &[Pronunciation]) -> Vec<Rule> {
    entries
        .iter()
        .map(|p| Rule {
            written: p.written.clone(),
            spoken: p.spoken.clone(),
            match_case: p.match_case,
        })
        .collect()
}

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
        Ok(rules(&entries).into())
    }

    /// The entries of `book` that `edit` would have said differently.
    pub fn changes(&self, book: &BookModel, edit: &Edit) -> Result<Vec<usize>, String> {
        let id = BookId(book.id.clone());
        let entries = lock(&self.library)
            .pronunciations_for(&id)
            .map_err(|e| e.to_string())?;
        let mut after = entries.clone();
        match *edit {
            Edit::Save {
                book: scope,
                written,
                spoken,
                match_case,
            } => {
                if scope.is_some_and(|scope| scope != book.id) {
                    return Ok(Vec::new()); // Another book's list.
                }
                let written = words(written).ok_or("say which word is written")?;
                let spoken = words(spoken).ok_or("say how the word is said")?;
                let scope = scope.map(|_| id);
                match after
                    .iter_mut()
                    .find(|e| e.book_id == scope && e.written == written)
                {
                    Some(entry) => {
                        entry.spoken = spoken;
                        entry.match_case = match_case;
                    }
                    None => {
                        // Last in its list: a book's own list comes before the one for all.
                        let at = match scope {
                            Some(_) => after.iter().position(|e| e.book_id.is_none()),
                            None => None,
                        };
                        after.insert(
                            at.unwrap_or(after.len()),
                            Pronunciation {
                                id: 0,
                                book_id: scope,
                                written,
                                spoken,
                                match_case,
                                created_at: 0,
                            },
                        );
                    }
                }
            }
            Edit::Remove(removed) => after.retain(|e| e.id != removed),
        }
        let (before, after) = (rules(&entries), rules(&after));
        Ok((0..book.total_entries())
            .filter(|&index| {
                book.segment(index).is_some_and(|s| {
                    !s.speech.is_empty()
                        && aoide_text::pronounce(&s.speech, &before).text
                            != aoide_text::pronounce(&s.speech, &after).text
                })
            })
            .collect())
    }

    /// Entry `id`.
    pub fn entry(&self, id: i64) -> Result<Pronunciation, String> {
        lock(&self.library)
            .pronunciation(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no such pronunciation".into())
    }
}

impl super::Core {
    /// Book `book`'s pronunciation list, or with `None` the list for all books.
    pub fn pronunciations(&self, book: Option<&str>) -> Result<Vec<Pronunciation>, String> {
        self.pronunciations.list(book)
    }

    /// What saving `written` said as `spoken` in book `id`'s list would change in the book.
    pub fn pronunciation_affected(
        &self,
        id: &str,
        written: &str,
        spoken: &str,
        match_case: bool,
    ) -> Result<AffectedView, String> {
        let book = self.load_book(id)?;
        let edit = Edit::Save {
            book: Some(id),
            written,
            spoken,
            match_case,
        };
        let changed = self.pronunciations.changes(&book, &edit)?;
        // Without a voice to plan with, nothing can be made yet.
        let cached = self.speech_plan(&book).map_or(0, |plan| {
            changed
                .iter()
                .filter(|i| plan.items.get(**i).is_some_and(|item| item.cached))
                .count()
        });
        Ok(AffectedView {
            sentences: changed.len(),
            cached,
        })
    }

    /// Adds a word to a pronunciation list, or changes how it is said; the sentences it
    /// changes are made again.
    pub async fn save_pronunciation(
        &self,
        book: Option<&str>,
        written: &str,
        spoken: &str,
        match_case: bool,
    ) -> Result<Pronunciation, String> {
        let edit = Edit::Save {
            book,
            written,
            spoken,
            match_case,
        };
        let changed = self.changed_by(book, &edit)?;
        let saved = self
            .pronunciations
            .save(book, written, spoken, match_case)?;
        self.pronunciations_changed(book, changed, "pronunciation.saved")
            .await?;
        Ok(saved)
    }

    /// Removes a word from its pronunciation list; the sentences it changed are made again.
    pub async fn delete_pronunciation(&self, id: i64) -> Result<(), String> {
        let entry = self.pronunciations.entry(id)?;
        let book = entry.book_id.map(|b| b.0);
        let changed = self.changed_by(book.as_deref(), &Edit::Remove(id))?;
        self.pronunciations.remove(id)?;
        self.pronunciations_changed(book.as_deref(), changed, "pronunciation.removed")
            .await
    }

    /// Says `spoken` alone in book `book`'s voice (`None`: the voice books are read in), as a
    /// WAV file, to hear a pronunciation before saving it. Nothing is saved or cached.
    pub async fn preview_pronunciation(
        &self,
        book: Option<&str>,
        spoken: &str,
    ) -> Result<Vec<u8>, String> {
        let spoken = words(spoken).ok_or("say how the word is said")?;
        let profile = self.engine.profile();
        if profile.launch.is_some() {
            self.speech_server.want();
            self.wait_for_speech_server().await?;
        }
        let (voice, params, language) = match book {
            Some(id) => {
                let settings = self.book_settings(id);
                let language = settings
                    .language
                    .clone()
                    .or_else(|| self.load_book(id).ok()?.language.clone());
                (settings.voice_id, settings.params, language)
            }
            None => (None, Default::default(), None),
        };
        self.engine
            .say(profile, voice, params, language, &[(spoken, 0)])
            .await
    }

    /// The books `edit` to book `book`'s list (`None`: the list for all books) says
    /// differently, with how many sentences of each. For all books, those looked at are the
    /// book being listened to and the prepared books; queued books are planned again anyway.
    fn changed_by(&self, book: Option<&str>, edit: &Edit) -> Result<Vec<(String, usize)>, String> {
        let books: Vec<String> = match book {
            Some(book) => vec![book.into()],
            None => {
                let prepared = lock(&self.library)
                    .book_values::<usize>(BookKey::Prepared)
                    .map_err(|e| e.to_string())?;
                let mut books: Vec<String> = prepared.into_keys().map(|id| id.0).collect();
                books.extend(self.playback.book().map(|b| b.id.clone()));
                books.sort();
                books.dedup();
                books
            }
        };
        let mut changed = Vec::new();
        for id in books {
            // An item without text (an audiobook) has nothing to say differently.
            let Ok(model) = self.load_book(&id) else {
                continue;
            };
            let sentences = self.pronunciations.changes(&model, edit)?.len();
            if sentences > 0 {
                changed.push((id, sentences));
            }
        }
        Ok(changed)
    }

    /// A list changed: the book being listened to is planned again if it says something
    /// differently now, and so are the queued books the list applies to; a prepared book it
    /// changes is no longer prepared.
    async fn pronunciations_changed(
        &self,
        book: Option<&str>,
        changed: Vec<(String, usize)>,
        event: &'static str,
    ) -> Result<(), String> {
        let affected: usize = changed.iter().map(|(_, n)| n).sum();
        match (book, event) {
            (Some(book), "pronunciation.saved") => tracing::info!(
                event = "pronunciation.saved",
                book = crate::logging::book(book),
                affected,
                "Pronunciation saved"
            ),
            (Some(book), _) => tracing::info!(
                event = "pronunciation.removed",
                book = crate::logging::book(book),
                affected,
                "Pronunciation removed"
            ),
            (None, "pronunciation.saved") => tracing::info!(
                event = "pronunciation.saved",
                scope = "all",
                affected,
                "Pronunciation saved"
            ),
            (None, _) => tracing::info!(
                event = "pronunciation.removed",
                scope = "all",
                affected,
                "Pronunciation removed"
            ),
        }
        let changed: HashSet<String> = changed.into_iter().map(|(id, _)| id).collect();
        if self
            .playback
            .book()
            .is_some_and(|b| changed.contains(&b.id))
        {
            self.restart_session().await?;
        }
        let any = !changed.is_empty();
        self.plan(PlanEvent::PronunciationsChanged {
            book: book.map(Into::into),
            changed,
        });
        // A prepared book it changes shows as prepared no longer.
        if any {
            self.emit(super::CoreEvent::LibraryChanged);
        }
        Ok(())
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
    fn a_change_says_differently_only_the_sentences_it_reaches() {
        let lists = Pronunciations::new(library_with(&["b"]));
        let book =
            BookModel::parse("b", &super::super::tests::epub(), &Default::default()).unwrap();
        let save = |book, written, spoken| Edit::Save {
            book,
            written,
            spoken,
            match_case: false,
        };
        let lamp = lists
            .changes(&book, &save(None, "lamp", "lantern"))
            .unwrap();
        assert_eq!(lamp.len(), 2, "the chapter title and the sentence");
        assert!(lists
            .changes(&book, &save(Some("other"), "lamp", "lantern"))
            .unwrap()
            .is_empty());
        assert!(lists
            .changes(&book, &save(Some("b"), "zeppelin", "zep"))
            .unwrap()
            .is_empty());

        // The book's own rule wins: a rule for all books under it changes nothing.
        lists.save(Some("b"), "lamp", "light", false).unwrap();
        assert!(lists
            .changes(&book, &save(None, "lamp", "lantern"))
            .unwrap()
            .is_empty());
        let own = lists.list(Some("b")).unwrap()[0].id;
        assert_eq!(lists.changes(&book, &Edit::Remove(own)).unwrap(), lamp);
        // Saved as it is, nothing changes.
        assert!(lists
            .changes(&book, &save(Some("b"), "lamp", "light"))
            .unwrap()
            .is_empty());
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
