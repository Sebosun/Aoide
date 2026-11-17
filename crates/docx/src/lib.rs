//! Word files (`.docx`) as Aoide's parsed-book model, pure and deterministic.
//!
//! The body's paragraphs are blocks: a paragraph styled as a heading (by its style's name,
//! `heading N` or `Title`, or its outline level) is a heading, one in a numbered or bulleted
//! list a list item, a table's row one paragraph of its cells. Chapters start at the highest
//! heading level used (the next one down when that level is used only once, as a title).
//! Footnotes are notes; the document's properties give the title, author and language.
//! Deleted text (tracked changes) and text boxes are not read.

use std::collections::HashMap;
use std::io::Read;

use aoide_document::{Block, BlockKind, Chapter, Note, NoteRef, ParsedBook};
use roxmltree::{Document, Node};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a readable Word file: {0}")]
    Invalid(String),
    #[error("the document has no text")]
    Empty,
}

const PATH: &str = "word/document.xml";

/// A footnote marker in a paragraph: where it stands (a character offset), its label, its note.
type Marker = (usize, String, Option<usize>);

/// A Word file as a book.
pub fn parse_docx(bytes: &[u8]) -> Result<ParsedBook, Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let mut part = |name: &str| -> Option<String> {
        let file = zip.by_name(name).ok()?;
        let mut text = String::new();
        // At most 64 MB, as an EPUB's entries.
        file.take(64 << 20).read_to_string(&mut text).ok()?;
        Some(text)
    };
    let document = part(PATH).ok_or_else(|| Error::Invalid("no word/document.xml".into()))?;
    let styles = part("word/styles.xml");
    let footnotes = part("word/footnotes.xml");
    let core = part("docProps/core.xml");

    let levels = match &styles {
        Some(xml) => heading_levels(&parse(xml)?),
        None => HashMap::new(),
    };
    let mut notes: Vec<(String, Note)> = Vec::new();
    if let Some(xml) = &footnotes {
        let doc = parse(xml)?;
        for footnote in doc.descendants().filter(|n| is(n, "footnote")) {
            let (Some(id), None) = (attr(footnote, "id"), attr(footnote, "type")) else {
                continue;
            };
            let text = footnote
                .children()
                .filter(|n| is(n, "p"))
                .map(|p| paragraph_text(p, &[]).0)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            notes.push((
                id.to_owned(),
                Note {
                    id: format!("word/footnotes.xml#{id}"),
                    text,
                },
            ));
        }
    }

    let doc = parse(&document)?;
    let body = doc
        .descendants()
        .find(|n| is(n, "body"))
        .ok_or_else(|| Error::Invalid("no body".into()))?;
    let mut blocks = Vec::new();
    read_blocks(body, &levels, &notes, &mut blocks);
    if blocks.is_empty() {
        return Err(Error::Empty);
    }
    let mut book = ParsedBook::of(chapters(blocks));
    book.notes = notes.into_iter().map(|(_, note)| note).collect();
    if let Some(xml) = &core {
        let doc = parse(xml)?;
        let field = |name: &str| {
            doc.descendants()
                .find(|n| n.tag_name().name() == name)
                .and_then(|n| n.text())
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty())
        };
        book.title = field("title");
        book.authors = field("creator").into_iter().collect();
        book.language = field("language");
        book.description = field("description");
    }
    if book.title.is_none() {
        book.title = book
            .chapters
            .first()
            .and_then(|c| c.blocks.first())
            .filter(|b| matches!(b.kind, BlockKind::Heading { level: 1 }))
            .map(|b| b.text.clone());
    }
    Ok(book)
}

fn parse(xml: &str) -> Result<Document<'_>, Error> {
    Document::parse(xml).map_err(|e| Error::Invalid(e.to_string()))
}

fn is(node: &Node, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|n| is(n, name))
}

/// The heading level each style stands for, by its id (from its name, `heading N` or `Title`).
fn heading_levels(styles: &Document) -> HashMap<String, u8> {
    styles
        .descendants()
        .filter(|n| is(n, "style"))
        .filter_map(|style| {
            let id = attr(style, "styleId")?;
            let name = child(style, "name").and_then(|n| attr(n, "val"))?;
            Some((id.to_owned(), level_of_name(name)?))
        })
        .collect()
}

/// `heading 2` (or the style id `Heading2`) as 2; `Title` as 1.
fn level_of_name(name: &str) -> Option<u8> {
    let lower = name.to_ascii_lowercase().replace(' ', "");
    if lower == "title" {
        return Some(1);
    }
    let n: u8 = lower.strip_prefix("heading")?.parse().ok()?;
    (1..=9).contains(&n).then_some(n.min(6))
}

