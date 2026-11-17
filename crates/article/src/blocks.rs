//! The main content's HTML as blocks: headings, paragraphs, list items and preformatted text,
//! in reading order. Inline markup is flattened to its text. A data table is kept beside the
//! text (an insert), and so is a picture on the web (its address, description and caption);
//! other images and figures, layout tables, embeds and forms are left out and counted. The walk keeps its own stack, so nesting depth costs
//! memory, not call depth.

use std::collections::BTreeMap;

use aoide_document::{Block, BlockKind, Caption, Insert, InsertKind, LeftOut, Warning};
use ego_tree::NodeRef;
use scraper::{Html, Node};

use crate::ExtractOptions;

pub(crate) struct Body {
    pub blocks: Vec<Block>,
    pub warnings: Vec<Warning>,
    /// Tables kept beside the text, before the block they precede.
    pub inserts: Vec<Insert>,
}

/// Elements whose text runs on within the surrounding block.
const INLINE: &[&str] = &[
    "a", "abbr", "acronym", "b", "bdi", "bdo", "big", "cite", "code", "data", "del", "dfn", "em",
    "font", "i", "ins", "kbd", "mark", "q", "s", "samp", "small", "span", "strike", "strong",
    "sub", "sup", "time", "tt", "u", "var", "wbr", "label", "output",
];

/// Elements that carry nothing to read.
const SILENT: &[&str] = &[
    "script", "style", "noscript", "template", "head", "svg", "canvas", "button", "select",
    "textarea", "input", "map",
];

fn left_out(name: &str) -> Option<LeftOut> {
    Some(match name {
        "img" | "picture" => LeftOut::Image,
        "figure" => LeftOut::Figure,
        "table" => LeftOut::Table,
        "iframe" | "embed" | "object" | "video" | "audio" => LeftOut::Embed,
        "form" => LeftOut::Form,
        _ => return None,
    })
}

fn heading(name: &str) -> Option<u8> {
    match name.as_bytes() {
        [b'h', level @ b'1'..=b'6'] => Some(level - b'0'),
        _ => None,
    }
}

enum Step<'a> {
    Visit(NodeRef<'a, Node>, BlockKind),
    /// The end of a block element: what ran on inside it is one block.
    Close,
    /// The end of a list item.
    EndItem,
}

struct Builder<'o> {
    options: &'o ExtractOptions,
    blocks: Vec<Block>,
    bytes: usize,
    left_out: BTreeMap<LeftOut, usize>,
    running: String,
    running_kind: BlockKind,
    /// For each list item being read, how many blocks there were when it began: its first
    /// block is the item, later ones are paragraphs that continue it.
    items: Vec<usize>,
    /// A scene break was read since the last block.
    after_break: bool,
    inserts: Vec<Insert>,
}

impl Builder<'_> {
    fn full(&self) -> bool {
        self.blocks.len() >= self.options.max_blocks || self.bytes >= self.options.max_text_bytes
    }

    fn push(&mut self, kind: BlockKind, text: String) {
        if text.is_empty() || self.full() {
            return;
        }
        let continues = self
            .items
            .last()
            .is_some_and(|&start| self.blocks.len() > start);
        let kind = match kind {
            BlockKind::ListItem if continues => BlockKind::Paragraph,
            kind => kind,
        };
        self.bytes += text.len();
        self.blocks.push(Block {
            kind,
            text,
            break_before: std::mem::take(&mut self.after_break),
            layout: None,
        });
    }

    fn run_on(&mut self, text: &str, kind: BlockKind) {
        if self.running.trim().is_empty() {
            self.running_kind = kind;
        }
        self.running.push_str(text);
    }

    fn flush(&mut self) {
        let text = collapse(&std::mem::take(&mut self.running));
        self.push(self.running_kind, text);
    }
}

