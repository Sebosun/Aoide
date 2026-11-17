//! FictionBook 2 files (`.fb2`, or one zipped) as Aoide's parsed-book model, pure and
//! deterministic. A main body's top sections are chapters (deeper sections' titles are
//! headings within them); `<body name="notes">` holds the notes that `type="note"` links
//! point to; the cover is a base64 `<binary>`; `<sequence>` is the series.

use std::io::Read;

use aoide_document::{Block, BlockKind, Chapter, Cover, Note, NoteRef, ParsedBook, Series};
use base64::Engine;
use roxmltree::{Document, Node};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a readable FictionBook file: {0}")]
    Invalid(String),
    #[error("the book has no text")]
    Empty,
}

const PATH: &str = "fb2";

/// An FB2 file (or a zip holding one) as a book.
pub fn parse_fb2(bytes: &[u8]) -> Result<ParsedBook, Error> {
    let xml = if bytes.starts_with(b"PK\x03\x04") {
        unzipped(bytes)?
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let document = Document::parse(&xml).map_err(|e| Error::Invalid(e.to_string()))?;
    let root = document.root_element();
    let info = child(root, "description").and_then(|d| child(d, "title-info"));
    let bodies: Vec<Node> = children(root, "body").collect();
    let notes_body = bodies.iter().find(|b| b.attribute("name") == Some("notes"));
    let notes: Vec<(String, Note)> = notes_body
        .map(|body| {
            body.descendants()
                .filter(|n| n.has_tag_name("section"))
                .filter_map(|section| {
                    let id = section.attribute("id")?;
                    let text = children(section, "p")
                        .map(inline_text)
                        .collect::<Vec<_>>()
                        .join(" ");
                    Some((
                        id.to_owned(),
                        Note {
                            id: format!("{PATH}#{id}"),
                            text,
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut reader = Reader {
        notes: &notes,
        chapters: Vec::new(),
    };
    for body in bodies.iter().filter(|b| b.attribute("name").is_none()) {
        let sections: Vec<Node> = children(*body, "section").collect();
        if sections.is_empty() {
            reader.chapter(*body, None);
        }
        for section in sections {
            reader.chapter(section, Some(section));
        }
    }
    let chapters = reader.chapters;
    if chapters.is_empty() {
        return Err(Error::Empty);
    }
    let mut book = ParsedBook::of(chapters);
    book.notes = notes.into_iter().map(|(_, note)| note).collect();
    if let Some(info) = info {
        book.title = child(info, "book-title")
            .map(inline_text)
            .filter(|t| !t.is_empty());
        book.authors = children(info, "author")
            .map(author)
            .filter(|a| !a.is_empty())
            .collect();
        book.language = child(info, "lang")
            .map(inline_text)
            .filter(|l| !l.is_empty());
        book.description = child(info, "annotation")
            .map(|a| {
                a.descendants()
                    .filter(|n| n.has_tag_name("p"))
                    .map(inline_text)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|d| !d.is_empty());
        book.series = child(info, "sequence").and_then(|s| {
            let name = s.attribute("name")?.trim();
            (!name.is_empty()).then(|| Series {
                name: name.to_owned(),
                position: s
                    .attribute("number")
                    .and_then(|n| n.trim().parse::<f32>().ok())
                    .filter(|n| n.is_finite()),
            })
        });
        book.cover = child(info, "coverpage")
            .and_then(|c| child(c, "image"))
            .and_then(href)
            .and_then(|id| binary(root, id.trim_start_matches('#')));
    }
    Ok(book)
}

/// The `.fb2` file in a zip.
fn unzipped(bytes: &[u8]) -> Result<String, Error> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let name = zip
        .file_names()
        .find(|n| n.to_ascii_lowercase().ends_with(".fb2"))
        .map(str::to_owned)
        .ok_or_else(|| Error::Invalid("no .fb2 file in the zip".into()))?;
    let file = zip
        .by_name(&name)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let mut bytes = Vec::new();
    // At most 64 MB, as an EPUB's entries.
    file.take(64 << 20)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn child<'a, 'i: 'a>(node: Node<'a, 'i>, name: &'a str) -> Option<Node<'a, 'i>> {
    children(node, name).next()
}

fn children<'a, 'i: 'a>(
    node: Node<'a, 'i>,
    name: &'a str,
) -> impl Iterator<Item = Node<'a, 'i>> + 'a {
    node.children().filter(move |n| n.tag_name().name() == name)
}

fn href<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == "href")
        .map(|a| a.value())
}

fn author(node: Node) -> String {
    let names: Vec<String> = ["first-name", "middle-name", "last-name"]
        .iter()
        .filter_map(|n| child(node, n).map(inline_text))
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        child(node, "nickname").map(inline_text).unwrap_or_default()
    } else {
        names.join(" ")
    }
}

fn binary(root: Node, id: &str) -> Option<Cover> {
    let node = children(root, "binary").find(|b| b.attribute("id") == Some(id))?;
    let data: String = node.text().unwrap_or("").split_whitespace().collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    Some(Cover {
        media_type: node
            .attribute("content-type")
            .unwrap_or("image/jpeg")
            .to_owned(),
        bytes,
    })
}

/// An element's text, whitespace collapsed.
fn inline_text(node: Node) -> String {
    node.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

struct Reader<'n> {
    notes: &'n [(String, Note)],
    chapters: Vec<Chapter>,
}

/// A chapter being read: its blocks and note markers, and whether a scene break waits.
struct Building {
    blocks: Vec<Block>,
    refs: Vec<NoteRef>,
    after_break: bool,
}

impl Reader<'_> {
    fn chapter(&mut self, node: Node, section: Option<Node>) {
        let mut building = Building {
            blocks: Vec::new(),
            refs: Vec::new(),
            after_break: false,
        };
        self.read(node, 1, &mut building);
        if building.blocks.iter().all(|b| b.text.is_empty()) {
            return;
        }
        let title = section
            .and_then(|s| child(s, "title"))
            .map(paragraphs_text)
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format!("Chapter {}", self.chapters.len() + 1));
        let mut chapter = Chapter::of(title, building.blocks, PATH);
        chapter.note_refs = building.refs;
        self.chapters.push(chapter);
    }

    /// The blocks of a section (or a body without sections), at heading `level`.
    fn read(&self, node: Node, level: u8, into: &mut Building) {
        for child in node.children().filter(Node::is_element) {
            match child.tag_name().name() {
                "title" => self.push(into, BlockKind::Heading { level }, child, true),
                "subtitle" => self.push(
                    into,
                    BlockKind::Heading {
                        level: (level + 1).min(6),
                    },
                    child,
                    false,
                ),
                "p" | "v" | "text-author" => self.push(into, BlockKind::Paragraph, child, false),
                "empty-line" => into.after_break = !into.blocks.is_empty(),
                "section" => self.read(child, (level + 1).min(6), into),
                "epigraph" | "cite" | "poem" | "stanza" | "annotation" => {
                    self.read(child, level, into)
                }
                "table" => {
                    for row in child.children().filter(|n| n.has_tag_name("tr")) {
                        self.push(into, BlockKind::Paragraph, row, false);
                    }
                }
                _ => {}
            }
        }
    }

    /// A block of `node`'s text (a title's paragraphs joined), its note links taken out.
    fn push(&self, into: &mut Building, kind: BlockKind, node: Node, title: bool) {
        let mut text = String::new();
        let mut refs = Vec::new();
        let parts: Vec<Node> = if title {
            node.children().filter(|n| n.has_tag_name("p")).collect()
        } else {
            vec![node]
        };
        for (n, part) in parts.into_iter().enumerate() {
            if n > 0 {
                text.push(' ');
            }
            self.walk(part, &mut text, &mut refs);
        }
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            return;
        }
        for (at, label, note) in refs {
            let before: String = text.chars().take(at).collect();
            let offset = before
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .count()
                + usize::from(!before.trim().is_empty() && before.ends_with(char::is_whitespace));
            into.refs.push(NoteRef {
                block: into.blocks.len(),
                offset: offset.min(collapsed.chars().count()),
                label,
                note,
            });
        }
        let mut block = Block::of(kind, collapsed);
        block.break_before = std::mem::take(&mut into.after_break);
        into.blocks.push(block);
    }

    /// `node`'s text into `text`; a note link is not text: where it stood, its label, its note.
    fn walk(&self, node: Node, text: &mut String, refs: &mut Vec<(usize, String, Option<usize>)>) {
        for child in node.children() {
            if child.is_text() {
                text.push_str(child.text().unwrap_or(""));
            } else if child.has_tag_name("a") && child.attribute("type") == Some("note") {
                let target = href(child).unwrap_or("").trim_start_matches('#');
                let note = self.notes.iter().position(|(id, _)| id == target);
                refs.push((text.chars().count(), inline_text(child), note));
            } else if child.is_element() {
                self.walk(child, text, refs);
            }
        }
    }
}

/// A title's paragraphs as one line.
fn paragraphs_text(title: Node) -> String {
    title
        .children()
        .filter(|n| n.has_tag_name("p"))
        .map(inline_text)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
