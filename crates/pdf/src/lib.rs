//! Reflowed text from PDF files, as Aoide's parsed-book model (`aoide_document::ParsedBook`), so a
//! PDF is read, followed and spoken like an EPUB. Pure and deterministic: the bytes in, the book
//! out; nothing is written and nothing is fetched.
//!
//! A PDF places lines on pages; this joins them back into paragraphs. A paragraph starts after a
//! vertical gap, at an indented line, or after a short line that ends a sentence; a word broken
//! at a line end is mended, and a sentence that runs over the page goes on. Running heads and
//! page numbers (lines at the same height at the top or bottom of most pages) are left out, and
//! a line set larger than the text is a heading. Chapters follow the outline (bookmarks); without
//! one they are runs of ten pages. Title, authors, keywords and subject come from the document's
//! Info dictionary. Scanned pages carry no text: such a PDF is refused with [`PdfError::NoText`].

#[cfg(feature = "testing")]
pub mod testing;

use std::collections::HashMap;

use aoide_document::{Block, BlockKind, Chapter, ParsedBook, SourceLocation};
use pdf_oxide::object::Object;
use pdf_oxide::{Destination, PdfDocument, ReadingOrder};

/// Pages per chapter when the PDF has no outline.
const PAGES_PER_RUN: usize = 10;
/// A line this much larger than the text is a heading.
const HEADING_SCALE: f32 = 1.25;
/// A gap between lines this many times the text's size starts a paragraph.
const PARAGRAPH_GAP: f32 = 1.6;
/// A line at the same height on this share of pages (in hundredths) is a running head or foot.
const RUNNING_SHARE: usize = 40;
/// Hundredths of line pairs that must be an indented line running on for a hanging indent.
const HANGING_SHARE: usize = 15;
/// Headings and running lines are short.
const SHORT_WORDS: usize = 15;

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("not a readable PDF: {0}")]
    Unreadable(String),
    #[error("this PDF is locked with a password")]
    Locked,
    #[error("this PDF has no text to read: its pages are pictures, as in a scan")]
    NoText,
}

/// A PDF's own description of itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    /// Its keywords, for tags.
    pub subjects: Vec<String>,
    /// Its subject line, as a description.
    pub description: Option<String>,
}

/// The document's Info dictionary, without reading its pages.
pub fn read_pdf_metadata(bytes: &[u8]) -> Result<PdfMetadata, PdfError> {
    Ok(metadata(&open(bytes)?))
}

/// Whether the PDF has text to read, looking only as far as its first page with some (a scan
/// has none, and is refused as [`parse_pdf`] would refuse it).
pub fn check_text(bytes: &[u8]) -> Result<(), PdfError> {
    let doc = open(bytes)?;
    let pages = doc
        .page_count()
        .map_err(|e| PdfError::Unreadable(e.to_string()))?;
    if (0..pages).any(|page| !page_lines(&doc, page).is_empty()) {
        Ok(())
    } else {
        Err(PdfError::NoText)
    }
}

/// The PDF's text as chapters of paragraphs and headings.
pub fn parse_pdf(bytes: &[u8]) -> Result<ParsedBook, PdfError> {
    let doc = open(bytes)?;
    let meta = metadata(&doc);
    let pages = doc
        .page_count()
        .map_err(|e| PdfError::Unreadable(e.to_string()))?;
    let lines: Vec<Vec<Line>> = (0..pages).map(|page| page_lines(&doc, page)).collect();
    if lines.iter().all(Vec::is_empty) {
        return Err(PdfError::NoText);
    }
    let lines = without_running_lines(lines);
    let blocks = paragraphs(&lines);
    let chapters = chapters(&doc, pages, blocks, meta.title.as_deref());
    Ok(ParsedBook {
        title: meta.title,
        authors: meta.authors,
        language: None,
        publisher: None,
        description: meta.description,
        identifiers: Vec::new(),
        isbn: None,
        subjects: meta.subjects,
        cover: None,
        notes: Vec::new(),
        chapters,
        warnings: Vec::new(),
        excluded: Vec::new(),
    })
}