pub(crate) fn from_html(html: &str, page: &url::Url, options: &ExtractOptions) -> Body {
    let fragment = Html::parse_fragment(html);
    let mut builder = Builder {
        options,
        blocks: Vec::new(),
        bytes: 0,
        left_out: BTreeMap::new(),
        running: String::new(),
        running_kind: BlockKind::Paragraph,
        items: Vec::new(),
        after_break: false,
        inserts: Vec::new(),
    };
    let mut steps = vec![Step::Visit(fragment.tree.root(), BlockKind::Paragraph)];
    while let Some(step) = steps.pop() {
        if builder.full() {
            break;
        }
        let (node, kind) = match step {
            Step::Visit(node, kind) => (node, kind),
            Step::Close => {
                builder.flush();
                continue;
            }
            Step::EndItem => {
                builder.items.pop();
                continue;
            }
        };
        let element = match node.value() {
            Node::Text(text) => {
                builder.run_on(text, kind);
                continue;
            }
            Node::Element(element) => element,
            Node::Document | Node::Fragment => {
                steps.extend(node.children().rev().map(|c| Step::Visit(c, kind)));
                continue;
            }
            _ => continue,
        };
        let name = element.name();
        if name == "table" {
            builder.flush();
            match data_table(node) {
                Some(kind) => builder.inserts.push(Insert {
                    before_block: builder.blocks.len(),
                    kind,
                }),
                None => *builder.left_out.entry(LeftOut::Table).or_default() += 1,
            }
            continue;
        }
        if matches!(name, "img" | "picture" | "figure") {
            // One inside a line (a formula, an emoji) is part of the line: left out, as the
            // line must not be cut.
            let between_lines = builder.running.trim().is_empty();
            match picture(node, page).filter(|_| between_lines) {
                Some(kind) => {
                    builder.flush();
                    builder.inserts.push(Insert {
                        before_block: builder.blocks.len(),
                        kind,
                    });
                }
                None => *builder.left_out.entry(left_out(name).unwrap()).or_default() += 1,
            }
            continue;
        }
        if let Some(what) = left_out(name) {
            *builder.left_out.entry(what).or_default() += 1;
            continue;
        }
        if SILENT.contains(&name) {
            continue;
        }
        if name == "hr" {
            builder.flush();
            builder.after_break = true;
            continue;
        }
        if name == "br" {
            builder.run_on(" ", kind);
            continue;
        }
        if name == "math" {
            builder.run_on(&formula(node), kind);
            continue;
        }
        if INLINE.contains(&name) {
            steps.extend(node.children().rev().map(|c| Step::Visit(c, kind)));
            continue;
        }
        builder.flush();
        if name == "pre" {
            let text = preformatted(node, &mut builder.left_out);
            builder.push(BlockKind::Preformatted, text);
            continue;
        }
        let inner = match (heading(name), name) {
            (Some(level), _) => BlockKind::Heading { level },
            (None, "li") => BlockKind::ListItem,
            // A paragraph in a list item is the item (or continues it).
            (None, "p" | "blockquote") if kind == BlockKind::ListItem => kind,
            (None, "p" | "blockquote") => BlockKind::Paragraph,
            // Any other block keeps the kind it sits in, so a `div` in a list item is the item.
            (None, _) => kind,
        };
        if name == "li" {
            builder.items.push(builder.blocks.len());
            steps.push(Step::EndItem);
        }
        steps.push(Step::Close);
        steps.extend(node.children().rev().map(|c| Step::Visit(c, inner)));
    }
    builder.flush();
    let warnings = builder
        .left_out
        .iter()
        .map(|(&what, &count)| Warning::LeftOut { what, count })
        .collect();
    Body {
        blocks: builder.blocks,
        warnings,
        inserts: builder.inserts,
    }
}

