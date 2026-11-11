use crate::{Block, BlockKind, ExclusionReason, Note, NoteRef};
use ego_tree::iter::Edge;
use scraper::{Html, Node};
use std::collections::HashMap;

pub(crate) struct Content {
    pub title: String,
    pub tokens: Vec<String>,
    /// Block kind in effect for each token (parallel to `tokens`).
    pub kinds: Vec<BlockKind>,
    pub anchors: HashMap<String, usize>,
    pub headings: Vec<Heading>,
    pub sections: Vec<Section>,
    pub notes: Vec<Note>,
    refs: HashMap<usize, (String, String)>,
}

pub(crate) struct Section {
    pub start: usize,
    pub end: usize,
    pub kind: Option<ExclusionReason>,
}

pub(crate) struct Heading {
    pub offset: usize,
    pub title: String,
    pub level: u8,
}

impl Content {
    pub fn reason(&self, start: usize, end: usize) -> Option<ExclusionReason> {
        self.classification(start, end).flatten()
    }

    // Outer None means no structural evidence; Some(None) means explicit main text.
    pub fn classification(&self, start: usize, end: usize) -> Option<Option<ExclusionReason>> {
        self.sections
            .iter()
            .filter(|section| {
                section.start < end
                    && section.end > start
                    && self.text(start, section.start.clamp(start, end)).is_empty()
                    && self.text(section.end.clamp(start, end), end).is_empty()
            })
            .min_by_key(|section| section.end - section.start)
            .map(|section| section.kind)
    }

    pub fn text(&self, start: usize, end: usize) -> String {
        join_blocks(&self.blocks(start, end))
    }

    /// Visible lines between token offsets; a line's kind comes from its first visible token.
    pub fn blocks(&self, start: usize, end: usize) -> Vec<Block> {
        self.blocks_and_refs(start, end).0
    }

    pub fn blocks_and_refs(
        &self,
        start: usize,
        end: usize,
    ) -> (Vec<Block>, Vec<(NoteRef, String)>) {
        let mut blocks = Vec::new();
        let mut refs = Vec::new();
        let mut pending = Vec::new();
        let mut line = String::new();
        let mut kind = None;
        let mut finish = |line: &mut String,
                          kind: &mut Option<BlockKind>,
                          pending: &mut Vec<(usize, String, String)>| {
            let text = line.split_whitespace().collect::<Vec<_>>().join(" ");
            if let (false, Some(kind)) = (text.is_empty(), kind.take()) {
                for (offset, label, href) in pending.drain(..) {
                    refs.push((
                        NoteRef {
                            block: blocks.len(),
                            offset: offset.min(text.chars().count()),
                            label,
                            note: None,
                        },
                        href,
                    ));
                }
                blocks.push(Block { kind, text });
            }
            pending.clear();
            line.clear();
        };
        for index in start..end {
            let token = &self.tokens[index];
            if token == "\n" {
                finish(&mut line, &mut kind, &mut pending);
                continue;
            }
            if let Some((label, href)) = self.refs.get(&index) {
                let prefix = line.split_whitespace().collect::<Vec<_>>().join(" ");
                let offset = prefix.chars().count()
                    + usize::from(!prefix.is_empty() && line.ends_with(char::is_whitespace));
                pending.push((offset, label.clone(), href.clone()));
            }
            if kind.is_none() && !token.trim().is_empty() {
                kind = Some(self.kinds[index]);
            }
            line.push_str(token);
        }
        finish(&mut line, &mut kind, &mut pending);
        (blocks, refs)
    }
}

