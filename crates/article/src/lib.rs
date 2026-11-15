//! Readable text from a stored web page. Pure: the same HTML and URL always give the same
//! result, nothing is fetched and no script runs. The page is first narrowed to its main
//! content by a readability-style extractor, then turned into the blocks books are read from.

mod blocks;
mod scan;

use aoide_document::{Chapter, ParsedBook, SourceLocation};
use url::Url;

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
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArticleMeta {
    pub title: Option<String>,
    pub word_count: usize,
}

/// Reads the article in `html`, which was fetched from `url`.
pub fn extract(html: &str, url: &Url, options: &ExtractOptions) -> Extracted {
    let (body, title) = if scan::nesting(html) > options.max_depth {
        (scan::plain(html, options), scan::title(html))
    } else {
        match readable(html, url, options) {
            Some(article) => (
                blocks::from_html(&article.content, options),
                Some(blocks::collapse(&article.title)).filter(|t| !t.is_empty()),
            ),
            None => (blocks::from_html("", options), None),
        }
    };
    let word_count = body.blocks.iter().map(|b| words(&b.text)).sum();
    let texts: Vec<&str> = body.blocks.iter().map(|b| b.text.as_str()).collect();
    let chapter = Chapter {
        title: title
            .clone()
            .unwrap_or_else(|| url.host_str().unwrap_or("Article").into()),
        text: texts.join("\n\n"),
        blocks: body.blocks,
        note_refs: Vec::new(),
        sources: vec![SourceLocation {
            path: url.as_str().into(),
            fragment: None,
        }],
    };
    Extracted {
        document: ParsedBook {
            title: title.clone(),
            authors: Vec::new(),
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![url.as_str().into()],
            isbn: None,
            cover: None,
            notes: Vec::new(),
            chapters: vec![chapter],
            warnings: body.warnings,
            excluded: Vec::new(),
        },
        meta: ArticleMeta { title, word_count },
    }
}

fn config(options: &ExtractOptions) -> dom_smoothie::Config {
    dom_smoothie::Config {
        max_elements_to_parse: options.max_elements,
        ..Default::default()
    }
}

/// The page narrowed to its main content; `None` when the extractor found none.
fn readable(html: &str, url: &Url, options: &ExtractOptions) -> Option<dom_smoothie::Article> {
    dom_smoothie::Readability::new(html, Some(url.as_str()), Some(config(options)))
        .and_then(|mut reader| reader.parse())
        .ok()
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}
