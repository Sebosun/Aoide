//! Pictures, tables and code: what they add to a chapter, and that its text stays as it was.
mod support;

use aoide_epub::{
    parse_epub, read_resource, BlockKind, Caption, Insert, InsertKind, ParseOptions, TableShape,
};

/// A chapter with a picture, a figure with its caption, a table with a caption, and code.
pub const ILLUSTRATED: &str = r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<h1>The Lighthouse</h1>
<p>The keeper climbed the stair.</p>
<img src="images/stair.png" alt="A spiral stair"/>
<p>At the top the lamp waited.</p>
<figure><img src="../OPS/images/lamp.png" alt="The great lamp"/><figcaption>The lamp, lit at dusk.</figcaption></figure>
<p>He counted the years.</p>
<table><caption>Lamps lit each year</caption>
<tr><th>Year</th><th>Lamps</th></tr>
<tr><td>1901</td><td>12</td></tr>
<tr><td>1902</td><td>14 and a half</td></tr>
</table>
<pre>fn light() {
    lamp.on();
}</pre>
<p>Then he slept.</p>
</body></html>"#;

pub fn illustrated() -> Vec<u8> {
    support::single(ILLUSTRATED)
}

/// What the parser made of the chapter before pictures, tables and code were read: the guard
/// that the display text (and so every stored place) stays the same.
#[test]
fn the_display_text_is_what_it_was() {
    let book = parse_epub(&illustrated(), &ParseOptions::default()).unwrap();
    let chapter = &book.chapters[0];
    let texts: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        texts,
        [
            "The Lighthouse",
            "The keeper climbed the stair.",
            "At the top the lamp waited.",
            "The lamp, lit at dusk.",
            "He counted the years.",
            "Lamps lit each year",
            "Year Lamps",
            "1901 12",
            "1902 14 and a half",
            "fn light() { lamp.on(); }",
            "Then he slept.",
        ]
    );
    assert_eq!(chapter.blocks[9].kind, BlockKind::Preformatted);
    assert_eq!(
        chapter.text,
        texts.join("\n\n"),
        "the text is the blocks joined"
    );
}

fn picture(resource: &str, alt: &str, caption: Option<Caption>) -> InsertKind {
    InsertKind::Picture {
        resource: resource.into(),
        alt: alt.into(),
        caption,
    }
}

fn inserts(body: &str) -> Vec<Insert> {
    let book = parse_epub(&support::single(body), &ParseOptions::default()).unwrap();
    book.chapters.into_iter().next().unwrap().inserts
}

#[test]
fn pictures_stand_between_blocks_with_the_paragraph_that_captions_them() {
    let book = parse_epub(&illustrated(), &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters[0].inserts,
        [
            Insert {
                before_block: 2,
                kind: picture("OPS/images/stair.png", "A spiral stair", None),
            },
            Insert {
                before_block: 3,
                kind: picture(
                    "OPS/images/lamp.png",
                    "The great lamp",
                    Some(Caption::Block(3))
                ),
            },
        ]
    );
}

#[test]
fn a_picture_first_or_last_or_inside_a_line() {
    let found = inserts(
        r#"<img src="a.png" alt=""/><p>One <img src="b.png" alt="b"/> two.</p><p>Three.</p><svg><image xlink:href="c.jpg"/></svg>"#,
    );
    let at: Vec<(usize, &str)> = found
        .iter()
        .map(|i| match &i.kind {
            InsertKind::Picture { resource, .. } => (i.before_block, resource.as_str()),
            other => panic!("{other:?}"),
        })
        .collect();
    // Inside a line: after it, so the line is not cut.
    assert_eq!(at, [(0, "OPS/a.png"), (1, "OPS/b.png"), (2, "OPS/c.jpg")]);
}

#[test]
fn the_cover_decoration_and_pictures_from_elsewhere_are_left_out() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="cover" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/chapter.xhtml",
                r#"<img src="cover.jpg"/><p>Text.</p><img src="rule.png" role="presentation"/><img src="https://example.com/x.png"/><img src="data:image/png;base64,AAAA"/><img src="kept.png"/>"#,
            ),
            ("OPS/cover.jpg", "jpeg"),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters[0].inserts,
        [Insert {
            before_block: 1,
            kind: picture("OPS/kept.png", "", None)
        }]
    );
}

#[test]
fn a_resource_is_read_from_the_book() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            ("OPS/chapter.xhtml", "<p>Text.</p>"),
            ("OPS/images/a.png", "png bytes"),
        ],
    );
    let options = ParseOptions::default();
    assert_eq!(
        read_resource(&bytes, "OPS/images/a.png", &options).unwrap(),
        b"png bytes"
    );
    assert!(read_resource(&bytes, "OPS/images/missing.png", &options).is_err());
}

#[test]
fn a_table_keeps_its_rows_as_text_and_records_where_each_cell_is() {
    let book = parse_epub(&illustrated(), &ParseOptions::default()).unwrap();
    let chapter = &book.chapters[0];
    assert_eq!(
        chapter.tables,
        [TableShape {
            rows: 6..9,
            caption_block: Some(5),
            header_rows: 1,
            cells: vec![vec![0..4, 5..10], vec![0..4, 5..7], vec![0..4, 5..18]],
        }]
    );
    let cell = |row: usize, n: usize| -> String {
        let range = chapter.tables[0].cells[row][n].clone();
        chapter.blocks[6 + row]
            .text
            .chars()
            .skip(range.start)
            .take(range.len())
            .collect()
    };
    assert_eq!(
        (cell(0, 1), cell(2, 1)),
        ("Lamps".into(), "14 and a half".into())
    );
}

#[test]
fn a_table_that_is_not_a_grid_of_lines_stays_text_only() {
    // A paragraph in a cell makes a row of several lines: no shape is recorded.
    let book = parse_epub(
        &support::single(
            "<table><tr><td><p>One</p><p>Two</p></td><td>3</td></tr></table><table><tr><td></td><td>4</td></tr></table>",
        ),
        &ParseOptions::default(),
    )
    .unwrap();
    let chapter = &book.chapters[0];
    let texts: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, ["One", "Two", "3", "4"]);
    assert_eq!(
        chapter.tables,
        [TableShape {
            rows: 3..4,
            caption_block: None,
            header_rows: 0,
            cells: vec![vec![0..0, 0..1]],
        }],
        "an empty cell is an empty range"
    );
}