pub(crate) fn join_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub(crate) fn read(source: &str) -> Content {
    let html = Html::parse_document(source);
    let mut content = Content {
        title: "Untitled chapter".into(),
        tokens: Vec::new(),
        kinds: Vec::new(),
        anchors: HashMap::new(),
        headings: Vec::new(),
        sections: Vec::new(),
        notes: Vec::new(),
        refs: HashMap::new(),
    };
    let mut suppressed = 0;
    let mut heading_start = None;
    let mut sections = HashMap::new();
    let mut stack = vec![BlockKind::Paragraph];
    for edge in html.tree.root().traverse() {
        match edge {
            Edge::Open(node) => match node.value() {
                Node::Element(element) => {
                    if suppressed > 0 || hidden(element) {
                        suppressed += 1;
                        continue;
                    }
                    let current = scraper::ElementRef::wrap(node).unwrap();
                    if is_note_body(current) {
                        let body = current;
                        content.notes.push(Note {
                            id: element.id().unwrap().to_owned(),
                            text: note_text(body),
                        });
                        suppressed += 1;
                        continue;
                    }
                    if is_note_ref(current) {
                        let marker = current;
                        let label = marker.text().collect::<String>().trim().to_owned();
                        content.refs.insert(
                            content.tokens.len(),
                            (label, element.attr("href").unwrap_or("").to_owned()),
                        );
                        content.push("", &stack);
                        suppressed += 1;
                        continue;
                    }
                    if let Some(id) = element.id() {
                        content
                            .anchors
                            .entry(id.to_owned())
                            .or_insert(content.tokens.len());
                    }
                    if element.name() == "a" {
                        if let Some(name) = element.attr("name") {
                            content
                                .anchors
                                .entry(name.to_owned())
                                .or_insert(content.tokens.len());
                        }
                    }
                    if let Some(kind) = section_kind(element) {
                        sections.insert(node.id(), content.sections.len());
                        content.sections.push(Section {
                            start: content.tokens.len(),
                            end: content.tokens.len(),
                            kind,
                        });
                    }
                    if let Some(level) = heading_level(element.name()) {
                        heading_start = Some((content.tokens.len(), level));
                    }
                    if is_block(element.name()) {
                        let top = *stack.last().unwrap();
                        stack.push(block_kind(element.name()).unwrap_or(top));
                        content.push("\n", &stack);
                    } else if matches!(element.name(), "td" | "th") {
                        content.push(" ", &stack);
                    }
                }
                Node::Text(text) if suppressed == 0 => content.push(
                    &text
                        .chars()
                        .map(|c| if c.is_whitespace() { ' ' } else { c })
                        .collect::<String>(),
                    &stack,
                ),
                _ => {}
            },
            Edge::Close(node) => {
                if suppressed > 0 {
                    if node.value().is_element() {
                        suppressed -= 1;
                    }
                    continue;
                }
                if let Some(index) = sections.remove(&node.id()) {
                    content.sections[index].end = content.tokens.len();
                }
                if node
                    .value()
                    .as_element()
                    .is_some_and(|e| heading_level(e.name()).is_some())
                {
                    if let Some((offset, level)) = heading_start.take() {
                        let title = content.text(offset, content.tokens.len());
                        if content.headings.is_empty() {
                            content.title = title.clone();
                        }
                        content.headings.push(Heading {
                            offset,
                            title,
                            level,
                        });
                    }
                }
                if node
                    .value()
                    .as_element()
                    .is_some_and(|e| is_block(e.name()))
                {
                    if stack.len() > 1 {
                        stack.pop();
                    }
                    content.push("\n", &stack);
                } else if node
                    .value()
                    .as_element()
                    .is_some_and(|e| matches!(e.name(), "td" | "th"))
                {
                    content.push(" ", &stack);
                }
            }
        }
    }
    content
}

impl Content {
    fn push(&mut self, token: &str, stack: &[BlockKind]) {
        self.tokens.push(token.to_owned());
        self.kinds.push(*stack.last().unwrap());
    }
}

/// Kind for block elements that start a new kind of block; `None` inherits the enclosing kind.
fn block_kind(name: &str) -> Option<BlockKind> {
    match name {
        "p" => Some(BlockKind::Paragraph),
        "li" => Some(BlockKind::ListItem),
        "pre" => Some(BlockKind::Preformatted),
        _ => heading_level(name).map(|level| BlockKind::Heading { level }),
    }
}

fn section_kind(element: &scraper::node::Element) -> Option<Option<ExclusionReason>> {
    element
        .attr("epub:type")
        .into_iter()
        .chain(element.attr("role"))
        .flat_map(str::split_whitespace)
        .find_map(|token| {
            if matches!(token, "bodymatter" | "chapter" | "doc-chapter") {
                Some(None)
            } else {
                ExclusionReason::from_token(token).map(Some)
            }
        })
}

