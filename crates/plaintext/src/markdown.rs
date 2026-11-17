//! Markdown: headings are chapters (level 1, or level 2 under a single level 1), inline markup
//! becomes its text, fenced and indented code is kept as written, a rule is a scene break,
//! footnotes (`[^1]`) are notes, and front matter gives the title and author. The common
//! subset only: what it does not know stays as text.

use std::collections::HashMap;

use aoide_document::{Block, BlockKind, Chapter, Note, NoteRef, ParsedBook, Warning};

use crate::{decode, Error};

const PATH: &str = "markdown";

/// A Markdown file as a book.
pub fn parse_markdown(bytes: &[u8]) -> Result<ParsedBook, Error> {
    let (text, warning) = decode(bytes);
    let (meta, body) = front_matter(&text);
    let read = blocks(body);
    if read.blocks.is_empty() {
        return Err(Error::Empty);
    }
    let mut warnings: Vec<Warning> = warning.into_iter().collect();
    // Notes by label, in the order they are defined.
    let index: HashMap<&str, usize> = read
        .notes
        .iter()
        .enumerate()
        .map(|(i, (label, _))| (label.as_str(), i))
        .collect();
    let notes: Vec<Note> = read
        .notes
        .iter()
        .map(|(label, text)| Note {
            id: format!("{PATH}#{label}"),
            text: text.clone(),
        })
        .collect();
    let mut referenced = vec![false; notes.len()];
    let top = |level: u8| {
        read.blocks
            .iter()
            .filter(|(b, _)| b.kind == BlockKind::Heading { level })
            .count()
    };
    // One level 1 is the title; several are the chapters.
    let level = match (top(1), top(2)) {
        (0 | 1, 1..) => 2,
        _ => 1,
    };
    let mut chapters: Vec<(Vec<Block>, Vec<NoteRef>)> = Vec::new();
    for (block, refs) in read.blocks {
        let starts = matches!(block.kind, BlockKind::Heading { level: l } if l <= level);
        if starts || chapters.is_empty() {
            chapters.push((Vec::new(), Vec::new()));
        }
        let (blocks, chapter_refs) = chapters.last_mut().unwrap();
        for (offset, label) in refs {
            let note = index.get(label.as_str()).copied();
            match note {
                Some(n) => referenced[n] = true,
                None => warnings.push(Warning::UnresolvedNoteRef {
                    path: PATH.into(),
                    target: label.clone(),
                }),
            }
            chapter_refs.push(NoteRef {
                block: blocks.len(),
                offset,
                label,
                note,
            });
        }
        blocks.push(block);
    }
    for (note, referenced) in notes.iter().zip(referenced) {
        if !referenced {
            warnings.push(Warning::UnreferencedNote {
                id: note.id.clone(),
            });
        }
    }
    let first_heading = chapters
        .iter()
        .flat_map(|(blocks, _)| blocks)
        .find(|b| matches!(b.kind, BlockKind::Heading { level: 1 }))
        .map(|b| b.text.clone());
    let title = meta.get("title").cloned().or(first_heading);
    let chapters: Vec<Chapter> = chapters
        .into_iter()
        .map(|(blocks, refs)| {
            let name = blocks
                .iter()
                .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
                .map(|b| b.text.clone())
                .or_else(|| title.clone())
                .unwrap_or_else(|| "Text".into());
            let mut chapter = Chapter::of(name, blocks, PATH);
            chapter.note_refs = refs;
            chapter
        })
        .collect();
    let mut book = ParsedBook::of(chapters);
    book.title = title;
    book.authors = meta.get("author").cloned().into_iter().collect();
    book.language = meta.get("lang").or(meta.get("language")).cloned();
    book.description = meta.get("description").cloned();
    book.notes = notes;
    book.warnings = warnings;
    Ok(book)
}

