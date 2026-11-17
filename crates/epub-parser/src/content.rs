use crate::{Block, BlockKind, ExclusionReason, Note, NoteRef};
use ego_tree::iter::Edge;
use scraper::{Html, Node};
use std::collections::{HashMap, HashSet};

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
    /// Empty tokens that mark a scene break (see [`is_scene_break`]).
    breaks: HashSet<usize>,
    /// Empty tokens where a picture stands: its `src` (as written), `alt`, and its figure.
    pictures: HashMap<usize, (String, String, Option<usize>)>,
    /// Where each figure's caption begins (a token), by figure.
    captions: HashMap<usize, usize>,
    /// Empty tokens where a table's parts open and close.
    table_marks: HashMap<usize, TableMark>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableMark {
    Table,
    TableEnd,
    Caption,
    Row,
    RowEnd,
    /// A cell opens; `true` for a heading cell (`th`).
    Cell(bool),
    CellEnd,
}

/// A table being laid out.
#[derive(Default)]
struct TableBuild {
    caption_block: Option<usize>,
    /// Each row: its block, and its cells (start, end, a heading cell).
    rows: Vec<(Option<usize>, Vec<(usize, usize, bool)>)>,
    /// A cell's start: its block and offset, and whether it is a heading.
    open_cell: Option<(usize, usize, bool)>,
    /// Not a grid of lines (a cell over several lines, a table inside it).
    broken: bool,
}

impl TableBuild {
    fn shape(self, count: usize) -> Option<crate::TableShape> {
        let rows: Vec<_> = self
            .rows
            .into_iter()
            .filter(|(_, cells)| !cells.is_empty())
            .collect();
        let first = rows.first()?.0?;
        let consecutive = rows
            .iter()
            .enumerate()
            .all(|(i, (block, _))| *block == Some(first + i));
        if self.broken || !consecutive || first + rows.len() > count {
            return None;
        }
        let header_rows = rows
            .iter()
            .take_while(|(_, cells)| cells.iter().all(|(_, _, heading)| *heading))
            .count();
        Some(crate::TableShape {
            rows: first..first + rows.len(),
            caption_block: self.caption_block.filter(|&b| b + 1 == first),
            header_rows,
            cells: rows
                .into_iter()
                .map(|(_, cells)| {
                    cells
                        .into_iter()
                        .map(|(start, end, _)| start..end)
                        .collect()
                })
                .collect(),
        })
    }
}

