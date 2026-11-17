//! A linear pass over the HTML text that builds no tree. HTML tree building costs more than
//! linear time in nesting depth, so a page nested past `ExtractOptions::max_depth` is read
//! here instead: its text, split into paragraphs at block tags, with page chrome, scripts and
//! styles left out.

use aoide_document::{Block, BlockKind};

use crate::blocks::{collapse, Body};
use crate::ExtractOptions;

/// Elements HTML closes on its own or that hold nothing: they add no nesting.
const NO_DEPTH: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr", "p", "li", "dt", "dd", "option", "tr", "td", "th", "!doctype",
];

/// Elements whose content is not text to read.
const RAW: &[&str] = &["script", "style", "template", "noscript", "svg", "math"];

/// Page chrome: the walk skips what these hold.
const CHROME: &[&str] = &[
    "nav", "header", "footer", "aside", "form", "button", "select",
];

/// Elements whose start or end separates paragraphs.
const BREAKS: &[&str] = &[
    "p",
    "div",
    "li",
    "ul",
    "ol",
    "dl",
    "dt",
    "dd",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "br",
    "hr",
    "blockquote",
    "pre",
    "section",
    "article",
    "main",
    "table",
    "tr",
    "td",
    "th",
    "body",
    "figure",
    "figcaption",
];

struct Tag<'a> {
    name: String,
    closing: bool,
    self_closing: bool,
    /// The input after the tag.
    rest: &'a str,
}

/// The tag at the start of `html` (which begins with `<`), or `None` when it is not one.
/// Every search stops at the next `<`, so reading a page tag by tag stays linear.
fn tag(html: &str) -> Option<Tag<'_>> {
    let body = &html[1..];
    let (closing, body) = match body.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    let name: String = body
        .bytes()
        .take(33)
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'!' || *b == b'-')
        .map(|b| b.to_ascii_lowercase() as char)
        .collect();
    if name.is_empty()
        || name.len() > 32
        || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '!')
    {
        return None;
    }
    let end = body.find(['>', '<'])?;
    if body.as_bytes()[end] == b'<' {
        return None;
    }
    Some(Tag {
        name,
        closing,
        self_closing: body[..end].ends_with('/'),
        rest: &body[end + 1..],
    })
}

/// Where `</name` next starts in `html`, ignoring case, or the end.
fn close_of(html: &str, name: &str) -> usize {
    let needle = format!("</{name}");
    html.as_bytes()
        .windows(needle.len())
        .position(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
        .unwrap_or(html.len())
}

/// How deeply the page nests its elements, counted from its tags.
pub(crate) fn nesting(html: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        let Some(tag) = tag(rest) else {
            rest = &rest[1..];
            continue;
        };
        rest = tag.rest;
        if RAW.contains(&tag.name.as_str()) && !tag.closing && !tag.self_closing {
            rest = &rest[close_of(rest, &tag.name)..];
            continue;
        }
        if NO_DEPTH.contains(&tag.name.as_str()) || tag.self_closing {
            continue;
        }
        if tag.closing {
            depth = depth.saturating_sub(1);
        } else {
            depth += 1;
            deepest = deepest.max(depth);
        }
    }
    deepest
}

/// The page's text as paragraphs, without building a tree.
pub(crate) fn plain(html: &str, options: &ExtractOptions) -> Body {
    let mut blocks = Vec::new();
    let (mut running, mut bytes) = (String::new(), 0usize);
    let mut after_break = false;
    let mut flush = |running: &mut String, blocks: &mut Vec<Block>, after_break: &mut bool| {
        let text = collapse(&html_escape::decode_html_entities(running));
        running.clear();
        if !text.is_empty() && blocks.len() < options.max_blocks && bytes < options.max_text_bytes {
            bytes += text.len();
            blocks.push(Block {
                kind: BlockKind::Paragraph,
                text,
                break_before: std::mem::take(after_break),
                layout: None,
            });
        }
    };
    let mut rest = html;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            running.push_str(rest);
            break;
        };
        running.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        let Some(tag) = tag(rest) else {
            running.push('<');
            rest = &rest[1..];
            continue;
        };
        rest = tag.rest;
        let name = tag.name.as_str();
        if !tag.closing && !tag.self_closing && RAW.contains(&name) {
            rest = &rest[close_of(rest, name)..];
            continue;
        }
        if !tag.closing && !tag.self_closing && CHROME.contains(&name) {
            // Chrome can nest its own kind (`header` in `header`): skip to the matching close.
            rest = skip_element(rest, name);
            continue;
        }
        if name == "title" && !tag.closing {
            rest = &rest[close_of(rest, name)..];
            continue;
        }
        if BREAKS.contains(&name) {
            flush(&mut running, &mut blocks, &mut after_break);
        }
        after_break |= name == "hr";
    }
    flush(&mut running, &mut blocks, &mut after_break);
    Body {
        blocks,
        warnings: Vec::new(),
        inserts: Vec::new(),
    }
}

/// `html` after the end of an element named `name` whose start tag was just read.
fn skip_element<'a>(html: &'a str, name: &str) -> &'a str {
    let mut depth = 1usize;
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        match tag(rest) {
            Some(tag) if tag.name == name => {
                rest = tag.rest;
                if tag.closing {
                    depth -= 1;
                    if depth == 0 {
                        return rest;
                    }
                } else if !tag.self_closing {
                    depth += 1;
                }
            }
            Some(tag) => rest = tag.rest,
            None => rest = &rest[1..],
        }
    }
    ""
}

/// The text of the page's `<title>`.
pub(crate) fn title(html: &str) -> Option<String> {
    let start = html
        .as_bytes()
        .windows(6)
        .position(|w| w.eq_ignore_ascii_case(b"<title"))?;
    let tag = tag(&html[start..])?;
    let text = &tag.rest[..close_of(tag.rest, "title")];
    let text = collapse(&html_escape::decode_html_entities(text));
    (!text.is_empty()).then_some(text)
}
