//! A book's notes as Markdown: its highlights with their notes and its bookmarks, by chapter in
//! reading order, for Copy as Markdown and for exporting.

use std::collections::HashSet;
use std::path::Path;

use aoide_library::{BookId, Bookmark, ItemKind};

use super::highlights::HighlightView;
use super::{lock, Core};

/// What goes into a book's notes.
pub(super) struct Notes<'a> {
    pub title: &'a str,
    pub authors: &'a [String],
    /// Chapter titles by index (an audiobook's parts); an empty one is "Chapter n".
    pub chapters: &'a [String],
    pub highlights: &'a [HighlightView],
    pub bookmarks: &'a [Bookmark],
    /// The day of the export, `YYYY-MM-DD`.
    pub date: &'a str,
}

/// Backslashes the characters Markdown would read as markup in text from a book, and a line
/// start that would begin a heading, a list or a quotation.
fn escape(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    let digits = out.chars().take_while(char::is_ascii_digit).count();
    let list = out.starts_with(['-', '+']) || (digits > 0 && out[digits..].starts_with(['.', ')']));
    if list {
        let at = if digits > 0 { digits } else { 0 };
        out.insert(at, '\\');
    }
    out
}

/// `ms` as `h:mm:ss`, or `m:ss` under an hour.
fn clock(ms: u64) -> String {
    let s = ms / 1000;
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

enum Entry<'a> {
    Bookmark(&'a Bookmark),
    Highlight(&'a HighlightView),
}

/// The notes as one Markdown document: a heading for the book and one for each chapter that
/// has notes; a highlight as a quotation followed by its note, bookmarks as a list.
pub(super) fn markdown(notes: &Notes) -> String {
    let mut out = format!("# {}\n", escape(notes.title));
    let authors = notes.authors.join(", ");
    if authors.is_empty() {
        out.push_str(&format!("Exported {} from Aoide\n", notes.date));
    } else {
        out.push_str(&format!(
            "*{}* · exported {} from Aoide\n",
            escape(&authors),
            notes.date
        ));
    }
    let mut entries: Vec<(usize, usize, u8, Entry)> = notes
        .bookmarks
        .iter()
        .map(|b| {
            (
                b.chapter as usize,
                b.char_offset as usize,
                0,
                Entry::Bookmark(b),
            )
        })
        .chain(
            notes
                .highlights
                .iter()
                .map(|h| (h.chapter, h.start, 1, Entry::Highlight(h))),
        )
        .collect();
    entries.sort_by_key(|(chapter, at, kind, _)| (*chapter, *at, *kind));
    let mut chapter = None;
    let mut in_list = false;
    for (index, _, _, entry) in &entries {
        if chapter != Some(*index) {
            chapter = Some(*index);
            let title = notes
                .chapters
                .get(*index)
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .map_or_else(|| format!("Chapter {}", index + 1), escape);
            out.push_str(&format!("\n## {title}\n"));
            in_list = false;
        }
        match entry {
            Entry::Bookmark(bookmark) => {
                if !in_list {
                    out.push('\n');
                }
                in_list = true;
                let place = match bookmark.position_ms {
                    Some(ms) => clock(ms),
                    None => escape(&bookmark.excerpt),
                };
                let note = bookmark.note.trim();
                if note.is_empty() {
                    out.push_str(&format!("- 🔖 {place}\n"));
                } else {
                    out.push_str(&format!("- 🔖 {place} · {}\n", note.replace('\n', " ")));
                }
            }
            Entry::Highlight(highlight) => {
                in_list = false;
                out.push('\n');
                for line in highlight.text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        out.push_str(">\n");
                    } else {
                        out.push_str(&format!("> {}\n", escape(line)));
                    }
                }
                let note = highlight.note.trim();
                if !note.is_empty() {
                    out.push_str(&format!("\n{note}\n"));
                }
            }
        }
    }
    out
}

