//! Readable text from a stored web page. Pure: the same HTML and URL always give the same
//! result, nothing is fetched and no script runs. The page is first narrowed to its main
//! content by a readability-style extractor, then turned into the blocks books are read from.

mod address;
mod blocks;
mod completeness;
mod meta;
mod patterns;
mod scan;

use aoide_document::{Block, BlockKind, Chapter, ParsedBook, SourceLocation, Warning};
use url::Url;

pub use address::{canonical_id, normalise};
pub use completeness::{Completeness, WallKind};
pub use meta::FeedLink;

/// Bounds on the work one extraction may do.
#[derive(Debug, Clone)]
pub struct ExtractOptions {
    /// Element nesting past which the page is read as plain text: building a tree for it
    /// would cost more than linear time.
    pub max_depth: usize,
    /// Elements the content finder looks at; a larger page is read only this far.
    pub max_elements: usize,
    /// Blocks kept; the rest of a longer body is dropped.
    pub max_blocks: usize,
    /// Bytes of text kept across all blocks.
    pub max_text_bytes: usize,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            max_depth: 256,
            max_elements: 200_000,
            max_blocks: 20_000,
            max_text_bytes: 4 * 1024 * 1024,
        }
    }
}

/// What a page gave.
#[derive(Debug, PartialEq, Eq)]
pub struct Extracted {
    /// One chapter; headings are kept as heading blocks.
    pub document: ParsedBook,
    pub meta: ArticleMeta,
    pub completeness: Completeness,
    /// Feeds the page links to (`<link rel="alternate">`).
    pub feeds: Vec<FeedLink>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArticleMeta {
    pub title: Option<String>,
    pub byline: Option<String>,
    pub site_name: Option<String>,
    pub published: Option<Published>,
    pub language: Option<String>,
    pub canonical_url: Option<Url>,
    pub lead_image: Option<Url>,
    /// Words in the text that was captured.
    pub word_count: usize,
    pub declared_words: Option<usize>,
}

/// A publication date as the page wrote it, and the instant when it parses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub text: String,
    pub at_ms: Option<i64>,
}

/// Reads the article in `html`, which was fetched from `url`.
pub fn extract(html: &str, url: &Url, options: &ExtractOptions) -> Extracted {
    let (body, read, signs, feeds) = if scan::nesting(html) > options.max_depth {
        let meta = ArticleMeta {
            title: scan::title(html),
            ..ArticleMeta::default()
        };
        let read = meta::Read {
            meta,
            description: None,
            declared: Default::default(),
        };
        let signs = completeness::Signs::default();
        (scan::plain(html, options), read, signs, Vec::new())
    } else {
        let found = readable(html, url, options);
        let page = scraper::Html::parse_document(html);
        (
            blocks::from_html(&found.content, options),
            meta::read(&page, url, found.title, found.byline),
            completeness::Signs::of(&page),
            meta::feeds(&page, url),
        )
    };
    let mut blocks = body.blocks;
    let mut warnings = body.warnings;
    let mut meta = read.meta;
    let declared = read.declared;
    // A page whose markup falls short of the text its metadata carries (as a capture is judged
    // cut short): read the metadata's. A body a little longer (captions, a credit line) is the
    // same text, and the markup keeps its headings and lists.
    if let Some(full) = &declared.body {
        let captured: usize = blocks.iter().map(|b| words(&b.text)).sum();
        if captured * 100 < words(full) * patterns::CUT_SHORT_PERCENT {
            blocks = paragraphs(full, options);
            warnings.push(Warning::BodyFromMetadata);
        }
    }
    // The title is shown above the text; a first heading saying the same is not repeated.
    if let (Some(title), Some(first)) = (&meta.title, blocks.first()) {
        if matches!(first.kind, BlockKind::Heading { .. }) && same_words(title, &first.text) {
            blocks.remove(0);
        }
    }
    meta.word_count = blocks.iter().map(|b| words(&b.text)).sum();
    meta.declared_words = declared
        .word_count
        .or_else(|| declared.body.as_deref().map(words));
    let completeness =
        completeness::judge(meta.word_count, meta.declared_words, declared.free, &signs);
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    let chapter = Chapter {
        title: meta
            .title
            .clone()
            .unwrap_or_else(|| url.host_str().unwrap_or("Article").into()),
        text: texts.join("\n\n"),
        blocks,
        note_refs: Vec::new(),
        sources: vec![SourceLocation {
            path: url.as_str().into(),
            fragment: None,
        }],
    };
    let address = meta.canonical_url.as_ref().unwrap_or(url);
    Extracted {
        document: ParsedBook {
            title: meta.title.clone(),
            authors: meta.byline.iter().cloned().collect(),
            language: meta.language.clone(),
            publisher: meta.site_name.clone(),
            description: read.description,
            identifiers: vec![address.as_str().into()],
            isbn: None,
            subjects: Vec::new(),
            cover: None,
            notes: Vec::new(),
            chapters: vec![chapter],
            warnings,
            excluded: Vec::new(),
        },
        meta,
        completeness,
        feeds,
    }
}

