//! Plain text and Markdown files as Aoide's parsed-book model, pure and deterministic.
//!
//! Text: paragraphs are blank-line separated (hard-wrapped lines joined), or one a line when
//! the file has no blank lines (a line that runs on into a lowercase one joined to it).
//! Chapters start at lines like "Chapter 4", "PART II" or a roman numeral alone, or at a form
//! feed; a text without any is cut into parts of about 5,000 words. Project Gutenberg's header
//! and licence are reported as excluded sections.

mod markdown;

use aoide_document::{
    Block, BlockKind, Chapter, ExcludedSection, ExclusionReason, ParsedBook, Warning,
};

pub use markdown::parse_markdown;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("the file has no text")]
    Empty,
}

/// Words in a part, when a text has no chapter headings.
const PART_WORDS: usize = 5_000;

/// Where a text's chapters come from, in its sources.
const PATH: &str = "text";

/// A text file's bytes as text: UTF-8 (with or without its mark), UTF-16 with its mark, else
/// Windows-1252 with a warning. Line ends become `\n`.
pub(crate) fn decode(bytes: &[u8]) -> (String, Option<Warning>) {
    let (text, warning) = if let Some((encoding, _)) = encoding_rs::Encoding::for_bom(bytes) {
        (encoding.decode_with_bom_removal(bytes).0.into_owned(), None)
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        (text.to_owned(), None)
    } else {
        let text = encoding_rs::WINDOWS_1252
            .decode_without_bom_handling(bytes)
            .0;
        let warning = Warning::GuessedEncoding {
            encoding: "windows-1252".into(),
        };
        (text.into_owned(), Some(warning))
    };
    (text.replace("\r\n", "\n").replace('\r', "\n"), warning)
}

/// A plain text file as a book.
pub fn parse_text(bytes: &[u8]) -> Result<ParsedBook, Error> {
    let (text, warning) = decode(bytes);
    let gutenberg = gutenberg(&text);
    let body = gutenberg.as_ref().map_or(text.as_str(), |g| g.body);
    let mut chapters = Vec::new();
    for page in body.split('\x0c') {
        chapters.extend(chapters_of(&paragraphs(page)));
    }
    if chapters.is_empty() {
        return Err(Error::Empty);
    }
    let mut book = ParsedBook::of(chapters);
    book.warnings.extend(warning);
    if let Some(g) = gutenberg {
        book.title = g.field("Title");
        book.authors = g.field("Author").into_iter().collect();
        book.language = g.field("Language").map(|l| language_code(&l));
        for (text, reason) in [
            (g.header, ExclusionReason::TitlePage),
            (g.licence, ExclusionReason::Copyright),
        ] {
            let blocks: Vec<Block> = paragraphs(text)
                .into_iter()
                .map(|p| Block::of(BlockKind::Paragraph, p))
                .collect();
            if !blocks.is_empty() {
                let title = match reason {
                    ExclusionReason::TitlePage => "Project Gutenberg's header",
                    _ => "Project Gutenberg's licence",
                };
                book.excluded.push(ExcludedSection {
                    chapter: Chapter::of(title.into(), blocks, PATH),
                    reason,
                });
            }
        }
    }
    Ok(book)
}

/// A Project Gutenberg text's parts.
struct Gutenberg<'a> {
    header: &'a str,
    body: &'a str,
    licence: &'a str,
}

impl Gutenberg<'_> {
    /// `Name: value` from the header.
    fn field(&self, name: &str) -> Option<String> {
        self.header.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == name)
                .then(|| value.trim().to_owned())
                .filter(|v| !v.is_empty())
        })
    }
}

fn gutenberg(text: &str) -> Option<Gutenberg<'_>> {
    let marker = |line: &str, word: &str| {
        let line = line.trim().to_ascii_uppercase();
        line.starts_with("***") && line.contains(word) && line.contains("PROJECT GUTENBERG")
    };
    let mut at = 0;
    let (mut start, mut end) = (None, None);
    for line in text.split_inclusive('\n') {
        if start.is_none() && marker(line, "START OF") {
            start = Some((at, at + line.len()));
        } else if start.is_some() && end.is_none() && marker(line, "END OF") {
            end = Some((at, at + line.len()));
        }
        at += line.len();
    }
    let (start, end) = (start?, end.unwrap_or((text.len(), text.len())));
    Some(Gutenberg {
        header: &text[..start.0],
        body: &text[start.1..end.0],
        licence: &text[end.1..],
    })
}