/// Book `id`'s notes and what is in them: (Markdown, highlights, bookmarks).
fn notes_of(core: &Core, id: &str) -> Result<(String, usize, usize), String> {
    let record = lock(&core.library)
        .book(&BookId(id.into()))
        .map_err(|e| e.to_string())?
        .ok_or("that book is not in the library")?;
    let bookmarks = core.bookmarks(id)?;
    let (chapters, highlights) = if record.kind == ItemKind::Audiobook {
        // A time bookmark's excerpt is the title of its part.
        let mut parts = Vec::new();
        for bookmark in &bookmarks {
            let part = bookmark.chapter as usize;
            if parts.len() <= part {
                parts.resize(part + 1, String::new());
            }
            parts[part].clone_from(&bookmark.excerpt);
        }
        (parts, Vec::new())
    } else {
        match core.load_book(id) {
            Ok(book) => {
                let titles = book.chapters.iter().map(|c| c.title.clone()).collect();
                (titles, core.highlights.list(&book)?)
            }
            // Its file is gone or cannot be read: its notes are still the user's, as stored.
            Err(_) => (Vec::new(), core.highlights.unanchored(id)?),
        }
    };
    let title = record.title.unwrap_or_else(|| "Untitled".into());
    let text = markdown(&Notes {
        title: &title,
        authors: &record.authors,
        chapters: &chapters,
        highlights: &highlights,
        bookmarks: &bookmarks,
        date: &chrono::Local::now().format("%Y-%m-%d").to_string(),
    });
    Ok((text, highlights.len(), bookmarks.len()))
}

/// Writes book `id`'s notes to `path`.
fn write_notes(core: &Core, id: &str, path: &Path) -> Result<(), String> {
    let (text, highlights, bookmarks) = notes_of(core, id)?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    tracing::info!(
        event = "notes.exported",
        book = crate::logging::book(id),
        highlights,
        bookmarks,
        file = %crate::logging::file(path),
        "Exported a book's notes"
    );
    Ok(())
}

impl Core {
    /// Book `id`'s highlights, their notes and its bookmarks as Markdown (Copy as Markdown).
    pub fn notes_markdown(&self, id: &str) -> Result<String, String> {
        Ok(notes_of(self, id)?.0)
    }

    /// Writes book `id`'s notes as Markdown to the file `path`.
    pub fn export_notes(&self, id: &str, path: &str) -> Result<(), String> {
        write_notes(self, id, Path::new(path))
    }

