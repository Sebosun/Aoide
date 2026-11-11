//! Optional checks against an EPUB of your own. No book is stored in the repository, and nothing
//! from the book is printed unless an assertion fails.
//!
//! ```sh
//! AOIDE_TEST_EPUB=/path/to/book.epub \
//! AOIDE_TEST_EPUB_CHAPTERS=12 \  # optional: the number of chapters you expect
//! cargo test -p aoide-epub --test local_books -- --ignored
//! ```
use aoide_epub::{parse_epub, ParseOptions};

fn local_book() -> Vec<u8> {
    let path = std::env::var("AOIDE_TEST_EPUB").expect("set AOIDE_TEST_EPUB to an EPUB file");
    std::fs::read(path).expect("AOIDE_TEST_EPUB must be a readable file")
}

#[test]
#[ignore = "set AOIDE_TEST_EPUB to an EPUB of your own"]
fn a_local_book_parses_into_chapters_with_text() {
    let book = parse_epub(&local_book(), &ParseOptions::default()).unwrap();
    assert!(!book.chapters.is_empty(), "no chapters found");
    assert!(
        book.chapters.iter().any(|c| !c.text.trim().is_empty()),
        "every chapter is empty"
    );
    assert!(
        book.chapters.iter().all(|c| !c.sources.is_empty()),
        "a chapter has no source document"
    );
    if let Ok(expected) = std::env::var("AOIDE_TEST_EPUB_CHAPTERS") {
        let titles: Vec<_> = book.chapters.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(
            book.chapters.len(),
            expected
                .parse::<usize>()
                .expect("AOIDE_TEST_EPUB_CHAPTERS must be a number"),
            "chapters found: {titles:?}"
        );
    }
}

#[test]
#[ignore = "set AOIDE_TEST_EPUB to an EPUB of your own"]
fn a_local_book_parses_the_same_way_every_time() {
    let bytes = local_book();
    let first = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    let second = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert!(first == second, "two parses of the same file differ");
}

#[test]
#[ignore = "set AOIDE_TEST_EPUB to an EPUB of your own"]
fn keeping_front_matter_only_adds_text() {
    let bytes = local_book();
    let without = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    let with = parse_epub(
        &bytes,
        &ParseOptions {
            exclude_front_matter: false,
            ..ParseOptions::default()
        },
    )
    .unwrap();
    let words = |book: &aoide_epub::ParsedBook| {
        book.chapters
            .iter()
            .map(|c| c.text.split_whitespace().count())
            .sum::<usize>()
    };
    assert!(words(&with) >= words(&without));
}
