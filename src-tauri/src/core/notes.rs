//! A book's notes as Markdown: its highlights with their notes and its bookmarks, by chapter in
//! reading order, for Copy as Markdown and for exporting.

use aoide_library::Bookmark;

use super::highlights::HighlightView;

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
}