    /// Writes the notes of every book in the library that has highlights or bookmarks into
    /// `folder`, one file per book named after its title; returns how many files.
    pub fn export_all_notes(&self, folder: &str) -> Result<u32, String> {
        let folder = Path::new(folder);
        let books = lock(&self.library).books().map_err(|e| e.to_string())?;
        let mut taken = HashSet::new();
        let mut written = 0;
        for book in books {
            let has_notes = {
                let library = lock(&self.library);
                !library
                    .highlights(&book.id)
                    .map_err(|e| e.to_string())?
                    .is_empty()
                    || !library
                        .bookmarks(&book.id)
                        .map_err(|e| e.to_string())?
                        .is_empty()
            };
            if !has_notes {
                continue;
            }
            if written == 0 {
                std::fs::create_dir_all(folder)
                    .map_err(|e| format!("{}: {e}", folder.display()))?;
            }
            let name = super::export::file_name(book.title.as_deref().unwrap_or("Notes"));
            let stem = name.strip_suffix(".m4b").unwrap_or(&name).to_owned();
            let stem = (1..)
                .map(|n| {
                    if n == 1 {
                        stem.clone()
                    } else {
                        format!("{stem} ({n})")
                    }
                })
                .find(|s| taken.insert(s.to_lowercase()))
                .expect("unbounded");
            write_notes(self, &book.id.0, &folder.join(format!("{stem}.md")))?;
            written += 1;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::highlights::HighlightView;
    use aoide_library::{BookId, Bookmark};

    fn highlight(chapter: usize, start: usize, text: &str, note: &str) -> HighlightView {
        HighlightView {
            id: start as i64,
            chapter,
            chapter_title: String::new(),
            start,
            end: start + text.chars().count(),
            text: text.into(),
            note: note.into(),
            detached: false,
        }
    }

    fn bookmark(chapter: u32, char_offset: u32, excerpt: &str, note: &str) -> Bookmark {
        Bookmark {
            id: char_offset.into(),
            book_id: BookId("b".into()),
            chapter,
            char_offset,
            segment_id: None,
            excerpt: excerpt.into(),
            note: note.into(),
            created_at: 0,
            position_ms: None,
        }
    }

    #[test]
    fn a_books_notes_are_markdown_by_chapter_in_reading_order() {
        let notes = Notes {
            title: "Quay Lights",
            authors: &["Test Author".into(), "A. N. Other".into()],
            chapters: &["The Quay".into(), String::new()],
            highlights: &[
                highlight(
                    0,
                    10,
                    "Gulls argued over the nets.",
                    "Noisy, as ever.\nTwo lines of it.",
                ),
                highlight(0, 38, "The tide came in *slowly*.\n\nBoats [and] all.", ""),
                highlight(1, 10, "Someone lit the lamp at dusk.", ""),
            ],
            bookmarks: &[
                bookmark(0, 10, "Gulls argued over the nets.", ""),
                bookmark(0, 40, "The tide came in slowly.", " Turn back here "),
                bookmark(0, 60, "Boats and all.", ""),
            ],
            date: "2026-11-16",
        };
        assert_eq!(markdown(&notes), include_str!("testdata/notes.md"));
    }

    #[tokio::test]
    async fn a_books_notes_are_exported_to_a_file_and_every_books_to_a_folder() {
        use crate::core::tests::{library_folder, start_with, until};
        use std::sync::Arc;
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
        // Nothing to export yet: no file is written.
        assert_eq!(
            core.export_all_notes(&dir.path().join("all").to_string_lossy())
                .unwrap(),
            0
        );
        let text = core.load_book(&id).unwrap().chapters[1].text.clone();
        let start = text.find("lit the lamp").unwrap();
        core.add_highlight(&id, 1, start, start + 12).unwrap();
        core.add_bookmark_at(&id, "here", None).unwrap();

        let markdown = core.notes_markdown(&id).unwrap();
        assert!(
            markdown.starts_with("# Quay Lights\n*Test Author* · exported "),
            "{markdown}"
        );
        assert!(
            markdown.contains("## The Lamp\n\n> lit the lamp\n"),
            "{markdown}"
        );
        assert!(markdown.contains("- 🔖 "), "{markdown}");

        let file = dir.path().join("notes.md");
        core.export_notes(&id, &file.to_string_lossy()).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), markdown);

        let folder = dir.path().join("all");
        assert_eq!(core.export_all_notes(&folder.to_string_lossy()).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(folder.join("Quay Lights.md")).unwrap(),
            markdown
        );
    }

    #[tokio::test]
    async fn the_notes_of_a_book_whose_file_is_gone_are_still_exported() {
        use crate::core::tests::{library_folder, start_with, until};
        use std::sync::Arc;
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
        // Highlighted in an earlier session (the book is not loaded in this one).
        lock(&core.library)
            .add_highlight(
                &BookId(id.clone()),
                &aoide_library::NewHighlight {
                    chapter: 1,
                    start: 4,
                    end: 16,
                    text: "lit the lamp".into(),
                    note: "bright".into(),
                },
                &[],
                1,
            )
            .unwrap();
        std::fs::remove_file(dir.path().join("Books/quay-lights.epub")).unwrap();
        core.rescan().unwrap();

        let markdown = core.notes_markdown(&id).unwrap();
        assert!(
            markdown.contains("## Chapter 2\n\n> lit the lamp\n\nbright\n"),
            "{markdown}"
        );
        let folder = dir.path().join("all");
        assert_eq!(core.export_all_notes(&folder.to_string_lossy()).unwrap(), 1);
        assert_eq!(
            std::fs::read_to_string(folder.join("Quay Lights.md")).unwrap(),
            markdown
        );
    }
}