fn typed(element: &scraper::node::Element, tokens: &[&str]) -> bool {
    element
        .attr("epub:type")
        .into_iter()
        .chain(element.attr("role"))
        .flat_map(str::split_whitespace)
        .any(|token| tokens.contains(&token))
}

fn is_note_body(element: scraper::ElementRef<'_>) -> bool {
    // Without an id there is no stable public note identity: keep uncertain prose.
    if element.value().id().is_none() {
        return false;
    }
    if typed(
        element.value(),
        &[
            "footnote",
            "endnote",
            "rearnote",
            "note",
            "doc-footnote",
            "doc-endnote",
        ],
    ) {
        return true;
    }
    let Some(parent) = element.parent().and_then(scraper::ElementRef::wrap) else {
        return false;
    };
    let container = if matches!(parent.value().name(), "ol" | "ul") {
        if typed(
            parent.value(),
            &["endnotes", "footnotes", "rearnotes", "doc-endnotes"],
        ) {
            return true;
        }
        parent.parent().and_then(scraper::ElementRef::wrap)
    } else {
        Some(parent)
    };
    container.is_some_and(|container| {
        typed(
            container.value(),
            &["endnotes", "footnotes", "rearnotes", "doc-endnotes"],
        )
    })
}

fn is_note_ref(element: scraper::ElementRef<'_>) -> bool {
    if element.value().name() != "a" {
        return false;
    }
    if typed(element.value(), &["noteref", "doc-noteref"]) {
        return true;
    }
    element
        .value()
        .attr("href")
        .is_some_and(|href| href.contains('#'))
        && element.text().collect::<String>().trim().chars().count() <= 4
        && (element
            .ancestors()
            .filter_map(scraper::ElementRef::wrap)
            .any(|e| e.value().name() == "sup")
            || element
                .descendants()
                .filter_map(scraper::ElementRef::wrap)
                .any(|e| e.value().name() == "sup"))
}

/// Only edge links are back-link markers; short links in the prose are preserved.
fn note_text(body: scraper::ElementRef<'_>) -> String {
    let mut parts = Vec::new();
    let mut suppressed = 0;
    for edge in body.traverse() {
        match edge {
            Edge::Open(node) => match node.value() {
                Node::Element(element) => {
                    if suppressed > 0 || hidden(element) {
                        suppressed += 1;
                        continue;
                    }
                    let link = scraper::ElementRef::wrap(node).unwrap();
                    let label = link.text().collect::<String>();
                    if element.name() == "a"
                        && element.attr("href").is_some_and(|href| href.contains('#'))
                        && label.trim().chars().count() <= 4
                    {
                        parts.push((label, true));
                        suppressed += 1;
                    } else if is_block(element.name()) {
                        parts.push((" ".into(), false));
                    }
                }
                Node::Text(text) if suppressed == 0 => parts.push((text.to_string(), false)),
                _ => {}
            },
            Edge::Close(node) => {
                if suppressed > 0 {
                    if node.value().is_element() {
                        suppressed -= 1;
                    }
                } else if node
                    .value()
                    .as_element()
                    .is_some_and(|e| is_block(e.name()))
                {
                    parts.push((" ".into(), false));
                }
            }
        }
    }
    let first = parts.iter().position(|(text, _)| !text.trim().is_empty());
    let last = parts.iter().rposition(|(text, _)| !text.trim().is_empty());
    parts
        .into_iter()
        .enumerate()
        .filter(|(index, (_, backlink))| {
            !backlink || (Some(*index) != first && Some(*index) != last)
        })
        .map(|(_, (text, _))| text)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn heading_level(name: &str) -> Option<u8> {
    match name {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn hidden(element: &scraper::node::Element) -> bool {
    let style = element
        .attr("style")
        .unwrap_or("")
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<String>();
    matches!(
        element.name(),
        "head" | "script" | "style" | "nav" | "template" | "noscript"
    ) || element.attr("hidden").is_some()
        || element.attr("aria-hidden") == Some("true")
        || style.contains("display:none")
        || style.contains("visibility:hidden")
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "h1" | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "p"
            | "li"
            | "pre"
            | "div"
            | "section"
            | "br"
            | "tr"
    )
}
