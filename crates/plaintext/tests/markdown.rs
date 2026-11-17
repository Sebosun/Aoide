//! Markdown files: headings as chapters, inline markup as text, code and footnotes.
use aoide_document::BlockKind;
use aoide_plaintext::parse_markdown;

const ESSAY: &str = r#"---
title: "On Lamps"
author: Mara Quill
---

# On Lamps

An essay about *lamps* and the [people](https://example.com) who light them.[^1]

## The wick

Trim it **often**, and keep `oil` near.
It burns longer that way.

- A short wick
- A clean glass

```
trim(wick)
  .light()
```

***

> A lamp is a promise.

![A lamp](lamp.png)

## The glass

Clean it with care_fully chosen cloths; 2 * 3 is six.

[^1]: Written at the harbour, 1902.
"#;

#[test]
fn a_markdown_essay_reads_as_chapters_of_plain_text() {
    let book = parse_markdown(ESSAY.as_bytes()).unwrap();
    assert_eq!(book.title.as_deref(), Some("On Lamps"));
    assert_eq!(book.authors, ["Mara Quill"]);
    let chapters: Vec<(&str, Vec<(BlockKind, &str)>)> = book
        .chapters
        .iter()
        .map(|c| {
            (
                c.title.as_str(),
                c.blocks.iter().map(|b| (b.kind, b.text.as_str())).collect(),
            )
        })
        .collect();
    let heading = |level| BlockKind::Heading { level };
    assert_eq!(
        chapters,
        [
            (
                "On Lamps",
                vec![
                    (heading(1), "On Lamps"),
                    (
                        BlockKind::Paragraph,
                        "An essay about lamps and the people who light them."
                    ),
                ]
            ),
            (
                "The wick",
                vec![
                    (heading(2), "The wick"),
                    (
                        BlockKind::Paragraph,
                        "Trim it often, and keep oil near. It burns longer that way."
                    ),
                    (BlockKind::ListItem, "A short wick"),
                    (BlockKind::ListItem, "A clean glass"),
                    (BlockKind::Preformatted, "trim(wick)\n  .light()"),
                    (BlockKind::Paragraph, "A lamp is a promise."),
                ]
            ),
            (
                "The glass",
                vec![
                    (heading(2), "The glass"),
                    (
                        BlockKind::Paragraph,
                        "Clean it with care_fully chosen cloths; 2 * 3 is six."
                    ),
                ]
            ),
        ]
    );
    // The rule marks a scene break before the quote.
    assert!(book.chapters[1].blocks[5].break_before);
    // The footnote is a note, its marker where it stood.
    assert_eq!(book.notes.len(), 1);
    assert_eq!(book.notes[0].text, "Written at the harbour, 1902.");
    let marker = &book.chapters[0].note_refs[0];
    assert_eq!(
        (
            marker.block,
            marker.offset,
            marker.label.as_str(),
            marker.note
        ),
        (1, 51, "1", Some(0))
    );
    assert!(book.warnings.is_empty(), "{:?}", book.warnings);
}

#[test]
fn several_top_headings_are_the_chapters_and_none_makes_one() {
    let book =
        parse_markdown(b"# One\n\nFirst.\n\n## Inside\n\nStill one.\n\n# Two\n\nSecond.").unwrap();
    let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["One", "Two"]);
    let book = parse_markdown(b"Just a note to self.").unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].blocks[0].text, "Just a note to self.");
}
