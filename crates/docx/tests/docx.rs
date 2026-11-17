//! Word files: headings by style, lists, footnotes and the document's properties.
use std::io::Write;

use aoide_document::BlockKind;
use aoide_docx::parse_docx;

const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

fn docx(body: &str, extra: &[(&str, String)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let document =
        format!(r#"<?xml version="1.0"?><w:document {W}><w:body>{body}</w:body></w:document>"#);
    let files = [
        ("[Content_Types].xml", "<Types/>".to_string()),
        ("word/document.xml", document),
    ];
    for (name, text) in files.iter().chain(extra) {
        zip.start_file(*name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn p(style: Option<&str>, runs: &str) -> String {
    let style = style.map_or(String::new(), |s| {
        format!(r#"<w:pPr><w:pStyle w:val="{s}"/></w:pPr>"#)
    });
    format!("<w:p>{style}{runs}</w:p>")
}

fn r(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

#[test]
fn a_word_file_reads_as_chapters_by_its_headings() {
    let body = [
        p(Some("Title"), &r("The Lamp Room")),
        p(Some("Heading1"), &r("Chapter One")),
        p(None, &(r("The keeper ") + &r("climbed") + &r(" the stair."))),
        p(
            None,
            &(r("He trimmed the wick.") + r#"<w:r><w:footnoteReference w:id="2"/></w:r>"#),
        ),
        format!(r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>{}</w:p>"#, r("A short wick")),
        p(Some("Heading2"), &r("The glass")),
        p(None, &(r("Clean") + "<w:r><w:tab/></w:r>" + &r("it."))),
        p(None, ""),
        format!(
            "<w:tbl><w:tr><w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>",
            p(None, &r("Oil")),
            p(None, &r("Two pints"))
        ),
        p(Some("MyChapter"), &r("Chapter Two")),
        p(None, &r("Morning came.")),
    ]
    .concat();
    let styles = format!(
        r#"<w:styles {W}><w:style w:styleId="MyChapter"><w:name w:val="heading 1"/></w:style></w:styles>"#
    );
    let footnotes = format!(
        r#"<w:footnotes {W}><w:footnote w:id="0" w:type="separator"><w:p/></w:footnote><w:footnote w:id="2">{}</w:footnote></w:footnotes>"#,
        p(None, &r("A spiral one."))
    );
    let core = r#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>The Lamp Room</dc:title><dc:creator>Mara Quill</dc:creator><dc:language>en-GB</dc:language></cp:coreProperties>"#;
    let bytes = docx(
        &body,
        &[
            ("word/styles.xml", styles),
            ("word/footnotes.xml", footnotes),
            ("docProps/core.xml", core.into()),
        ],
    );
    let book = parse_docx(&bytes).unwrap();
    assert_eq!(book.title.as_deref(), Some("The Lamp Room"));
    assert_eq!(book.authors, ["Mara Quill"]);
    assert_eq!(book.language.as_deref(), Some("en-GB"));
    let h = |level| BlockKind::Heading { level };
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
    assert_eq!(
        chapters,
        [
            ("The Lamp Room", vec![(h(1), "The Lamp Room")]),
            (
                "Chapter One",
                vec![
                    (h(1), "Chapter One"),
                    (BlockKind::Paragraph, "The keeper climbed the stair."),
                    (BlockKind::Paragraph, "He trimmed the wick."),
                    (BlockKind::ListItem, "A short wick"),
                    (h(2), "The glass"),
                    (BlockKind::Paragraph, "Clean it."),
                    (BlockKind::Paragraph, "Oil, Two pints"),
                ]
            ),
            (
                "Chapter Two",
                vec![
                    (h(1), "Chapter Two"),
                    (BlockKind::Paragraph, "Morning came.")
                ]
            ),
        ]
    );
    assert_eq!(book.notes.len(), 1);
    assert_eq!(book.notes[0].text, "A spiral one.");
    let marker = &book.chapters[1].note_refs[0];
    assert_eq!((marker.block, marker.offset, marker.note), (2, 20, Some(0)));
}

#[test]
fn a_word_file_without_headings_is_one_chapter_and_a_broken_one_is_an_error() {
    let bytes = docx(&p(None, &r("Just a letter.")), &[]);
    let book = parse_docx(&bytes).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].blocks[0].text, "Just a letter.");
    assert!(parse_docx(&docx("", &[])).is_err(), "no text");
    assert!(parse_docx(b"not a zip").is_err());
}