/// Text written as paragraphs separated by blank lines, as blocks.
fn paragraphs(text: &str, options: &ExtractOptions) -> Vec<Block> {
    let text = text.replace("\r\n", "\n");
    let mut blocks = Vec::new();
    let mut bytes = 0;
    let mut paragraph = String::new();
    for line in text.lines().chain(std::iter::once("")) {
        if !line.trim().is_empty() {
            paragraph.push_str(line);
            paragraph.push(' ');
            continue;
        }
        let text = blocks::collapse(&html_escape::decode_html_entities(&paragraph));
        paragraph.clear();
        if text.is_empty() {
            continue;
        }
        if blocks.len() >= options.max_blocks || bytes >= options.max_text_bytes {
            break;
        }
        bytes += text.len();
        blocks.push(Block {
            kind: BlockKind::Paragraph,
            text,
        });
    }
    blocks
}

/// The same words, ignoring case and spacing.
fn same_words(a: &str, b: &str) -> bool {
    let words = |s: &str| {
        s.split_whitespace()
            .map(str::to_lowercase)
            .collect::<Vec<_>>()
    };
    words(a) == words(b)
}

fn config(options: &ExtractOptions) -> dom_smoothie::Config {
    dom_smoothie::Config {
        max_elements_to_parse: options.max_elements,
        ..Default::default()
    }
}

/// What the content finder found.
#[derive(Default)]
struct Found {
    /// The page narrowed to its main content, as HTML (empty when none was found).
    content: String,
    /// The page's `<title>` with the site's name taken off.
    title: Option<String>,
    /// The byline, from the page's metadata or else from the article ("By …" near its top).
    byline: Option<String>,
}

fn readable(html: &str, url: &Url, options: &ExtractOptions) -> Found {
    let Ok(mut reader) =
        dom_smoothie::Readability::new(html, Some(url.as_str()), Some(config(options)))
    else {
        return Found::default();
    };
    let title = Some(blocks::collapse(&reader.get_article_title())).filter(|t| !t.is_empty());
    let Ok(article) = reader.parse() else {
        return Found {
            title,
            ..Found::default()
        };
    };
    Found {
        content: article.content.to_string(),
        title,
        byline: article.byline,
    }
}

/// Words in `text`: what spaces separate, except in Chinese and Japanese, which are written
/// without them; there each character counts as a word (as reading-time estimates count them),
/// and so does each run of other letters or digits beside them ("DeepMind开发" is three).
pub(crate) fn words(text: &str) -> usize {
    text.split_whitespace()
        .map(|token| {
            if !token.chars().any(spaceless) {
                return 1;
            }
            let mut count = 0;
            let mut in_run = false;
            for c in token.chars() {
                if spaceless(c) {
                    count += 1;
                    in_run = false;
                } else if c.is_alphanumeric() {
                    count += usize::from(!in_run);
                    in_run = true;
                } else {
                    in_run = false;
                }
            }
            count
        })
        .sum()
}

/// A character of a script written without spaces between words: Han (CJK ideographs and their
/// extensions), Hiragana and Katakana.
fn spaceless(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{30ff}'
        | '\u{31f0}'..='\u{31ff}'
        | '\u{3400}'..='\u{4dbf}'
        | '\u{4e00}'..='\u{9fff}'
        | '\u{f900}'..='\u{faff}'
        | '\u{ff66}'..='\u{ff9f}'
        | '\u{20000}'..='\u{3134f}')
}