/// `key: value` lines between `---` lines at the top, and the text after them.
fn front_matter(text: &str) -> (HashMap<String, String>, &str) {
    let mut meta = HashMap::new();
    let Some(rest) = text.strip_prefix("---\n") else {
        return (meta, text);
    };
    let Some(end) = rest.find("\n---") else {
        return (meta, text);
    };
    for line in rest[..end].lines() {
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim().trim_matches(['"', '\'']).trim();
            if !value.is_empty() {
                meta.insert(key.trim().to_ascii_lowercase(), value.to_owned());
            }
        }
    }
    let after = &rest[end + 4..];
    (meta, after.split_once('\n').map_or("", |(_, body)| body))
}

/// The blocks read, each with its footnote markers (offset, label); and the notes.
struct Read {
    blocks: Vec<(Block, Vec<(usize, String)>)>,
    notes: Vec<(String, String)>,
}

fn blocks(text: &str) -> Read {
    let lines: Vec<&str> = text.lines().collect();
    let mut read = Read {
        blocks: Vec::new(),
        notes: Vec::new(),
    };
    let mut paragraph: Vec<&str> = Vec::new();
    let mut kind = BlockKind::Paragraph;
    let mut after_break = false;
    let flush =
        |read: &mut Read, lines: &mut Vec<&str>, kind: BlockKind, after_break: &mut bool| {
            if lines.is_empty() {
                return;
            }
            let (text, refs) = inline(&lines.join(" "));
            lines.clear();
            if text.is_empty() {
                return;
            }
            let mut block = Block::of(kind, text);
            block.break_before = std::mem::take(after_break);
            read.blocks.push((block, refs));
        };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        i += 1;
        if trimmed.is_empty() {
            flush(&mut read, &mut paragraph, kind, &mut after_break);
            kind = BlockKind::Paragraph;
            continue;
        }
        if let Some(fence) = ["```", "~~~"].into_iter().find(|f| trimmed.starts_with(f)) {
            flush(&mut read, &mut paragraph, kind, &mut after_break);
            let mut code = Vec::new();
            while i < lines.len() && !lines[i].trim().starts_with(fence) {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            push_code(&mut read, &code, &mut after_break);
            continue;
        }
        if paragraph.is_empty() && (line.starts_with("    ") || line.starts_with('\t')) {
            let mut code = vec![line];
            while i < lines.len()
                && (lines[i].starts_with("    ")
                    || lines[i].starts_with('\t')
                    || lines[i].trim().is_empty())
            {
                code.push(lines[i]);
                i += 1;
            }
            let code: Vec<&str> = code
                .iter()
                .map(|l| {
                    l.strip_prefix("    ")
                        .or(l.strip_prefix('\t'))
                        .unwrap_or(l.trim_start())
                })
                .collect();
            push_code(&mut read, &code, &mut after_break);
            continue;
        }
        let hashes = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut read, &mut paragraph, kind, &mut after_break);
            let title = trimmed[hashes..].trim().trim_end_matches('#').trim();
            let mut heading = vec![title];
            flush(
                &mut read,
                &mut heading,
                BlockKind::Heading {
                    level: hashes as u8,
                },
                &mut after_break,
            );
            continue;
        }
        let rule = trimmed.len() >= 3
            && ['-', '*', '_']
                .iter()
                .any(|c| trimmed.chars().all(|x| x == *c || x == ' ') && trimmed.starts_with(*c));
        if rule && paragraph.is_empty() {
            after_break = true;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("[^") {
            if let Some((label, text)) = rest.split_once("]:") {
                flush(&mut read, &mut paragraph, kind, &mut after_break);
                let mut body = vec![text.trim().to_owned()];
                while i < lines.len() && lines[i].starts_with("    ") {
                    body.push(lines[i].trim().to_owned());
                    i += 1;
                }
                let (text, _) = inline(&body.join(" "));
                read.notes.push((label.to_owned(), text));
                continue;
            }
        }
        if let Some(item) = list_item(trimmed) {
            flush(&mut read, &mut paragraph, kind, &mut after_break);
            kind = BlockKind::ListItem;
            paragraph.push(item);
            continue;
        }
        let quoted = trimmed.trim_start_matches('>').trim_start();
        paragraph.push(if trimmed.starts_with('>') {
            quoted
        } else {
            trimmed
        });
    }
    flush(&mut read, &mut paragraph, kind, &mut after_break);
    read
}