/// A picture (`img`, or the first in a `picture` or `figure`) with an address on the web: the
/// address resolved against the page's, its description, and a figure's caption.
fn picture(node: NodeRef<'_, Node>, page: &url::Url) -> Option<InsertKind> {
    let element = scraper::ElementRef::wrap(node)?;
    let img = if element.value().name() == "img" {
        element
    } else {
        element
            .descendants()
            .filter_map(scraper::ElementRef::wrap)
            .find(|e| e.value().name() == "img")?
    };
    let src = ["src", "data-src"]
        .iter()
        .filter_map(|name| img.value().attr(name))
        .map(str::trim)
        .find(|src| !src.is_empty() && !src.starts_with("data:"))?;
    let address = page.join(src).ok()?;
    if !matches!(address.scheme(), "http" | "https") {
        return None;
    }
    let caption = (element.value().name() == "figure")
        .then(|| {
            element
                .descendants()
                .filter_map(scraper::ElementRef::wrap)
                .find(|e| e.value().name() == "figcaption")
        })
        .flatten()
        .map(|c| collapse(&c.text().collect::<String>()))
        .filter(|c| !c.is_empty())
        .map(Caption::Text);
    Some(InsertKind::Picture {
        resource: address.into(),
        alt: collapse(img.value().attr("alt").unwrap_or("")),
        caption,
    })
}

/// A table that is a grid of cells (two rows and two columns at least): its rows' cell texts,
/// heading rows and caption. A table inside it (a layout) makes it none.
fn data_table(table: NodeRef<'_, Node>) -> Option<InsertKind> {
    let elements = || {
        table
            .descendants()
            .skip(1)
            .filter_map(scraper::ElementRef::wrap)
    };
    if elements().any(|e| e.value().name() == "table") {
        return None;
    }
    let text = |e: scraper::ElementRef<'_>| collapse(&e.text().collect::<String>());
    let caption = elements()
        .find(|e| e.value().name() == "caption")
        .map(text)
        .filter(|c| !c.is_empty());
    let mut rows = Vec::new();
    let mut header_rows = 0;
    for row in elements().filter(|e| e.value().name() == "tr") {
        let cells: Vec<_> = row
            .children()
            .filter_map(scraper::ElementRef::wrap)
            .filter(|c| matches!(c.value().name(), "td" | "th"))
            .collect();
        if cells.is_empty() {
            continue;
        }
        let in_head = row
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .take_while(|a| a.value().name() != "table")
            .any(|a| a.value().name() == "thead");
        let heading = in_head || cells.iter().all(|c| c.value().name() == "th");
        if heading && header_rows == rows.len() {
            header_rows += 1;
        }
        rows.push(cells.into_iter().map(text).collect::<Vec<String>>());
    }
    // A grid: a single row or a single column is a box around something, not data.
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    (rows.len() >= 2 && columns >= 2).then_some(InsertKind::Table {
        rows,
        header_rows,
        caption,
    })
}

/// A `pre` element's text as written, without its first line break and trailing blank space.
fn preformatted(node: NodeRef<'_, Node>, left: &mut BTreeMap<LeftOut, usize>) -> String {
    let mut text = String::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        match node.value() {
            Node::Text(t) => text.push_str(t),
            Node::Element(e) if e.name() == "br" => text.push('\n'),
            Node::Element(e) => {
                if let Some(what) = left_out(e.name()) {
                    *left.entry(what).or_default() += 1;
                } else if !SILENT.contains(&e.name()) {
                    stack.extend(node.children().rev());
                }
            }
            _ => {}
        }
    }
    let text = text.replace("\r\n", "\n");
    let text = text.strip_prefix('\n').unwrap_or(&text);
    text.trim_end().to_owned()
}

/// A formula's text as it reads in its sentence: its symbols with a space between them, without
/// the source it was written in (TeX in an `annotation`), as EPUBs and Readability read MathML.
fn formula(node: NodeRef<'_, Node>) -> String {
    let mut text = String::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        match node.value() {
            Node::Text(t) => text.push_str(t),
            Node::Element(e) if !e.name().starts_with("annotation") => {
                text.push(' ');
                stack.extend(node.children().rev());
            }
            _ => {}
        }
    }
    collapse(&text)
}

/// Runs of whitespace as one space, none at either end.
pub(crate) fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