/// "English" as "en", for the few languages Project Gutenberg names most.
fn language_code(name: &str) -> String {
    let codes = [
        ("english", "en"),
        ("french", "fr"),
        ("german", "de"),
        ("spanish", "es"),
        ("italian", "it"),
        ("dutch", "nl"),
        ("portuguese", "pt"),
        ("finnish", "fi"),
        ("swedish", "sv"),
        ("russian", "ru"),
        ("polish", "pl"),
        ("latin", "la"),
    ];
    let lower = name.to_ascii_lowercase();
    codes
        .iter()
        .find(|(n, _)| lower.starts_with(n))
        .map_or(name.to_owned(), |(_, code)| (*code).to_owned())
}

/// The text's paragraphs, each on one line with its whitespace collapsed.
fn paragraphs(text: &str) -> Vec<String> {
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Paragraphs apart by blank lines, or one a line.
    let blank_lines = text.trim().split('\n').any(|line| line.trim().is_empty());
    if blank_lines {
        return text
            .split('\n')
            .collect::<Vec<_>>()
            .split(|line| line.trim().is_empty())
            .map(|lines| collapse(&lines.join(" ")))
            .filter(|p| !p.is_empty())
            .collect();
    }
    let mut out: Vec<String> = Vec::new();
    let mut runs_on = false;
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let lower_start = line.chars().next().is_some_and(char::is_lowercase);
        match out.last_mut() {
            Some(last) if runs_on && lower_start => {
                last.push(' ');
                last.push_str(line);
            }
            _ => out.push(line.to_owned()),
        }
        runs_on = !line.ends_with([
            '.', '!', '?', ':', ';', '"', '\u{201d}', '\'', '\u{2019}', ')',
        ]);
    }
    out.into_iter().map(|p| collapse(&p)).collect()
}

/// Whether a paragraph is a chapter's heading.
fn is_heading(paragraph: &str) -> bool {
    if paragraph.chars().count() > 80 {
        return false;
    }
    let words: Vec<&str> = paragraph.split_whitespace().collect();
    let Some(first) = words.first() else {
        return false;
    };
    let first = first.trim_end_matches(['.', ':']).to_lowercase();
    let numbered = |word: Option<&&str>| {
        word.is_some_and(|w| {
            let w = w.trim_end_matches(['.', ':']).to_lowercase();
            w.chars().all(|c| c.is_ascii_digit())
                || is_roman(&w)
                || NUMBER_WORDS.contains(&w.as_str())
        })
    };
    match first.as_str() {
        "chapter" | "part" | "book" | "volume" => words.len() == 1 || numbered(words.get(1)),
        "prologue" | "epilogue" | "preface" | "introduction" | "afterword" => words.len() <= 6,
        _ => {
            words.len() == 1
                && (is_roman(&first)
                    || (first.len() <= 3 && first.chars().all(|c| c.is_ascii_digit())))
        }
    }
}

const NUMBER_WORDS: &[&str] = &[
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "last",
];

fn is_roman(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= 8
        && word.chars().all(|c| "ivxlcdm".contains(c))
        // "I", "mix", "did" and "civil" are words, not numerals.
        && !["mix", "did", "dim", "mild", "mid", "civil", "lid", "vid", "dill", "ill", "mill"]
            .contains(&word)
}

/// Paragraphs as chapters: one at each heading (the text before the first is a chapter of
/// its own, named by its first line); without headings, parts of about [`PART_WORDS`].
fn chapters_of(paragraphs: &[String]) -> Vec<Chapter> {
    if paragraphs.is_empty() {
        return Vec::new();
    }
    if !paragraphs.iter().any(|p| is_heading(p)) {
        return parts(paragraphs);
    }
    let mut chapters: Vec<(String, Vec<Block>)> = Vec::new();
    for paragraph in paragraphs {
        if is_heading(paragraph) {
            let heading = Block::of(BlockKind::Heading { level: 1 }, paragraph.clone());
            chapters.push((paragraph.clone(), vec![heading]));
            continue;
        }
        let block = Block::of(BlockKind::Paragraph, paragraph.clone());
        match chapters.last_mut() {
            Some((_, blocks)) => blocks.push(block),
            None => chapters.push((paragraph.clone(), vec![block])),
        }
    }
    chapters
        .into_iter()
        .map(|(title, blocks)| Chapter::of(title, blocks, PATH))
        .collect()
}

fn parts(paragraphs: &[String]) -> Vec<Chapter> {
    let mut parts: Vec<Vec<Block>> = vec![Vec::new()];
    let mut words = 0;
    for paragraph in paragraphs {
        let count = paragraph.split_whitespace().count();
        if words > 0 && words + count > PART_WORDS {
            parts.push(Vec::new());
            words = 0;
        }
        words += count;
        parts
            .last_mut()
            .unwrap()
            .push(Block::of(BlockKind::Paragraph, paragraph.clone()));
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(n, blocks)| Chapter::of(format!("Part {}", n + 1), blocks, PATH))
        .collect()
}
