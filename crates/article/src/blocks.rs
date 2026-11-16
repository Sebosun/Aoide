//! The main content's HTML as blocks: headings, paragraphs, list items and preformatted text,
//! in reading order. Inline markup is flattened to its text. Images, figures, tables, embeds
//! and forms are left out and counted. The walk keeps its own stack, so nesting depth costs
//! memory, not call depth.

use std::collections::BTreeMap;

use aoide_document::{Block, BlockKind, LeftOut, Warning};
use ego_tree::NodeRef;
use scraper::{Html, Node};

use crate::ExtractOptions;

pub(crate) struct Body {
    pub blocks: Vec<Block>,
    pub warnings: Vec<Warning>,
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

pub(crate) fn from_html(html: &str, options: &ExtractOptions) -> Body {
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
    }
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