/// Whether `bytes` are a PDF: its header within the first kilobyte (after a little junk, as
/// readers allow).
pub fn is_pdf(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(1024)]
        .windows(5)
        .any(|w| w == b"%PDF-")
}

fn open(bytes: &[u8]) -> Result<PdfDocument, PdfError> {
    if !is_pdf(bytes) {
        return Err(PdfError::Unreadable("no PDF header".into()));
    }
    let doc =
        PdfDocument::from_bytes(bytes.to_vec()).map_err(|e| PdfError::Unreadable(e.to_string()))?;
    if doc.is_encrypted() && doc.page_count().is_err() {
        return Err(PdfError::Locked);
    }
    Ok(doc)
}

fn metadata(doc: &PdfDocument) -> PdfMetadata {
    let info = doc
        .trailer()
        .as_dict()
        .and_then(|trailer| trailer.get("Info"))
        .and_then(|info| match info {
            Object::Reference(r) => doc.load_object(*r).ok(),
            other => Some(other.clone()),
        });
    let Some(Object::Dictionary(info)) = info else {
        return PdfMetadata::default();
    };
    let text = |key: &str| -> Option<String> {
        let value = match info.get(key)? {
            Object::Reference(r) => doc.load_object(*r).ok()?,
            other => other.clone(),
        };
        let text = pdf_oxide::optional_content::decode_pdf_text_string(value.as_string()?);
        let text = collapse(&text);
        (!text.is_empty()).then_some(text)
    };
    let split = |value: Option<String>, at: &[&str]| -> Vec<String> {
        let Some(mut value) = value else {
            return Vec::new();
        };
        for separator in at {
            value = value.replace(separator, ";");
        }
        value
            .split(';')
            .map(collapse)
            .filter(|s| !s.is_empty())
            .collect()
    };
    PdfMetadata {
        title: text("Title").and_then(|t| plausible_title(&t)),
        authors: split(text("Author"), &[" and ", " & "])
            .into_iter()
            .filter(|a| plausible_author(a))
            .collect(),
        subjects: split(text("Keywords"), &[","]),
        description: text("Subject"),
    }
}

/// A title as the program that made the PDF often fills it in: the document's file name
/// ("RT9093_FM.fm", "Microsoft Word - draft.docx") or a placeholder.
fn plausible_title(title: &str) -> Option<String> {
    let title = title
        .strip_prefix("Microsoft Word - ")
        .unwrap_or(title)
        .trim();
    let lower = title.to_lowercase();
    const FILE_ENDINGS: &[&str] = &[
        ".fm", ".doc", ".docx", ".indd", ".qxd", ".qxp", ".pdf", ".tex", ".dvi", ".rtf", ".txt",
        ".odt", ".p65", ".pm6", ".ps", ".eps", ".html", ".htm",
    ];
    let placeholder = matches!(
        lower.as_str(),
        "untitled" | "untitled document" | "title" | "document" | "no title"
    );
    let file = FILE_ENDINGS.iter().any(|e| lower.ends_with(e));
    (!placeholder && !file && title.chars().any(char::is_alphabetic)).then(|| title.to_string())
}

