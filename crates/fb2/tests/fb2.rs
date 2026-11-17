//! FictionBook files: chapters from sections, notes, the cover and the series.
use std::io::Write;

use aoide_document::BlockKind;
use aoide_fb2::parse_fb2;

const BOOK: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info>
  <genre>sf</genre>
  <author><first-name>Mara</first-name><last-name>Quill</last-name></author>
  <book-title>The Lamp Room</book-title>
  <annotation><p>A keeper and his light.</p></annotation>
  <coverpage><image l:href="#cover.png"/></coverpage>
  <lang>en</lang>
  <sequence name="Harbour Lights" number="2"/>
</title-info></description>
<body>
  <title><p>The Lamp Room</p></title>
  <section>
    <title><p>Chapter One</p></title>
    <epigraph><p>Light is a promise.</p></epigraph>
    <p>The keeper climbed the <emphasis>stair</emphasis>.<a l:href="#n1" type="note">1</a></p>
    <empty-line/>
    <p>Night fell.</p>
    <section>
      <title><p>The Wick</p></title>
      <p>He trimmed it.</p>
    </section>
  </section>
  <section>
    <title><p>Chapter Two</p></title>
    <poem><stanza><v>Over the water,</v><v>the lamp.</v></stanza></poem>
  </section>
</body>
<body name="notes">
  <section id="n1"><title><p>1</p></title><p>A spiral one.</p></section>
</body>
<binary id="cover.png" content-type="image/png">iVBORw0KGgo=</binary>
</FictionBook>"##;

fn chapters(book: &aoide_document::ParsedBook) -> Vec<(String, Vec<(BlockKind, String)>)> {
    book.chapters
        .iter()
        .map(|c| {
            (
                c.title.clone(),
                c.blocks.iter().map(|b| (b.kind, b.text.clone())).collect(),
            )
        })
        .collect()
}

#[test]
fn a_fictionbook_reads_as_chapters_with_its_notes_cover_and_series() {
    let book = parse_fb2(BOOK.as_bytes()).unwrap();
    assert_eq!(book.title.as_deref(), Some("The Lamp Room"));
    assert_eq!(book.authors, ["Mara Quill"]);
    assert_eq!(book.language.as_deref(), Some("en"));
    assert_eq!(book.description.as_deref(), Some("A keeper and his light."));
    let series = book.series.as_ref().unwrap();
    assert_eq!(
        (series.name.as_str(), series.position),
        ("Harbour Lights", Some(2.0))
    );
    let cover = book.cover.as_ref().unwrap();
    assert_eq!(cover.media_type, "image/png");
    assert_eq!(cover.bytes, b"\x89PNG\r\n\x1a\n");
    let h = |level| BlockKind::Heading { level };
    let p = BlockKind::Paragraph;
    assert_eq!(
        chapters(&book),
        [
            (
                "Chapter One".to_string(),
                vec![
                    (h(1), "Chapter One".to_string()),
                    (p, "Light is a promise.".into()),
                    (p, "The keeper climbed the stair.".into()),
                    (p, "Night fell.".into()),
                    (h(2), "The Wick".into()),
                    (p, "He trimmed it.".into()),
                ]
            ),
            (
                "Chapter Two".to_string(),
                vec![
                    (h(1), "Chapter Two".to_string()),
                    (p, "Over the water,".into()),
                    (p, "the lamp.".into()),
                ]
            ),
        ]
    );
    // The empty line is a scene break.
    assert!(book.chapters[0].blocks[3].break_before);
    assert_eq!(book.notes.len(), 1);
    assert_eq!(book.notes[0].text, "A spiral one.");
    let marker = &book.chapters[0].note_refs[0];
    assert_eq!(
        (
            marker.block,
            marker.offset,
            marker.label.as_str(),
            marker.note
        ),
        (2, 29, "1", Some(0))
    );
}

#[test]
fn a_zipped_fictionbook_is_read_and_a_broken_one_is_an_error() {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("lamp.fb2", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(BOOK.as_bytes()).unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert_eq!(parse_fb2(&bytes).unwrap().chapters.len(), 2);
    assert!(parse_fb2(b"<FictionBook><body><section>").is_err());
    assert!(parse_fb2(
        b"<FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\"><body/></FictionBook>"
    )
    .is_err());
}