fn push_code(read: &mut Read, lines: &[&str], after_break: &mut bool) {
    let code = lines.join("\n").trim_end().to_owned();
    if code.trim().is_empty() {
        return;
    }
    let mut block = Block::of(BlockKind::Preformatted, code);
    block.break_before = std::mem::take(after_break);
    read.blocks.push((block, Vec::new()));
}

/// The text of a list item's first line (`- `, `* `, `+ `, `1. `, `1) `).
fn list_item(line: &str) -> Option<&str> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return Some(rest);
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    let rest = &line[digits..];
    (digits > 0)
        .then(|| rest.strip_prefix(". ").or(rest.strip_prefix(") ")))
        .flatten()
}

/// A line of Markdown as its text, whitespace collapsed, and where its footnote markers
/// stood (character offset in that text, label).
fn inline(source: &str) -> (String, Vec<(usize, String)>) {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut refs = Vec::new();
    let mut i = 0;
    let boundary =
        |c: Option<&char>| c.is_none_or(|c| c.is_whitespace() || c.is_ascii_punctuation());
    while i < chars.len() {
        let c = chars[i];
        let rest: String = chars[i..].iter().take(2).collect();
        if c == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if rest == "[^" {
            if let Some(end) = chars[i..].iter().position(|&c| c == ']') {
                let label: String = chars[i + 2..i + end].iter().collect();
                let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
                refs.push((collapsed.chars().count(), label));
                i += end + 1;
                continue;
            }
        }
        if c == '!' && chars.get(i + 1) == Some(&'[') {
            if let Some((_, after)) = link(&chars, i + 1) {
                i = after;
                continue;
            }
        }
        if c == '[' {
            if let Some((text, after)) = link(&chars, i) {
                out.push_str(&inline(&text).0);
                i = after;
                continue;
            }
        }
        if c == '`' {
            let run = chars[i..].iter().take_while(|&&x| x == '`').count();
            let close = (i + run..chars.len()).find(|&j| {
                chars[j..].iter().take(run).all(|&x| x == '`') && j + run <= chars.len()
            });
            if let Some(close) = close {
                out.extend(&chars[i + run..close]);
                i = close + run;
                continue;
            }
        }
        if c == '<' {
            if let Some(end) = chars[i..].iter().position(|&c| c == '>') {
                let tag: String = chars[i + 1..i + end].iter().collect();
                if tag.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/')
                    && !tag.contains(' ')
                    || tag.contains('=')
                {
                    i += end + 1;
                    continue;
                }
            }
        }
        if matches!(c, '*' | '_' | '~') {
            let run = chars[i..].iter().take_while(|&&x| x == c).count();
            let (before, after) = (
                if i == 0 { None } else { chars.get(i - 1) },
                chars.get(i + run),
            );
            let opens = boundary(before) && after.is_some_and(|a| !a.is_whitespace());
            let closes = before.is_some_and(|b| !b.is_whitespace()) && boundary(after);
            let inside_word = !boundary(before) && !boundary(after);
            let delimiter =
                (opens || closes || (c == '*' && inside_word)) && (c != '~' || run == 2);
            if delimiter && !(c == '_' && inside_word) {
                i += run;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    (out.split_whitespace().collect::<Vec<_>>().join(" "), refs)
}

/// A link at `chars[at] == '['`: its text and where it ends (`[text](address)` or
/// `[text][ref]`).
fn link(chars: &[char], at: usize) -> Option<(String, usize)> {
    let mut depth = 0;
    let close = (at..chars.len()).find(|&j| {
        match chars[j] {
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
        depth == 0
    })?;
    let text: String = chars[at + 1..close].iter().collect();
    let end = match chars.get(close + 1) {
        Some('(') => ')',
        Some('[') => ']',
        _ => return None,
    };
    let after = (close + 2..chars.len()).find(|&j| chars[j] == end)?;
    Some((text, after + 1))
}