/// A stretch of a document laid out: its blocks, and what stands at places in them.
pub(crate) struct Laid {
    pub blocks: Vec<Block>,
    pub refs: Vec<(NoteRef, String)>,
    /// Pictures: the block they stand before, `src` as written, `alt`, the caption's block.
    pub pictures: Vec<(usize, String, String, Option<usize>)>,
    pub tables: Vec<crate::TableShape>,
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
        self.lay_out(start, end).blocks
    }

    pub fn lay_out(&self, start: usize, end: usize) -> Laid {
        let mut pictures = Vec::new();
        // The block each caption begins in, by figure.
        let mut caption_blocks: HashMap<usize, usize> = HashMap::new();
        let mut tables = Vec::new();
        let mut table: Option<TableBuild> = None;
        let mut blocks = Vec::new();
        let mut refs = Vec::new();
        let mut pending = Vec::new();
        let mut line = String::new();
        let mut kind = None;
        let mut after_break = false;
        let finish = |blocks: &mut Vec<Block>,
                      refs: &mut Vec<(NoteRef, String)>,
                      line: &mut String,
                      kind: &mut Option<BlockKind>,
                      pending: &mut Vec<(usize, String, String)>,
                      after_break: &mut bool| {
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
                blocks.push(Block {
                    kind,
                    text,
                    break_before: std::mem::take(after_break),
                });
            }
            pending.clear();
            line.clear();
        };
        for index in start..end {
            let token = &self.tokens[index];
            if token == "\n" {
                finish(
                    &mut blocks,
                    &mut refs,
                    &mut line,
                    &mut kind,
                    &mut pending,
                    &mut after_break,
                );
                continue;
            }
            if self.breaks.contains(&index) {
                // A break counts between lines, not inside one that has begun.
                after_break |= line.trim().is_empty();
                continue;
            }
            if let Some((src, alt, figure)) = self.pictures.get(&index) {
                // Inside a line that has begun, it stands after the line, which is not cut.
                let before = blocks.len() + usize::from(!line.trim().is_empty());
                pictures.push((before, src.clone(), alt.clone(), *figure));
                continue;
            }
            if let Some(mark) = self.table_marks.get(&index) {
                // The line being built is block `blocks.len()`; a cell's place in it is the
                // length of its text so far, as it will be collapsed.
                let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
                let at = collapsed.chars().count();
                let block = blocks.len();
                match (*mark, table.as_mut()) {
                    (TableMark::Table, None) => table = Some(TableBuild::default()),
                    (TableMark::Table, Some(outer)) => outer.broken = true,
                    (TableMark::TableEnd, Some(_)) => {
                        let built = table.take().unwrap();
                        // Its last row is a line still being built.
                        let count = blocks.len() + usize::from(!line.trim().is_empty());
                        tables.extend(built.shape(count));
                    }
                    (TableMark::Caption, Some(t)) => t.caption_block = Some(block),
                    (TableMark::Row, Some(t)) => t.rows.push((None, Vec::new())),
                    (TableMark::Cell(heading), Some(t)) => {
                        let start = at
                            + usize::from(
                                !collapsed.is_empty() && line.ends_with(char::is_whitespace),
                            );
                        t.open_cell = Some((block, start, heading));
                    }
                    (TableMark::CellEnd, Some(t)) => {
                        match (t.open_cell.take(), t.rows.last_mut()) {
                            (Some((opened, start, heading)), Some(row))
                                if opened == block && row.0.is_none_or(|b| b == block) =>
                            {
                                row.0 = Some(block);
                                row.1.push((start, at.max(start), heading));
                            }
                            _ => t.broken = true,
                        }
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(figure) = self
                .captions
                .iter()
                .find(|(_, &at)| at == index)
                .map(|(f, _)| *f)
            {
                // The line being built is the next block.
                caption_blocks.insert(figure, blocks.len());
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
        finish(
            &mut blocks,
            &mut refs,
            &mut line,
            &mut kind,
            &mut pending,
            &mut after_break,
        );
        let count = blocks.len();
        Laid {
            pictures: pictures
                .into_iter()
                .map(|(before, src, alt, figure)| {
                    let caption = figure
                        .and_then(|f| caption_blocks.get(&f).copied())
                        .filter(|&b| b < count);
                    (before.min(count), src, alt, caption)
                })
                .collect(),
            blocks,
            refs,
            tables,
        }
    }
}

pub(crate) fn join_blocks(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Ids of the note bodies in a document (as [`read`] would detach them), for recognising links
/// to them from other documents.
pub(crate) fn note_ids(source: &str) -> Vec<String> {
    let html = Html::parse_document(&crate::xhtml::expand_self_closing(source));
    html.root_element()
        .descendants()
        .filter_map(scraper::ElementRef::wrap)
        .filter(|element| is_note_body(*element))
        .filter_map(|element| element.value().id().map(str::to_owned))
        .collect()
}

/// `links_to_note` says whether an href (relative to this document) points at a note body,
/// here or in another document.
pub(crate) fn read(source: &str, links_to_note: &dyn Fn(&str) -> bool) -> Content {
    let html = Html::parse_document(&crate::xhtml::expand_self_closing(source));
    let mut content = Content {
        title: "Untitled chapter".into(),
        tokens: Vec::new(),
        kinds: Vec::new(),
        anchors: HashMap::new(),
        headings: Vec::new(),
        sections: Vec::new(),
        notes: Vec::new(),
        refs: HashMap::new(),
        breaks: HashSet::new(),
        pictures: HashMap::new(),
        captions: HashMap::new(),
        table_marks: HashMap::new(),
    };
    // Figures open around the element being read, innermost last; and how many there were.
    let mut figures: Vec<(ego_tree::NodeId, usize)> = Vec::new();
    let mut figure_count = 0;
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
                    if is_note_ref(current, links_to_note) {
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
                    let mark = match element.name() {
                        "table" => Some(TableMark::Table),
                        "caption" => Some(TableMark::Caption),
                        "tr" => Some(TableMark::Row),
                        "td" => Some(TableMark::Cell(false)),
                        "th" => Some(TableMark::Cell(true)),
                        _ => None,
                    };
                    if let Some(mark) = mark {
                        content.table_marks.insert(content.tokens.len(), mark);
                        content.push("", &stack);
                    }
                    if element.name() == "figure" {
                        figures.push((node.id(), figure_count));
                        figure_count += 1;
                    }
                    if element.name() == "figcaption" {
                        let has_text = current.text().any(|t| !t.trim().is_empty());
                        if let Some((_, figure)) = figures.last().filter(|_| has_text) {
                            content
                                .captions
                                .entry(*figure)
                                .or_insert(content.tokens.len());
                        }
                    }
                    if let Some(src) = picture_source(element) {
                        let alt = element
                            .attr("alt")
                            .unwrap_or("")
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" ");
                        let figure = figures.last().map(|(_, f)| *f);
                        content
                            .pictures
                            .insert(content.tokens.len(), (src.to_owned(), alt, figure));
                        content.push("", &stack);
                    }
                    if is_scene_break(current) {
                        // A marker, not a line break: one inside running text splits nothing.
                        content.breaks.insert(content.tokens.len());
                        content.push("", &stack);
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
                let closing = node.value().as_element().and_then(|e| match e.name() {
                    "table" => Some(TableMark::TableEnd),
                    "tr" => Some(TableMark::RowEnd),
                    "td" | "th" => Some(TableMark::CellEnd),
                    _ => None,
                });
                if let Some(mark) = closing {
                    content.table_marks.insert(content.tokens.len(), mark);
                    content.push("", &stack);
                }
                if figures.last().is_some_and(|(id, _)| *id == node.id()) {
                    figures.pop();
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
                crate::model::exclusion_from_token(token).map(Some)
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

/// A marker is typed as a note reference, or is a short link (a number or symbol) that is
/// superscripted or points at a note body. Longer links ("see the note on gates") stay prose.
fn is_note_ref(element: scraper::ElementRef<'_>, links_to_note: &dyn Fn(&str) -> bool) -> bool {
    if element.value().name() != "a" {
        return false;
    }
    if typed(element.value(), &["noteref", "doc-noteref"]) {
        return true;
    }
    let Some(href) = element
        .value()
        .attr("href")
        .filter(|href| href.contains('#'))
    else {
        return false;
    };
    element.text().collect::<String>().trim().chars().count() <= 4
        && (links_to_note(href)
            || element
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

/// A rule, or an element with nothing to read whose class or type names a scene break. A page
/// marker (`epub:type="pagebreak"`) is not one.
fn is_scene_break(element: scraper::ElementRef<'_>) -> bool {
    let value = element.value();
    value.name() == "hr"
        || (value
            .attr("class")
            .into_iter()
            .chain(value.attr("epub:type"))
            .flat_map(str::split_whitespace)
            .any(aoide_document::names_scene_break)
            && element.text().all(|text| text.trim().is_empty()))
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

/// The `src` of a picture to show: `<img>` or SVG's `<image>`, unless marked as decoration.
fn picture_source(element: &scraper::node::Element) -> Option<&str> {
    let src = match element.name() {
        "img" => element.attr("src"),
        // `xlink:href` is in the xlink namespace, which `attr` does not look in.
        "image" => element
            .attrs
            .iter()
            .find(|(name, _)| &*name.local == "href")
            .map(|(_, value)| &**value),
        _ => None,
    }?;
    let decoration =
        element.attr("role") == Some("presentation") || element.attr("aria-hidden") == Some("true");
    (!decoration && !src.trim().is_empty()).then_some(src.trim())
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
