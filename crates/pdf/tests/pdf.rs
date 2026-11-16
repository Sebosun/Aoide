//! Reflowing a PDF's text into chapters of paragraphs, against tiny PDFs written in the tests.

use aoide_document::BlockKind;
use aoide_pdf::testing::{line, lines_from, pdf, sized, Line};
use aoide_pdf::{parse_pdf, read_pdf_metadata, PdfError};

fn texts(book: &aoide_document::ParsedBook) -> Vec<Vec<String>> {
    book.chapters
        .iter()
        .map(|c| c.blocks.iter().map(|b| b.text.clone()).collect())
        .collect()
}

#[test]
fn lines_become_paragraphs_and_a_word_broken_at_a_line_end_is_mended() {
    let mut page = lines_from(
        700.0,
        &[
            "Every evening the keeper climbed to the light-",
            "house lamp and wound the old clockwork",
            "until the beam turned again.",
        ],
    );
    page.extend(lines_from(
        650.0,
        &[
            "The sea below was quiet, and the boats",
            "came home one by one.",
        ],
    ));
    let book = parse_pdf(&pdf(&[page], &[], &[])).unwrap();
    assert_eq!(
        texts(&book),
        [vec![
            "Every evening the keeper climbed to the lighthouse lamp and wound the old clockwork until the beam turned again.".to_string(),
            "The sea below was quiet, and the boats came home one by one.".into(),
        ]]
    );
    // A run of pages has no heading of its own; a short PDF is one, named for the document.
    assert_eq!(book.chapters[0].title, "Text");
    assert_eq!(book.chapters[0].blocks[0].kind, BlockKind::Paragraph);
    assert_eq!(
        book.chapters[0].text,
        book.chapters[0]
            .blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

/// A page of a book with a running head and a page number.
fn book_page(number: usize, body: &[&str]) -> Vec<Line> {
    let mut page = vec![
        sized(760.0, 9.0, "THE QUAY LIGHTS"),
        sized(40.0, 9.0, &number.to_string()),
    ];
    page.extend(lines_from(700.0, body));
    page
}

#[test]
fn running_heads_and_page_numbers_are_left_out_and_a_sentence_goes_on_over_the_page() {
    let pages = vec![
        book_page(
            1,
            &[
                "The ferry left at dawn.",
                "",
                "Nobody on the quay waved as it",
            ],
        ),
        book_page(
            2,
            &[
                "slipped out past the breakwater.",
                "",
                "By noon the wind had turned.",
            ],
        ),
        book_page(3, &["The keeper counted the gulls."]),
        book_page(4, &["Night came early that day."]),
    ];
    let book = parse_pdf(&pdf(&pages, &[], &[])).unwrap();
    let all: Vec<String> = texts(&book).concat();
    assert!(!all.iter().any(|t| t.contains("QUAY LIGHTS")), "{all:?}");
    assert!(
        !all.iter().any(|t| t.chars().all(|c| c.is_ascii_digit())),
        "{all:?}"
    );
    assert!(
        all.contains(
            &"Nobody on the quay waved as it slipped out past the breakwater.".to_string()
        ),
        "{all:?}"
    );
}

#[test]
fn chapters_follow_the_outline_and_a_larger_line_is_a_heading() {
    let pages = vec![
        vec![
            sized(720.0, 20.0, "Arrival"),
            line(680.0, "The boat reached the island at noon."),
        ],
        vec![line(700.0, "They unloaded the crates of lamp oil.")],
        vec![
            sized(720.0, 20.0, "Departure"),
            line(680.0, "In autumn the keeper rowed back alone."),
        ],
        vec![
            sized(720.0, 16.0, "A letter"),
            line(690.0, "It was waiting for him on the quay."),
        ],
    ];
    let book = parse_pdf(&pdf(&pages, &[("Arrival", 0), ("Departure", 2)], &[])).unwrap();
    assert_eq!(
        texts(&book),
        [
            vec![
                "Arrival".to_string(),
                "The boat reached the island at noon.".into(),
                "They unloaded the crates of lamp oil.".into()
            ],
            vec![
                "Departure".to_string(),
                "In autumn the keeper rowed back alone.".into(),
                "A letter".into(),
                "It was waiting for him on the quay.".into()
            ],
        ]
    );
    let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Arrival", "Departure"]);
    assert!(matches!(
        book.chapters[1].blocks[2].kind,
        BlockKind::Heading { level: 2 }
    ));
}

#[test]
fn without_an_outline_chapters_are_runs_of_ten_pages() {
    let pages: Vec<Vec<Line>> = (1..=23)
        .map(|n| lines_from(700.0, &[&format!("Page {n} tells of the harbour.")]))
        .collect();
    let book = parse_pdf(&pdf(&pages, &[], &[])).unwrap();
    let titles: Vec<&str> = book.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Pages 1–10", "Pages 11–20", "Pages 21–23"]);
}

#[test]
fn title_authors_and_keywords_come_from_the_document_info() {
    let bytes = pdf(
        &[lines_from(700.0, &["A short text."])],
        &[],
        &[
            ("Title", "The Quay Lights"),
            ("Author", "Mira Vale; Ellis Reed"),
            ("Keywords", "Sea, Lighthouses"),
        ],
    );
    let meta = read_pdf_metadata(&bytes).unwrap();
    assert_eq!(meta.title.as_deref(), Some("The Quay Lights"));
    assert_eq!(meta.authors, ["Mira Vale", "Ellis Reed"]);
    assert_eq!(meta.subjects, ["Sea", "Lighthouses"]);
    let book = parse_pdf(&bytes).unwrap();
    assert_eq!(book.title.as_deref(), Some("The Quay Lights"));
    assert_eq!(book.authors, ["Mira Vale", "Ellis Reed"]);
    assert_eq!(book.subjects, ["Sea", "Lighthouses"]);
}

#[test]
fn a_pdf_of_scanned_pages_has_no_text_to_read() {
    let bytes = pdf(&[Vec::new(), Vec::new()], &[], &[("Title", "Scanned")]);
    assert!(matches!(parse_pdf(&bytes), Err(PdfError::NoText)));
    assert!(matches!(
        aoide_pdf::check_text(&bytes),
        Err(PdfError::NoText)
    ));
    assert!(aoide_pdf::check_text(&pdf(&[lines_from(700.0, &["Words."])], &[], &[])).is_ok());
    assert!(matches!(
        parse_pdf(b"not a pdf"),
        Err(PdfError::Unreadable(_))
    ));
}

#[test]
fn entries_set_with_a_hanging_indent_are_paragraphs_of_their_own() {
    let hang = |y: f32, x: f32, text: &str| Line {
        x,
        y,
        size: 11.0,
        text: text.into(),
    };
    let page = vec![
        hang(
            700.0,
            72.0,
            "ad astra: to the stars, a motto of lamplighters and of sailors who",
        ),
        hang(686.0, 84.0, "steer by the night sky (proverb)"),
        hang(672.0, 72.0, "ad fontes: back to the springs"),
        hang(
            658.0,
            72.0,
            "ad lucem: towards the light, as the keepers of the old harbour",
        ),
        hang(644.0, 84.0, "lamp would say each evening"),
        hang(630.0, 72.0, "ad mare: to the sea"),
    ];
    let book = parse_pdf(&pdf(&[page], &[], &[])).unwrap();
    assert_eq!(
        texts(&book)[0],
        [
            "ad astra: to the stars, a motto of lamplighters and of sailors who steer by the night sky (proverb)",
            "ad fontes: back to the springs",
            "ad lucem: towards the light, as the keepers of the old harbour lamp would say each evening",
            "ad mare: to the sea",
        ]
    );
}

#[test]
fn a_heading_over_two_lines_is_one_heading() {
    let page = vec![
        sized(720.0, 20.0, "The Keepers of the"),
        sized(696.0, 20.0, "Northern Lights"),
        line(660.0, "Three keepers served the northern light in turn."),
    ];
    let book = parse_pdf(&pdf(&[page], &[], &[])).unwrap();
    assert_eq!(
        texts(&book)[0],
        [
            "The Keepers of the Northern Lights",
            "Three keepers served the northern light in turn."
        ]
    );
}

#[test]
fn a_file_name_for_a_title_and_a_user_name_for_an_author_are_not_kept() {
    for (title, author) in [
        ("RT9093_FM.fm", "DPIUser16"),
        ("Microsoft Word - quay-lights-final.docx", "Administrator"),
        ("untitled", "user"),
    ] {
        let bytes = pdf(
            &[lines_from(700.0, &["A short text."])],
            &[],
            &[("Title", title), ("Author", author)],
        );
        let meta = read_pdf_metadata(&bytes).unwrap();
        assert_eq!(meta.title, None, "{title}");
        assert!(meta.authors.is_empty(), "{author}");
    }
}