/// An author as the program that made the PDF often fills it in: the computer's user name.
fn plausible_author(author: &str) -> bool {
    let lower = author.to_lowercase();
    let account = matches!(
        lower.as_str(),
        "user" | "admin" | "administrator" | "owner" | "unknown" | "author" | "default"
    );
    // One word with digits in it ("DPIUser16") is an account, not a name.
    let handle = !author.contains(' ') && author.chars().any(|c| c.is_ascii_digit());
    !account && !handle && author.chars().any(char::is_alphabetic)
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A line of text where the page puts it (points; `y` grows up the page).
#[derive(Debug, Clone)]
struct Line {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    text: String,
    /// The last run of text ended with a space.
    spaced_end: bool,
}

/// A page's lines in reading order, column by column: its runs of text (in the order the
/// column-aware reading gives) joined where they stand side by side on one line.
fn page_lines(doc: &PdfDocument, page: usize) -> Vec<Line> {
    let spans = doc
        .extract_spans_with_reading_order(page, ReadingOrder::ColumnAware)
        .unwrap_or_default();
    let mut lines: Vec<Line> = Vec::new();
    for span in spans {
        let text = collapse(&span.text);
        if text.is_empty() {
            continue;
        }
        let height = if span.font_size > 0.0 {
            span.font_size
        } else {
            span.bbox.height
        }
        .max(1.0);
        if let Some(last) = lines.last_mut() {
            let end = last.x + last.width;
            let gap = span.bbox.x - end;
            let same_row = (last.y - span.bbox.y).abs() <= last.height.min(height) * 0.5;
            if same_row && gap >= -height * 0.5 && gap <= height * 3.0 {
                let spaced = gap > height * 0.15
                    || span.text.starts_with(char::is_whitespace)
                    || last.spaced_end;
                last.text = if spaced {
                    format!("{} {text}", last.text)
                } else {
                    format!("{}{text}", last.text)
                };
                last.width = (span.bbox.x + span.bbox.width).max(end) - last.x;
                last.height = last.height.max(height);
                last.spaced_end = span.text.ends_with(char::is_whitespace);
                continue;
            }
        }
        lines.push(Line {
            x: span.bbox.x,
            y: span.bbox.y,
            width: span.bbox.width,
            height,
            spaced_end: span.text.ends_with(char::is_whitespace),
            text,
        });
    }
    lines
}

/// The pages without their running heads and feet: a line that is the highest or the lowest on
/// its page, short, and at the same height there on enough pages (those with a few lines).
fn without_running_lines(pages: Vec<Vec<Line>>) -> Vec<Vec<Line>> {
    let height = |y: f32| (y / 2.0).round() as i32;
    let full: Vec<&Vec<Line>> = pages.iter().filter(|p| p.len() >= 3).collect();
    let enough = (full.len() * RUNNING_SHARE).div_ceil(100).max(3);
    let mut top: HashMap<i32, usize> = HashMap::new();
    let mut bottom: HashMap<i32, usize> = HashMap::new();
    let extremes = |page: &Vec<Line>| {
        let high = page.iter().map(|l| l.y).fold(f32::MIN, f32::max);
        let low = page.iter().map(|l| l.y).fold(f32::MAX, f32::min);
        (high, low)
    };
    for page in &full {
        let (high, low) = extremes(page);
        *top.entry(height(high)).or_default() += 1;
        *bottom.entry(height(low)).or_default() += 1;
    }
    pages
        .into_iter()
        .map(|page| {
            if page.len() < 3 {
                return page;
            }
            let (high, low) = extremes(&page);
            page.into_iter()
                .filter(|line| {
                    let short = line.text.split_whitespace().count() <= SHORT_WORDS;
                    let running = (line.y == high && top[&height(high)] >= enough)
                        || (line.y == low && bottom[&height(low)] >= enough);
                    !(short && running)
                })
                .collect()
        })
        .collect()
}

/// A block and the page it starts on.
struct Placed {
    page: usize,
    block: Block,
}

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

fn ends_sentence(text: &str) -> bool {
    text.trim_end()
        .ends_with(['.', '!', '?', '…', ':', '"', '”', '’', ')'])
}

/// The right edge of each page's text (where full lines end), or of the whole document's on a
/// page with few lines.
fn right_edges(pages: &[Vec<Line>]) -> Vec<f32> {
    let edge = |lines: &mut dyn Iterator<Item = &Line>| {
        let mut edges: Vec<f32> = lines.map(|l| l.x + l.width).collect();
        edges.sort_by(f32::total_cmp);
        edges.get(edges.len() * 9 / 10).copied().unwrap_or(0.0)
    };
    let all = edge(&mut pages.iter().flatten());
    pages
        .iter()
        .map(|lines| {
            if lines.len() >= 5 {
                edge(&mut lines.iter())
            } else {
                all
            }
        })
        .collect()
}

/// Whether paragraphs are set with a hanging indent (an entry's later lines indented, as in a
/// dictionary) rather than an indented first line: a line before an indented one mostly runs on
/// (the entry goes on) rather than ending a sentence (a paragraph ended), and such lines are
/// common (in columns, captions and drop caps make a few).
fn hanging(pages: &[Vec<Line>], size: f32) -> bool {
    let (mut runs_on, mut ends) = (0, 0);
    for pair in pages.iter().flat_map(|lines| lines.windows(2)) {
        let (a, b) = (&pair[0], &pair[1]);
        let gap = a.y - b.y;
        if gap > 0.0 && gap <= size * PARAGRAPH_GAP && b.x > a.x + size * 0.8 {
            if ends_sentence(&a.text) {
                ends += 1;
            } else {
                runs_on += 1;
            }
        }
    }
    let pairs: usize = pages.iter().map(|l| l.len().saturating_sub(1)).sum();
    runs_on > 2 * ends && runs_on * 100 >= pairs * HANGING_SHARE
}

/// The paragraph being built.
struct Open {
    page: usize,
    text: String,
    /// Its first line's left edge, and the leftmost of its lines.
    first: f32,
    left: f32,
    lines: usize,
    last: Line,
}

/// The lines of every page joined into paragraphs and headings, in order.
fn paragraphs(pages: &[Vec<Line>]) -> Vec<Placed> {
    let size = median(pages.iter().flatten().map(|l| l.height).collect());
    let rights = right_edges(pages);
    let hanging = hanging(pages, size);
    let mut out: Vec<Placed> = Vec::new();
    let mut open: Option<Open> = None;
    // The last heading line, while a heading may go on to the next line.
    let mut heading: Option<Line> = None;
    let close = |open: &mut Option<Open>, out: &mut Vec<Placed>| {
        if let Some(paragraph) = open.take() {
            out.push(Placed {
                page: paragraph.page,
                block: Block {
                    kind: BlockKind::Paragraph,
                    text: paragraph.text,
                },
            });
        }
    };
    for (page, lines) in pages.iter().enumerate() {
        let right = rights[page];
        for (i, line) in lines.iter().enumerate() {
            let words = line.text.split_whitespace().count();
            if line.height >= size * HEADING_SCALE && words <= SHORT_WORDS {
                close(&mut open, &mut out);
                // A heading over two lines: the same size just below the last heading line.
                let goes_on = heading.as_ref().is_some_and(|last| {
                    i > 0
                        && (line.height - last.height).abs() <= last.height * 0.15
                        && last.y - line.y > 0.0
                        && last.y - line.y <= last.height * 2.0
                });
                match out.last_mut() {
                    Some(placed) if goes_on => {
                        placed.block.text = format!("{} {}", placed.block.text, line.text);
                    }
                    _ => out.push(Placed {
                        page,
                        block: Block {
                            kind: BlockKind::Heading { level: 2 },
                            text: line.text.clone(),
                        },
                    }),
                }
                heading = Some(line.clone());
                continue;
            }
            heading = None;
            let starts = match &open {
                None => true,
                // Over a page: the paragraph goes on unless its sentence ended.
                Some(paragraph) if i == 0 => ends_sentence(&paragraph.last.text),
                Some(paragraph) => {
                    let last = &paragraph.last;
                    let gap = last.y - line.y;
                    let moved_up = gap < -size * 0.5;
                    let spaced = gap > size * PARAGRAPH_GAP;
                    let new_line = if hanging {
                        // An entry goes on only on lines indented from its first.
                        line.x <= paragraph.first + size * 0.8
                    } else {
                        paragraph.lines >= 2 && line.x > paragraph.left + size * 0.8
                    };
                    let short_end =
                        last.x + last.width < right - size * 2.0 && ends_sentence(&last.text);
                    moved_up || spaced || new_line || short_end
                }
            };
            if starts {
                close(&mut open, &mut out);
                open = Some(Open {
                    page,
                    text: line.text.clone(),
                    first: line.x,
                    left: line.x,
                    lines: 1,
                    last: line.clone(),
                });
                continue;
            }
            let paragraph = open.as_mut().expect("a paragraph is open");
            let mended = paragraph.text.strip_suffix('-').filter(|before| {
                before.ends_with(|c: char| c.is_alphabetic())
                    && line.text.starts_with(|c: char| c.is_lowercase())
            });
            paragraph.text = match mended {
                Some(before) => format!("{before}{}", line.text),
                None => format!("{} {}", paragraph.text, line.text),
            };
            paragraph.left = paragraph.left.min(line.x);
            paragraph.lines += 1;
            paragraph.last = line.clone();
        }
    }
    close(&mut open, &mut out);
    out
}

/// The blocks gathered into chapters: by the outline's top-level entries when it gives a few
/// with pages, else by runs of pages.
fn chapters(
    doc: &PdfDocument,
    pages: usize,
    blocks: Vec<Placed>,
    title: Option<&str>,
) -> Vec<Chapter> {
    let mut starts: Vec<(usize, String)> = doc
        .get_outline()
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| match item.dest {
            Some(Destination::PageIndex(page)) if page < pages => {
                Some((page, collapse(&item.title)))
            }
            _ => None,
        })
        .filter(|(_, title)| !title.is_empty())
        .collect();
    starts.sort_by_key(|(page, _)| *page);
    starts.dedup_by_key(|(page, _)| *page);
    // An entry for every page or so is an index of pages, not chapters.
    let outlined = !starts.is_empty() && (pages <= 20 || starts.len() * 2 <= pages);
    let spans: Vec<(usize, String, bool)> = if outlined {
        let mut spans = Vec::new();
        if starts[0].0 > 0 {
            spans.push((0, "Opening".to_string(), false));
        }
        spans.extend(starts.into_iter().map(|(page, title)| (page, title, true)));
        spans
    } else if pages <= PAGES_PER_RUN {
        vec![(0, title.unwrap_or("Text").to_string(), false)]
    } else {
        (0..pages)
            .step_by(PAGES_PER_RUN)
            .map(|start| {
                let end = (start + PAGES_PER_RUN).min(pages);
                (start, format!("Pages {}–{end}", start + 1), false)
            })
            .collect()
    };
    let mut blocks = blocks.into_iter().peekable();
    let mut chapters = Vec::new();
    for (i, (start, title, headed)) in spans.iter().enumerate() {
        let end = spans.get(i + 1).map_or(pages, |next| next.0);
        let mut own: Vec<Block> = Vec::new();
        while let Some(placed) = blocks.next_if(|b| b.page < end) {
            own.push(placed.block);
        }
        if *headed {
            // The chapter's title is its heading: the first line when it says it, else added.
            match own.first_mut() {
                Some(first) if collapse(&first.text).eq_ignore_ascii_case(title) => {
                    first.kind = BlockKind::Heading { level: 1 };
                }
                _ => own.insert(
                    0,
                    Block {
                        kind: BlockKind::Heading { level: 1 },
                        text: title.clone(),
                    },
                ),
            }
        }
        if own
            .iter()
            .all(|b| matches!(b.kind, BlockKind::Heading { .. }))
        {
            continue;
        }
        chapters.push(Chapter {
            title: title.clone(),
            text: own
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
            blocks: own,
            note_refs: Vec::new(),
            sources: vec![SourceLocation {
                path: format!("page-{}", start + 1),
                fragment: None,
            }],
        });
    }
    chapters
}