fn read_blocks(
    node: Node,
    levels: &HashMap<String, u8>,
    notes: &[(String, Note)],
    out: &mut Vec<(Block, Vec<Marker>)>,
) {
    for child_node in node.children().filter(Node::is_element) {
        match child_node.tag_name().name() {
            "p" => {
                let (text, refs) = paragraph_text(child_node, notes);
                if text.is_empty() {
                    continue;
                }
                let properties = child(child_node, "pPr");
                let style = properties
                    .and_then(|p| child(p, "pStyle"))
                    .and_then(|s| attr(s, "val"));
                let outline = properties
                    .and_then(|p| child(p, "outlineLvl"))
                    .and_then(|o| attr(o, "val"))
                    .and_then(|v| v.parse::<u8>().ok())
                    .filter(|&v| v < 9)
                    .map(|v| (v + 1).min(6));
                let level = style
                    .and_then(|s| levels.get(s).copied().or_else(|| level_of_name(s)))
                    .or(outline);
                let listed = properties.and_then(|p| child(p, "numPr")).is_some();
                let kind = match level {
                    Some(level) => BlockKind::Heading { level },
                    None if listed => BlockKind::ListItem,
                    None => BlockKind::Paragraph,
                };
                out.push((Block::of(kind, text), refs));
            }
            "tbl" => {
                for row in child_node.children().filter(|n| is(n, "tr")) {
                    let cells: Vec<String> = row
                        .children()
                        .filter(|n| is(n, "tc"))
                        .map(|cell| {
                            cell.descendants()
                                .filter(|n| is(n, "p"))
                                .map(|p| paragraph_text(p, &[]).0)
                                .filter(|t| !t.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ")
                        })
                        .filter(|t| !t.is_empty())
                        .collect();
                    if !cells.is_empty() {
                        out.push((
                            Block::of(BlockKind::Paragraph, cells.join(", ")),
                            Vec::new(),
                        ));
                    }
                }
            }
            // Content controls and the like hold paragraphs.
            "sdt" | "sdtContent" | "customXml" | "smartTag" => {
                read_blocks(child_node, levels, notes, out)
            }
            _ => {}
        }
    }
}

/// A paragraph's text, whitespace collapsed, and its footnote markers (offset, label, note).
fn paragraph_text(p: Node, notes: &[(String, Note)]) -> (String, Vec<Marker>) {
    let mut text = String::new();
    let mut refs = Vec::new();
    walk(p, notes, &mut text, &mut refs);
    let text = text.trim_end().to_owned();
    let length = text.chars().count();
    for r in &mut refs {
        r.0 = r.0.min(length);
    }
    (text, refs)
}

fn walk(node: Node, notes: &[(String, Note)], text: &mut String, refs: &mut Vec<Marker>) {
    let push = |text: &mut String, s: &str| {
        for c in s.chars() {
            if c.is_whitespace() {
                if !text.is_empty() && !text.ends_with(' ') {
                    text.push(' ');
                }
            } else {
                text.push(c);
            }
        }
    };
    for child_node in node.children() {
        if !child_node.is_element() {
            continue;
        }
        match child_node.tag_name().name() {
            "t" => push(text, child_node.text().unwrap_or("")),
            "tab" | "br" | "cr" => push(text, " "),
            "noBreakHyphen" => push(text, "-"),
            "footnoteReference" => {
                let id = attr(child_node, "id").unwrap_or("");
                let note = notes.iter().position(|(n, _)| n == id);
                let label = note.map_or(id.to_owned(), |n| (n + 1).to_string());
                refs.push((text.chars().count(), label, note));
            }
            // Deleted text, field codes, text boxes and drawings are not the paragraph's text.
            "delText" | "instrText" | "txbxContent" | "drawing" | "pict" | "pPr" | "rPr" => {}
            _ => walk(child_node, notes, text, refs),
        }
    }
}

/// Blocks as chapters, starting at the highest heading level used (or the next one down when
/// that level is used once, as a title, and the next is used).
fn chapters(blocks: Vec<(Block, Vec<Marker>)>) -> Vec<Chapter> {
    let count = |level: u8| {
        blocks
            .iter()
            .filter(|(b, _)| b.kind == BlockKind::Heading { level })
            .count()
    };
    let top = (1..=6).find(|&l| count(l) > 0);
    let level = match top {
        Some(top) if count(top) == 1 && top < 6 && count(top + 1) > 1 => top + 1,
        Some(top) => top,
        None => 0,
    };
    let mut chapters: Vec<(Vec<Block>, Vec<NoteRef>)> = Vec::new();
    for (block, refs) in blocks {
        let starts = matches!(block.kind, BlockKind::Heading { level: l } if l <= level);
        if starts || chapters.is_empty() {
            chapters.push((Vec::new(), Vec::new()));
        }
        let (blocks, chapter_refs) = chapters.last_mut().expect("a chapter was started");
        for (offset, label, note) in refs {
            chapter_refs.push(NoteRef {
                block: blocks.len(),
                offset,
                label,
                note,
            });
        }
        blocks.push(block);
    }
    chapters
        .into_iter()
        .enumerate()
        .map(|(n, (blocks, refs))| {
            let title = blocks
                .iter()
                .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
                .map(|b| b.text.clone())
                .unwrap_or_else(|| format!("Part {}", n + 1));
            let mut chapter = Chapter::of(title, blocks, PATH);
            chapter.note_refs = refs;
            chapter
        })
        .collect()
}
