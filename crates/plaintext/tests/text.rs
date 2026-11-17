//! Plain text files: chapters, paragraphs, encodings, and Project Gutenberg's wrapping.
use aoide_document::{BlockKind, ExclusionReason, Warning};
use aoide_plaintext::parse_text;

fn texts(book: &aoide_document::ParsedBook) -> Vec<(String, Vec<String>)> {
    book.chapters
        .iter()
        .map(|c| {
            (
                c.title.clone(),
                c.blocks.iter().map(|b| b.text.clone()).collect(),
            )
        })
        .collect()
}

const GUTENBERG: &str = "The Project Gutenberg eBook of The Lamp Room\r
\r
Title: The Lamp Room\r
\r
Author: Mara Quill\r
\r
Language: English\r
\r
*** START OF THE PROJECT GUTENBERG EBOOK THE LAMP ROOM ***\r
\r
THE LAMP ROOM\r
\r
CHAPTER I.\r
\r
The keeper climbed the stair as the light\r
went out of the sky, and the gulls settled.\r
\r
He trimmed the wick.\r
\r
CHAPTER II.\r
\r
Morning came grey over the water.\r
\r
*** END OF THE PROJECT GUTENBERG EBOOK THE LAMP ROOM ***\r
\r
Updated editions will replace the previous one.\r
";

#[test]
fn a_gutenberg_text_reads_as_chapters_without_its_header_and_licence() {
    let book = parse_text(GUTENBERG.as_bytes()).unwrap();
    assert_eq!(book.title.as_deref(), Some("The Lamp Room"));
    assert_eq!(book.authors, ["Mara Quill"]);
    assert_eq!(book.language.as_deref(), Some("en"));
    assert_eq!(
        texts(&book),
        [
            ("THE LAMP ROOM".to_string(), vec!["THE LAMP ROOM".to_string()]),
            (
                "CHAPTER I.".into(),
                vec![
                    "CHAPTER I.".into(),
                    "The keeper climbed the stair as the light went out of the sky, and the gulls settled.".into(),
                    "He trimmed the wick.".into()
                ]
            ),
            (
                "CHAPTER II.".into(),
                vec!["CHAPTER II.".into(), "Morning came grey over the water.".into()]
            ),
        ]
    );
    assert_eq!(
        book.chapters[1].blocks[0].kind,
        BlockKind::Heading { level: 1 }
    );
    // Not silently dropped: reported as left out.
    let excluded: Vec<ExclusionReason> = book.excluded.iter().map(|e| e.reason).collect();
    assert_eq!(
        excluded,
        [ExclusionReason::TitlePage, ExclusionReason::Copyright]
    );
    assert!(book.excluded[1].chapter.text.contains("Updated editions"));
    for chapter in &book.chapters {
        let joined: Vec<&str> = chapter.blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(chapter.text, joined.join("\n\n"));
    }
}

#[test]
fn lines_without_blank_lines_are_rejoined_where_a_sentence_runs_on() {
    let book = parse_text(
        b"One line ends here.\nAnother starts and\ncontinues on the next line.\nThe end.",
    )
    .unwrap();
    assert_eq!(
        texts(&book)[0].1,
        [
            "One line ends here.",
            "Another starts and continues on the next line.",
            "The end."
        ]
    );
}

#[test]
fn a_text_without_headings_is_cut_into_parts_and_its_encoding_is_guessed() {
    let paragraph = "word ".repeat(1000);
    let long = [paragraph.trim(); 12].join("\n\n");
    let book = parse_text(long.as_bytes()).unwrap();
    let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Part 1", "Part 2", "Part 3"]);
    assert_eq!(book.chapters[0].blocks.len(), 5);

    // Windows-1252: "café" with é as 0xE9.
    let book = parse_text(b"A caf\xe9 by the quay.").unwrap();
    assert_eq!(book.chapters[0].blocks[0].text, "A café by the quay.");
    assert_eq!(
        book.warnings,
        [Warning::GuessedEncoding {
            encoding: "windows-1252".into()
        }]
    );
    // UTF-16 with its mark.
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain("Tide.".encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert_eq!(
        parse_text(&utf16).unwrap().chapters[0].blocks[0].text,
        "Tide."
    );
    assert!(parse_text(b"  \n\n ").is_err(), "no text");
}
