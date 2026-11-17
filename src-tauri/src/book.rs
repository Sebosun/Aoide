//! A parsed, segmented book and the views the UI receives.

use aoide_epub::{BlockKind, ParsedBook, Warning};
use aoide_text::{
    estimate_word_timings, segment_at, segment_chapter_with_cuts, Segment, SegmentOptions,
};
use serde::Serialize;

pub struct ChapterModel {
    pub title: String,
    /// The chapter's display text (its blocks joined by blank lines); segment ranges index it.
    pub text: String,
    /// Note markers per segment index.
    pub notes: Vec<Vec<NoteMarker>>,
    pub segments: Vec<Segment>,
    pub block_kinds: Vec<BlockKind>,
    /// Global playlist index of the chapter's first segment.
    pub first_entry: usize,
    /// Pictures shown between blocks, outside the text.
    pub inserts: Vec<aoide_document::Insert>,
    /// Tables whose rows are blocks of the text.
    pub tables: Vec<aoide_document::TableShape>,
    /// Where each block starts in `text`, in characters.
    pub block_starts: Vec<usize>,
    /// Each code block's layout (see [`BlockView::layout`]).
    pub layouts: Vec<Option<String>>,
}

pub struct BookModel {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    /// EPUB description with markup removed.
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub isbn: Option<String>,
    pub chapters: Vec<ChapterModel>,
    pub warnings: Vec<String>,
    pub excluded: Vec<ExcludedView>,
    total_entries: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct NoteMarker {
    /// Index of the word the marker follows (0 when it precedes the first word).
    pub after_word: usize,
    pub label: String,
    /// `None` when the note body was not found.
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SegmentView {
    pub id: String,
    pub words: Vec<String>,
    /// Where each word starts in the chapter's display text (Unicode scalar values, as
    /// `Segment::words`): the page rejoins words with single spaces, so a selection on it is
    /// counted from these. Empty for a spoken note, which is not in the text.
    pub starts: Vec<usize>,
    pub speakable: bool,
    /// The speech engine says it, so it can be said again (see `Core::say_again`): not a
    /// silent entry, nor anything of a book its recording narrates.
    pub can_say_again: bool,
    /// A spoken footnote inserted after its paragraph (not part of the book's text flow).
    pub note: bool,
    /// Said, not part of the text, and not drawn on the page: what narration says for a
    /// picture, a table or code (see [`Describe`]).
    pub aside: bool,
    pub notes: Vec<NoteMarker>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BlockView {
    pub kind: &'static str,
    pub level: Option<u8>,
    pub segments: Vec<SegmentView>,
    /// Code as written (line breaks, indentation): the same words as its segments.
    pub layout: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterSummary {
    pub index: usize,
    pub title: String,
    /// An audiobook's part: how long it plays.
    pub duration_ms: Option<u64>,
    /// Left out of the narration: playback passes over it, preparation and export leave it
    /// out. It is still there to read.
    pub left_out: bool,
    /// Its title says it is front or back matter: offered to be left out.
    pub suggested: bool,
    /// Words read aloud in it (none for an audiobook's part).
    pub word_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterView {
    pub index: usize,
    pub title: String,
    pub blocks: Vec<BlockView>,
    /// When the book's recording narrates it: the spoken sentences it has no audio for, which
    /// playback skips. `None` while a speech engine narrates.
    pub not_narrated: Option<Vec<String>>,
    /// Pictures between the blocks, in order; block numbers index `blocks`.
    pub inserts: Vec<InsertView>,
    /// Tables whose rows are `blocks`, drawn as tables.
    pub tables: Vec<TableView>,
}

/// A table in the text: rows `first_block..first_block + rows` of `blocks`, each sentence in
/// the cell its first word is in.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct TableView {
    pub first_block: usize,
    pub rows: usize,
    pub header_rows: usize,
    /// The block that is its caption.
    pub caption_block: Option<usize>,
    /// Each row's cells, as `[start, end)` in the chapter's text (as words' `starts`).
    pub cells: Vec<Vec<[usize; 2]>>,
}

/// Something drawn between a chapter's blocks.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum InsertView {
    Picture {
        /// It stands before this block (`blocks.length`: after the last).
        before_block: usize,
        /// The picture's file.
        path: String,
        /// Its description; empty when it has none.
        alt: String,
        /// The block that is its caption (already read as text).
        caption_block: Option<usize>,
        /// A caption of its own, outside the text.
        caption: Option<String>,
    },
    /// A table outside the text (an article's): drawn, not read.
    Table {
        before_block: usize,
        /// Each row's cells' texts.
        rows: Vec<Vec<String>>,
        header_rows: usize,
        caption: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ExcludedView {
    pub title: String,
    pub reason: String,
}

/// An EPUB or a PDF (told apart by its content), parsed.
pub fn parse_document(bytes: &[u8]) -> Result<ParsedBook, String> {
    if aoide_pdf::is_pdf(bytes) {
        aoide_pdf::parse_pdf(bytes).map_err(|e| e.to_string())
    } else {
        aoide_epub::parse_epub(bytes, &aoide_epub::ParseOptions::default())
            .map_err(|e| e.to_string())
    }
}

/// How a book's text is laid out for listening, beyond the text itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout {
    /// A spoken segment for each referenced note, after its paragraph.
    pub read_notes: bool,
    /// How sentences are split, and the pauses after them.
    pub segments: SegmentOptions,
    /// What is said for pictures, tables and code.
    pub describe: Describe,
}

/// What narration says for pictures, tables and code (Settings › Listening).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct Describe {
    pub pictures: PictureSpeech,
    pub tables: TableSpeech,
    pub code: CodeSpeech,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum PictureSpeech {
    /// Nothing beyond a caption that is part of the text.
    Off,
    /// A caption outside the text too (an article's).
    #[default]
    Caption,
    /// Its description ("Picture: …") as well.
    AltAndCaption,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum TableSpeech {
    /// "A table of 4 columns and 12 rows", not its cells.
    #[default]
    Announce,
    /// Row by row, each cell after its column's heading.
    Read,
    Skip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum CodeSpeech {
    #[default]
    Read,
    /// "A code sample", not the code.
    Announce,
    Skip,
}

impl BookModel {
    /// An EPUB's or a PDF's text (told apart by its content).
    pub fn parse(id: &str, bytes: &[u8], layout: &Layout) -> Result<Self, String> {
        Ok(Self::from_parsed(id, parse_document(bytes)?, layout))
    }

    pub fn from_parsed(id: &str, parsed: ParsedBook, layout: &Layout) -> Self {
        let options = &layout.segments;
        let mut first_entry = 0;
        let chapters: Vec<ChapterModel> = parsed
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| {
                // A table's cells are drawn apart, so no sentence runs across two.
                let cuts: Vec<(usize, usize)> = chapter
                    .tables
                    .iter()
                    .flat_map(|table| table.rows.clone().zip(&table.cells))
                    .flat_map(|(block, cells)| {
                        cells
                            .iter()
                            .filter(|cell| cell.start > 0)
                            .map(move |cell| (block, cell.start))
                    })
                    .collect();
                let segments =
                    segment_chapter_with_cuts(index as u32, &chapter.blocks, &cuts, options);
                let offsets = aoide_text::block_offsets(&chapter.blocks);
                let mut notes = vec![Vec::new(); segments.len()];
                for note_ref in &chapter.note_refs {
                    let Some(base) = offsets.get(note_ref.block) else {
                        continue;
                    };
                    let position = base + note_ref.offset;
                    let in_block = || segments.iter().rposition(|s| s.block == note_ref.block);
                    let Some(segment) = segments
                        .iter()
                        .position(|s| s.block == note_ref.block && position <= s.range.end)
                        .or_else(in_block)
                    else {
                        continue;
                    };
                    let words_before = segments[segment]
                        .words
                        .iter()
                        .filter(|w| w.end <= position)
                        .count();
                    notes[segment].push(NoteMarker {
                        after_word: words_before.saturating_sub(1),
                        label: note_ref.label.clone(),
                        text: note_ref
                            .note
                            .and_then(|n| parsed.notes.get(n))
                            .map(|n| n.text.clone()),
                    });
                }
                let (segments, notes) = if layout.read_notes {
                    with_note_segments(index, segments, notes)
                } else {
                    (segments, notes)
                };
                let (segments, notes) = described(
                    index,
                    chapter,
                    &offsets,
                    segments,
                    notes,
                    &layout.describe,
                    options.paragraph_pause_ms,
                );
                let model = ChapterModel {
                    notes,
                    title: chapter.title.clone(),
                    text: chapter.text.clone(),
                    block_kinds: chapter.blocks.iter().map(|b| b.kind).collect(),
                    first_entry,
                    segments,
                    inserts: chapter.inserts.clone(),
                    tables: chapter.tables.clone(),
                    block_starts: offsets,
                    // An article's code keeps its whitespace in its text; an EPUB's beside it.
                    layouts: chapter
                        .blocks
                        .iter()
                        .map(|b| match b.kind {
                            BlockKind::Preformatted => b.layout.clone().or_else(|| {
                                let collapsed =
                                    b.text.split_whitespace().collect::<Vec<_>>().join(" ");
                                (collapsed != b.text).then(|| b.text.clone())
                            }),
                            _ => None,
                        })
                        .collect(),
                };
                first_entry += model.segments.len();
                model
            })
            .collect();
        Self {
            id: id.to_owned(),
            title: parsed.title.clone().unwrap_or_else(|| "Untitled".into()),
            authors: parsed.authors.clone(),
            language: parsed.language.clone(),
            description: parsed
                .description
                .as_deref()
                .map(plain_text)
                .filter(|d| !d.is_empty()),
            publisher: parsed.publisher.clone(),
            isbn: parsed.isbn.clone(),
            warnings: parsed.warnings.iter().map(describe_warning).collect(),
            excluded: parsed
                .excluded
                .iter()
                .map(|e| ExcludedView {
                    title: e.chapter.title.clone(),
                    reason: format!("{:?}", e.reason),
                })
                .collect(),
            chapters,
            total_entries: first_entry,
        }
    }

    pub fn total_entries(&self) -> usize {
        self.total_entries
    }

    /// (chapter, segment index) of a global playlist index.
    pub fn locate(&self, global: usize) -> Option<(usize, usize)> {
        let chapter = self
            .chapters
            .partition_point(|c| c.first_entry <= global)
            .checked_sub(1)?;
        let local = global - self.chapters[chapter].first_entry;
        (local < self.chapters[chapter].segments.len()).then_some((chapter, local))
    }

    pub fn segment(&self, global: usize) -> Option<&Segment> {
        let (chapter, local) = self.locate(global)?;
        self.chapters[chapter].segments.get(local)
    }

    pub fn index_of(&self, segment_id: &str) -> Option<usize> {
        let chapter: usize = segment_id.split(':').next()?.parse().ok()?;
        let model = self.chapters.get(chapter)?;
        let local = model.segments.iter().position(|s| s.id == segment_id)?;
        Some(model.first_entry + local)
    }

    /// First speakable segment of a chapter (or its first segment).
    pub fn chapter_start(&self, chapter: usize) -> Option<usize> {
        let model = self.chapters.get(chapter)?;
        let local = model
            .segments
            .iter()
            .position(|s| !s.speech.is_empty())
            .unwrap_or(0);
        (!model.segments.is_empty()).then_some(model.first_entry + local)
    }

    /// Global index for a saved (chapter, char offset) position.
    pub fn index_at(&self, chapter: usize, char_offset: usize) -> Option<usize> {
        let model = self.chapters.get(chapter)?;
        let local = segment_at(&model.segments, char_offset)?;
        Some(model.first_entry + local)
    }

    pub fn fraction(&self, global: usize) -> f64 {
        if self.total_entries == 0 {
            return 0.0;
        }
        (global as f64 / self.total_entries as f64).clamp(0.0, 1.0)
    }

    /// Position within the global index's chapter, 0..1.
    pub fn chapter_fraction(&self, global: usize) -> f64 {
        match self.locate(global) {
            Some((chapter, local)) => {
                let len = self.chapters[chapter].segments.len().max(1);
                local as f64 / len as f64
            }
            None => 1.0,
        }
    }

    /// Words that are read aloud (silent segments such as "* * *" are left out).
    pub fn word_count(&self) -> usize {
        (0..self.chapters.len()).map(|c| self.words_in(c)).sum()
    }

    /// Words of chapter `chapter` that are read aloud.
    pub fn words_in(&self, chapter: usize) -> usize {
        self.chapters.get(chapter).map_or(0, |c| {
            c.segments
                .iter()
                .filter(|s| !s.speech.is_empty())
                .map(|s| s.display.split_whitespace().count())
                .sum()
        })
    }

    pub fn chapter_summaries(&self) -> Vec<ChapterSummary> {
        self.chapters
            .iter()
            .enumerate()
            .map(|(index, c)| ChapterSummary {
                index,
                title: c.title.clone(),
                duration_ms: None,
                left_out: false,
                suggested: false,
                word_count: self.words_in(index),
            })
            .collect()
    }

    pub fn chapter_view(&self, index: usize) -> Option<ChapterView> {
        let model = self.chapters.get(index)?;
        let mut blocks: Vec<BlockView> = model
            .block_kinds
            .iter()
            .enumerate()
            .map(|(block, kind)| {
                let layout = model.layouts.get(block).cloned().flatten();
                let (kind, level) = match kind {
                    BlockKind::Heading { level } => ("heading", Some(*level)),
                    BlockKind::Paragraph => ("paragraph", None),
                    BlockKind::ListItem => ("listItem", None),
                    BlockKind::Preformatted => ("preformatted", None),
                };
                BlockView {
                    kind,
                    level,
                    segments: Vec::new(),
                    layout,
                }
            })
            .collect();
        for (local, segment) in model.segments.iter().enumerate() {
            if let Some(block) = blocks.get_mut(segment.block) {
                block.segments.push(SegmentView {
                    id: segment.id.clone(),
                    words: segment
                        .display
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect(),
                    starts: segment.words.iter().map(|word| word.start).collect(),
                    speakable: !segment.speech.is_empty(),
                    can_say_again: !segment.speech.is_empty(),
                    note: segment.id.contains(":n"),
                    aside: segment.id.contains(":i"),
                    notes: model.notes[local].clone(),
                });
            }
        }
        blocks.retain(|b| !b.segments.is_empty());
        Some(ChapterView {
            index,
            title: model.title.clone(),
            blocks,
            not_narrated: None,
            inserts: Vec::new(),
            tables: self.table_views(index),
        })
    }

    /// Which of chapter `index`'s blocks its view shows (those with segments).
    fn shown_blocks(model: &ChapterModel) -> Vec<bool> {
        let mut shown = vec![false; model.block_kinds.len()];
        for segment in &model.segments {
            if let Some(block) = shown.get_mut(segment.block) {
                *block = true;
            }
        }
        shown
    }

    fn table_views(&self, index: usize) -> Vec<TableView> {
        let Some(model) = self.chapters.get(index) else {
            return Vec::new();
        };
        let shown = Self::shown_blocks(model);
        let in_view = |block: usize| {
            shown[..block.min(shown.len())]
                .iter()
                .filter(|s| **s)
                .count()
        };
        model
            .tables
            .iter()
            .filter(|table| table.rows.clone().all(|b| shown.get(b) == Some(&true)))
            .map(|table| TableView {
                first_block: in_view(table.rows.start),
                rows: table.rows.len(),
                header_rows: table.header_rows,
                caption_block: table
                    .caption_block
                    .filter(|b| shown.get(*b) == Some(&true))
                    .map(in_view),
                cells: table
                    .rows
                    .clone()
                    .zip(&table.cells)
                    .map(|(block, cells)| {
                        let base = model.block_starts.get(block).copied().unwrap_or(0);
                        cells
                            .iter()
                            .map(|cell| [base + cell.start, base + cell.end])
                            .collect()
                    })
                    .collect(),
            })
            .collect()
    }

    /// Chapter `index`'s pictures, as its view draws them: `file_of` gives each resource's file
    /// (`None` leaves it out), and block numbers count the blocks the view shows.
    pub fn insert_views(
        &self,
        index: usize,
        file_of: &dyn Fn(&[&str]) -> Vec<Option<std::path::PathBuf>>,
    ) -> Vec<InsertView> {
        let Some(model) = self.chapters.get(index) else {
            return Vec::new();
        };
        let shown = Self::shown_blocks(model);
        // A block number in the view: how many shown blocks come before it.
        let in_view = |block: usize| {
            shown[..block.min(shown.len())]
                .iter()
                .filter(|s| **s)
                .count()
        };
        let resources: Vec<&str> = model
            .inserts
            .iter()
            .filter_map(|insert| match &insert.kind {
                aoide_document::InsertKind::Picture { resource, .. } => Some(resource.as_str()),
                aoide_document::InsertKind::Table { .. } => None,
            })
            .collect();
        let mut files = file_of(&resources).into_iter();
        model
            .inserts
            .iter()
            .filter_map(|insert| match &insert.kind {
                aoide_document::InsertKind::Table {
                    rows,
                    header_rows,
                    caption,
                } => Some(InsertView::Table {
                    before_block: in_view(insert.before_block),
                    rows: rows.clone(),
                    header_rows: *header_rows,
                    caption: caption.clone(),
                }),
                aoide_document::InsertKind::Picture { alt, caption, .. } => {
                    let file = files.next().flatten();
                    let (caption_block, caption) = match caption {
                        Some(aoide_document::Caption::Block(block))
                            if shown.get(*block) == Some(&true) =>
                        {
                            (Some(in_view(*block)), None)
                        }
                        Some(aoide_document::Caption::Text(text)) => (None, Some(text.clone())),
                        _ => (None, None),
                    };
                    Some(InsertView::Picture {
                        before_block: in_view(insert.before_block),
                        path: file?.to_string_lossy().into_owned(),
                        alt: alt.clone(),
                        caption_block,
                        caption,
                    })
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SearchHit {
    pub chapter: usize,
    pub chapter_title: String,
    pub segment_id: String,
    /// Text around the match; `match_start..match_end` (char offsets) is the match.
    pub snippet: String,
    pub match_start: usize,
    pub match_end: usize,
}

impl BookModel {
    /// Case-insensitive search of segment text (queries under 2 chars match nothing).
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
        let needle: Vec<char> = query.trim().chars().map(fold).collect();
        if needle.len() < 2 {
            return Vec::new();
        }
        let mut hits = Vec::new();
        for (chapter, model) in self.chapters.iter().enumerate() {
            for segment in &model.segments {
                let chars: Vec<char> = segment.display.chars().collect();
                let lower: Vec<char> = chars.iter().copied().map(fold).collect();
                let Some(at) = lower
                    .windows(needle.len())
                    .position(|w| w == needle.as_slice())
                else {
                    continue;
                };
                let from = at.saturating_sub(60);
                let to = (at + needle.len() + 60).min(chars.len());
                hits.push(SearchHit {
                    chapter,
                    chapter_title: model.title.clone(),
                    segment_id: segment.id.clone(),
                    snippet: chars[from..to].iter().collect(),
                    match_start: at - from,
                    match_end: at - from + needle.len(),
                });
                if hits.len() >= limit {
                    return hits;
                }
            }
        }
        hits
    }
}

/// Silence before and between notes read after their paragraph.
const NOTE_PAUSE_MS: u32 = 400;

/// Appends a spoken segment per referenced note after the last segment of its block.
/// A table's cells (`cols` columns and `rows` rows) as said aloud: "A table of …".
fn a_table_of(cols: usize, rows: usize, caption: Option<&str>) -> String {
    let plural = |n: usize, what: &str| format!("{n} {what}{}", if n == 1 { "" } else { "s" });
    let caption = caption
        .map(|c| format!(": {}", c.trim_end_matches('.')))
        .unwrap_or_default();
    format!(
        "A table of {} and {}{caption}.",
        plural(cols, "column"),
        plural(rows, "row")
    )
}

/// The chapter's segments with what `describe` asks said for its pictures, tables and code:
/// text segments silenced (same ids, nothing said, no pause) or said differently, and spoken
/// extras (`{chapter}:{block}:i{n}`) before a block, or after the last.
fn described(
    chapter_index: usize,
    chapter: &aoide_document::Chapter,
    offsets: &[usize],
    mut segments: Vec<Segment>,
    mut notes: Vec<Vec<NoteMarker>>,
    describe: &Describe,
    pause: u32,
) -> (Vec<Segment>, Vec<Vec<NoteMarker>>) {
    use aoide_document::{Caption, InsertKind};
    let silence = |segment: &mut Segment| {
        segment.speech.clear();
        segment.pause_after_ms = 0;
    };
    let text = |s: &Segment| !s.id.contains(":n");
    // (block, said): extras before a block (`blocks.len()`: after the last).
    let mut extras: Vec<(usize, String)> = Vec::new();
    for table in &chapter.tables {
        let in_rows = |s: &Segment| table.rows.contains(&s.block) && text(s);
        match describe.tables {
            TableSpeech::Announce | TableSpeech::Skip => {
                segments.iter_mut().filter(|s| in_rows(s)).for_each(silence);
                if describe.tables == TableSpeech::Announce {
                    let cols = table.cells.iter().map(Vec::len).max().unwrap_or(0);
                    extras.push((table.rows.start, a_table_of(cols, table.rows.len(), None)));
                }
            }
            TableSpeech::Read => {
                let cell_text = |block: usize, range: &std::ops::Range<usize>| -> String {
                    chapter.blocks[block]
                        .text
                        .chars()
                        .skip(range.start)
                        .take(range.len())
                        .collect()
                };
                let headings: Vec<String> = match table.header_rows {
                    0 => Vec::new(),
                    n => table.cells[n - 1]
                        .iter()
                        .map(|cell| cell_text(table.rows.start + n - 1, cell))
                        .collect(),
                };
                let mut said = std::collections::HashSet::new();
                for segment in segments.iter_mut().filter(|s| in_rows(s)) {
                    let row = segment.block - table.rows.start;
                    if row < table.header_rows {
                        silence(segment);
                        continue;
                    }
                    let at = segment.range.start - offsets[segment.block];
                    let column = table.cells[row].iter().rposition(|cell| cell.start <= at);
                    if let Some(column) = column.filter(|&c| said.insert((row, c))) {
                        if let Some(heading) = headings.get(column).filter(|h| !h.is_empty()) {
                            segment.speech = format!("{heading}: {}", segment.speech);
                        }
                    }
                }
            }
        }
    }
    for (block, kind) in chapter.blocks.iter().enumerate().map(|(i, b)| (i, b.kind)) {
        if kind != BlockKind::Preformatted || describe.code == CodeSpeech::Read {
            continue;
        }
        segments
            .iter_mut()
            .filter(|s| s.block == block && text(s))
            .for_each(silence);
        if describe.code == CodeSpeech::Announce {
            extras.push((block, "A code sample.".into()));
        }
    }
    for insert in &chapter.inserts {
        let said = match &insert.kind {
            InsertKind::Picture { alt, caption, .. } => {
                let alt = (describe.pictures == PictureSpeech::AltAndCaption && !alt.is_empty())
                    .then_some(alt.as_str());
                let caption = match caption {
                    Some(Caption::Text(text)) if describe.pictures != PictureSpeech::Off => {
                        Some(text.as_str())
                    }
                    _ => None,
                };
                let parts: Vec<String> = alt
                    .into_iter()
                    .chain(caption)
                    .map(|part| format!("{}.", part.trim_end_matches('.')))
                    .collect();
                (!parts.is_empty()).then(|| format!("Picture: {}", parts.join(" ")))
            }
            InsertKind::Table {
                rows,
                header_rows,
                caption,
            } => match describe.tables {
                TableSpeech::Skip => None,
                TableSpeech::Announce => {
                    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
                    Some(a_table_of(cols, rows.len(), caption.as_deref()))
                }
                TableSpeech::Read => {
                    let headings = header_rows.checked_sub(1).and_then(|h| rows.get(h));
                    let said: Vec<String> = rows
                        .iter()
                        .skip(*header_rows)
                        .flat_map(|row| row.iter().enumerate())
                        .filter(|(_, cell)| !cell.is_empty())
                        .map(|(c, cell)| {
                            let cell = cell.trim_end_matches('.');
                            match headings.and_then(|h| h.get(c)).filter(|h| !h.is_empty()) {
                                Some(heading) => format!("{heading}: {cell}."),
                                None => format!("{cell}."),
                            }
                        })
                        .collect();
                    let caption = caption
                        .iter()
                        .map(|c| format!("{}.", c.trim_end_matches('.')));
                    Some(caption.chain(said).collect::<Vec<_>>().join(" "))
                }
            },
        };
        extras.extend(said.map(|said| (insert.before_block, said)));
    }
    if extras.is_empty() {
        return (segments, notes);
    }
    extras.sort_by_key(|(block, _)| *block);
    let last_block = chapter.blocks.len().saturating_sub(1);
    let mut counts: std::collections::HashMap<usize, usize> = Default::default();
    let mut extra = |block: usize, said: String| {
        let anchor = block.min(last_block);
        let n = counts.entry(anchor).or_default();
        let id = format!("{chapter_index}:{anchor}:i{n}");
        *n += 1;
        let at = offsets
            .get(block)
            .copied()
            .unwrap_or_else(|| chapter.text.chars().count());
        Segment {
            id,
            block: anchor,
            range: at..at,
            speech: said.clone(),
            display: said,
            sentences: vec![],
            words: vec![],
            pause_after_ms: pause,
            role: aoide_text::Role::Narration,
        }
    };
    let mut pending = extras.into_iter().peekable();
    let mut out = Vec::with_capacity(segments.len() + 4);
    let mut out_notes = Vec::with_capacity(out.capacity());
    for (segment, markers) in segments.drain(..).zip(notes.drain(..)) {
        while let Some((block, _)) = pending.peek() {
            if *block > segment.block {
                break;
            }
            let (block, said) = pending.next().unwrap();
            out.push(extra(block, said));
            out_notes.push(Vec::new());
        }
        out.push(segment);
        out_notes.push(markers);
    }
    for (block, said) in pending {
        out.push(extra(block, said));
        out_notes.push(Vec::new());
    }
    (out, out_notes)
}

fn with_note_segments(
    chapter: usize,
    segments: Vec<Segment>,
    notes: Vec<Vec<NoteMarker>>,
) -> (Vec<Segment>, Vec<Vec<NoteMarker>>) {
    let blocks: Vec<usize> = segments.iter().map(|s| s.block).collect();
    let mut out_segments = Vec::with_capacity(segments.len());
    let mut out_notes = Vec::with_capacity(notes.len());
    let mut pending: Vec<NoteMarker> = Vec::new();
    for (i, (segment, markers)) in segments.into_iter().zip(notes).enumerate() {
        pending.extend(markers.iter().filter(|m| m.text.is_some()).cloned());
        let block = segment.block;
        let end = segment.range.end;
        let mut segment = segment;
        // The pause after the paragraph (a scene break's, the chapter end's) moves after the
        // last note read there.
        let after = segment.pause_after_ms.max(NOTE_PAUSE_MS);
        if !pending.is_empty() && blocks.get(i + 1) != Some(&block) {
            segment.pause_after_ms = NOTE_PAUSE_MS;
        }
        out_segments.push(segment);
        out_notes.push(markers);
        if blocks.get(i + 1) == Some(&block) {
            continue;
        }
        let count = pending.len();
        for (n, marker) in pending.drain(..).enumerate() {
            let pause = if n + 1 == count { after } else { NOTE_PAUSE_MS };
            let display = format!("Note {}. {}", marker.label, marker.text.unwrap_or_default());
            out_segments.push(Segment {
                id: format!("{chapter}:{block}:n{n}"),
                block,
                range: end..end,
                speech: display.clone(),
                display,
                sentences: vec![],
                words: vec![],
                pause_after_ms: pause,
                role: aoide_text::Role::Narration,
            });
            out_notes.push(vec![]);
        }
    }
    (out_segments, out_notes)
}

/// Index of the word being spoken `offset_ms` into a segment whose audio lasts `duration_ms`.
/// The word being spoken at `offset_ms`: the last word that has started (the first word
/// before speech begins). Uses `timings` when they match the segment's words, else an estimate.
pub fn word_index(
    timings: Option<&[aoide_text::WordEstimate]>,
    segment: &Segment,
    duration_ms: u32,
    offset_ms: u32,
) -> Option<usize> {
    if segment.words.is_empty() || duration_ms == 0 {
        return None;
    }
    let estimates;
    let timings = match timings.filter(|t| t.len() == segment.words.len()) {
        Some(timings) => timings,
        None => {
            estimates = estimate_word_timings(segment, duration_ms);
            &estimates
        }
    };
    let offset = offset_ms.min(duration_ms.saturating_sub(1));
    Some(
        timings
            .iter()
            .rposition(|t| t.start_ms <= offset)
            .unwrap_or(0),
    )
}

fn describe_warning(warning: &Warning) -> String {
    match warning {
        Warning::MissingNavigation => {
            "The book has no table of contents; chapters were inferred from the reading order."
                .into()
        }
        Warning::InferredChapter { title, .. } => {
            format!("Chapter \u{201c}{title}\u{201d} was inferred from a heading.")
        }
        Warning::AmbiguousFrontMatter { title } => format!(
            "\u{201c}{title}\u{201d} looks like front matter but is not marked as such; it is kept."
        ),
        Warning::NavigationTargetMissing { path, .. } => {
            format!("A table-of-contents entry points to a missing location ({path}).")
        }
        Warning::EmptySection { path } => format!("An empty section was skipped ({path})."),
        Warning::UnresolvedNoteRef { target, .. } => {
            format!("A note marker points to a note that was not found ({target}).")
        }
        Warning::UnreferencedNote { id } => {
            format!("A note is not referenced from the text ({id}); it is listed with the notes.")
        }
        Warning::LeftOut { what, count } => {
            use aoide_epub::LeftOut;
            let (one, many) = match what {
                LeftOut::Image => ("image", "images"),
                LeftOut::Figure => ("figure", "figures"),
                LeftOut::Table => ("table", "tables"),
                LeftOut::Embed => ("embedded item", "embedded items"),
                LeftOut::Form => ("form", "forms"),
            };
            match count {
                1 => format!("1 {one} was left out."),
                n => format!("{n} {many} were left out."),
            }
        }
        Warning::BodyFromMetadata => {
            "The page showed less text than it declared; the text comes from the page's metadata."
                .into()
        }
        Warning::GuessedEncoding { encoding } => format!(
            "The file is not UTF-8; it was read as {encoding}, so a few characters may be wrong."
        ),
    }
}

/// Strips markup and common entities from a metadata description and tidies whitespace.
pub fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                out.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
pub(crate) mod tests {
    #[test]
    fn plain_text_strips_markup_and_entities() {
        assert_eq!(
            super::plain_text("<p>A <b>tale</b> of&nbsp;two&amp;three</p>\n<p>cities.</p>"),
            "A tale of two&three cities."
        );
    }

    use super::*;
    use aoide_epub::{Block, Chapter, SourceLocation};

    fn chapter(title: &str, blocks: Vec<Block>) -> Chapter {
        Chapter {
            title: title.into(),
            text: blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
            blocks,
            note_refs: vec![],
            inserts: Vec::new(),
            tables: Vec::new(),
            sources: vec![SourceLocation {
                path: "a.xhtml".into(),
                fragment: None,
            }],
        }
    }

    fn notes_read() -> Layout {
        Layout {
            read_notes: true,
            ..Layout::default()
        }
    }

    fn block(kind: BlockKind, text: &str) -> Block {
        Block {
            kind,
            text: text.into(),
            break_before: false,
            layout: None,
        }
    }

    fn book() -> BookModel {
        let parsed = ParsedBook {
            title: Some("A Small Journey".into()),
            authors: vec!["Test Author".into()],
            language: Some("en".into()),
            publisher: None,
            description: None,
            identifiers: vec![],
            isbn: None,
            subjects: Vec::new(),
            series: None,
            cover: None,
            notes: vec![],
            chapters: vec![
                chapter(
                    "One",
                    vec![
                        block(BlockKind::Heading { level: 1 }, "One"),
                        block(
                            BlockKind::Paragraph,
                            "The boat reached the grey shore before the storm arrived. Everyone aboard the small boat was very tired.",
                        ),
                    ],
                ),
                chapter(
                    "Two",
                    vec![
                        block(BlockKind::Paragraph, "* * *"),
                        block(BlockKind::Paragraph, "Morning came slowly over the quiet harbour town."),
                    ],
                ),
            ],
            warnings: vec![Warning::MissingNavigation],
            excluded: vec![],
        };
        BookModel::from_parsed("bk", parsed, &Layout::default())
    }

    #[test]
    fn note_markers_attach_to_the_word_they_follow() {
        let mut parsed_chapter = chapter(
            "Notes",
            vec![block(
                BlockKind::Paragraph,
                "The harbour was quiet when the boats came in.",
            )],
        );
        parsed_chapter.note_refs = vec![
            aoide_epub::NoteRef {
                block: 0,
                offset: 11,
                label: "1".into(),
                note: Some(0),
            },
            aoide_epub::NoteRef {
                block: 0,
                offset: 46,
                label: "2".into(),
                note: None,
            },
        ];
        let parsed = ParsedBook {
            title: None,
            authors: vec![],
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            isbn: None,
            subjects: Vec::new(),
            series: None,
            cover: None,
            notes: vec![aoide_epub::Note {
                id: "a#n".into(),
                text: "Built in 1820.".into(),
            }],
            chapters: vec![parsed_chapter],
            warnings: vec![],
            excluded: vec![],
        };
        let view = BookModel::from_parsed("bk", parsed, &Layout::default())
            .chapter_view(0)
            .unwrap();
        let notes = &view.blocks[0].segments[0].notes;
        assert_eq!(notes[0].after_word, 1);
        assert_eq!(notes[0].text.as_deref(), Some("Built in 1820."));
        assert_eq!(notes[1].after_word, 8);
        assert_eq!(notes[1].text, None);
    }

    #[test]
    fn read_notes_inserts_spoken_notes_after_their_paragraph() {
        let mut parsed_chapter = chapter(
            "Notes",
            vec![
                block(
                    BlockKind::Paragraph,
                    "The harbour was quiet when the boats came in.",
                ),
                block(
                    BlockKind::Paragraph,
                    "Morning came slowly over the quiet harbour town.",
                ),
            ],
        );
        parsed_chapter.note_refs = vec![aoide_epub::NoteRef {
            block: 0,
            offset: 11,
            label: "1".into(),
            note: Some(0),
        }];
        let parsed = ParsedBook {
            title: None,
            authors: vec![],
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            isbn: None,
            subjects: Vec::new(),
            series: None,
            cover: None,
            notes: vec![aoide_epub::Note {
                id: "a#n".into(),
                text: "Built in 1820.".into(),
            }],
            chapters: vec![parsed_chapter],
            warnings: vec![],
            excluded: vec![],
        };
        let book = BookModel::from_parsed("bk", parsed, &notes_read());
        assert_eq!(book.total_entries(), 3);
        assert_eq!(book.segment(1).unwrap().speech, "Note 1. Built in 1820.");
        assert_eq!(book.index_of("0:0:n0"), Some(1));
        let view = book.chapter_view(0).unwrap();
        assert!(view.blocks[0].segments[1].note);
        assert!(!view.blocks[1].segments[0].note);
        assert_eq!(book.index_at(0, 60), Some(2));
    }

    #[test]
    fn a_note_read_after_the_chapters_last_paragraph_keeps_the_chapter_end_pause() {
        let mut parsed_chapter = chapter(
            "Notes",
            vec![block(
                BlockKind::Paragraph,
                "The harbour was quiet when the boats came in.",
            )],
        );
        parsed_chapter.note_refs = vec![aoide_epub::NoteRef {
            block: 0,
            offset: 11,
            label: "1".into(),
            note: Some(0),
        }];
        let parsed = ParsedBook {
            title: None,
            authors: vec![],
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            isbn: None,
            subjects: Vec::new(),
            series: None,
            cover: None,
            notes: vec![aoide_epub::Note {
                id: "a#n".into(),
                text: "Built in 1820.".into(),
            }],
            chapters: vec![parsed_chapter],
            warnings: vec![],
            excluded: vec![],
        };
        let book = BookModel::from_parsed("bk", parsed, &notes_read());
        let chapter_end = SegmentOptions::default().heading_pause_ms;
        assert_eq!(book.segment(0).unwrap().pause_after_ms, 400);
        assert_eq!(book.segment(1).unwrap().pause_after_ms, chapter_end);
    }

    #[test]
    fn playlist_spans_chapters_and_maps_both_ways() {
        let book = book();
        assert_eq!(book.total_entries(), 5);
        assert_eq!(book.locate(0), Some((0, 0)));
        assert_eq!(book.locate(3), Some((1, 0)));
        assert_eq!(book.locate(5), None);
        assert_eq!(book.index_of("1:1:0"), Some(4));
        assert_eq!(book.index_of("9:0:0"), None);
        assert_eq!(
            book.segment(4).unwrap().display,
            "Morning came slowly over the quiet harbour town."
        );
    }

    #[test]
    fn search_is_case_insensitive_with_snippets() {
        let book = book();
        let hits = book.search("HARBOUR", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].segment_id, "1:1:0");
        assert_eq!(hits[0].chapter_title, "Two");
        let hit = &hits[0];
        let matched: String = hit
            .snippet
            .chars()
            .skip(hit.match_start)
            .take(hit.match_end - hit.match_start)
            .collect();
        assert_eq!(matched, "harbour");
        assert_eq!(book.search("the", 2).len(), 2);
        assert!(book.search("a", 10).is_empty());
    }

    #[test]
    fn chapter_start_skips_unspeakable_segments() {
        let book = book();
        assert_eq!(book.chapter_start(0), Some(0));
        assert_eq!(book.chapter_start(1), Some(4));
    }

    #[test]
    fn word_count_covers_spoken_segments_only() {
        // "One" + 10 + 8 words in chapter one; "* * *" is silent; 8 words in chapter two.
        assert_eq!(book().word_count(), 27);
    }

    #[test]
    fn chapter_views_group_segments_by_block_with_words() {
        let view = book().chapter_view(0).unwrap();
        assert_eq!(view.blocks.len(), 2);
        assert_eq!(view.blocks[0].kind, "heading");
        assert_eq!(view.blocks[0].level, Some(1));
        assert_eq!(view.blocks[1].segments.len(), 2);
        assert_eq!(view.blocks[1].segments[1].words[0], "Everyone");
        let two = book().chapter_view(1).unwrap();
        assert!(!two.blocks[0].segments[0].speakable);
    }

    /// A chapter of every shape the page draws: a heading, a paragraph with a footnote read
    /// aloud, astral characters and a combining accent, a code block that keeps its line breaks
    /// and indentation, and a list item. The UI's selection tests count on it too
    /// (`chapter-texts` in `core/fixtures.rs`).
    pub(crate) fn every_shape() -> BookModel {
        let mut parsed_chapter = chapter(
            "Shapes",
            vec![
                block(BlockKind::Heading { level: 1 }, "Shapes"),
                block(
                    BlockKind::Paragraph,
                    "The 🌊 harbour was quiet when the boats came in. Cafe\u{301} lights burned 𝒶ll night.",
                ),
                block(
                    BlockKind::Preformatted,
                    "fn main() {\n    println!(\"tide\");\n\n        let  x = 1;\n}",
                ),
                block(BlockKind::ListItem, "A rope,  coiled twice."),
            ],
        );
        parsed_chapter.note_refs = vec![aoide_epub::NoteRef {
            block: 1,
            offset: 13,
            label: "1".into(),
            note: Some(0),
        }];
        let parsed = ParsedBook {
            title: None,
            authors: vec![],
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            isbn: None,
            subjects: Vec::new(),
            series: None,
            cover: None,
            notes: vec![aoide_epub::Note {
                id: "a#n".into(),
                text: "Built in 1820.".into(),
            }],
            chapters: vec![parsed_chapter],
            warnings: vec![],
            excluded: vec![],
        };
        BookModel::from_parsed("bk", parsed, &notes_read())
    }

    /// A chapter with a captioned table of a heading row and two rows.
    fn tabled() -> BookModel {
        let mut parsed = empty_parsed();
        parsed.chapters = vec![tabled_chapter()];
        BookModel::from_parsed("bk", parsed, &Layout::default())
    }

    fn tabled_chapter() -> Chapter {
        let mut parsed_chapter = chapter(
            "Lamps",
            vec![
                block(BlockKind::Heading { level: 1 }, "Lamps"),
                block(BlockKind::Paragraph, "Lamps lit each year"),
                block(BlockKind::Paragraph, "Year Lamps"),
                block(BlockKind::Paragraph, "1901 12"),
                block(BlockKind::Paragraph, "1902 14 and a half"),
                block(BlockKind::Paragraph, "After the table."),
            ],
        );
        parsed_chapter.tables = vec![aoide_document::TableShape {
            rows: 2..5,
            caption_block: Some(1),
            header_rows: 1,
            cells: vec![vec![0..4, 5..10], vec![0..4, 5..7], vec![0..4, 5..18]],
        }];
        parsed_chapter
    }

    fn empty_parsed() -> ParsedBook {
        ParsedBook {
            title: None,
            authors: vec![],
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            isbn: None,
            subjects: Vec::new(),
            series: None,
            cover: None,
            notes: vec![],
            chapters: vec![],
            warnings: vec![],
            excluded: vec![],
        }
    }

    fn described(describe: Describe) -> Layout {
        Layout {
            describe,
            ..Layout::default()
        }
    }

    /// Each segment's id and what is said for it ("" for nothing), in reading order.
    fn said(model: &BookModel) -> Vec<(String, String)> {
        model.chapters[0]
            .segments
            .iter()
            .map(|s| (s.id.clone(), s.speech.clone()))
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(id, said)| (id.to_string(), said.to_string()))
            .collect()
    }

    #[test]
    fn a_table_is_announced_read_row_by_row_or_skipped_as_chosen() {
        let parsed = || {
            let mut parsed = empty_parsed();
            parsed.chapters = vec![tabled_chapter()];
            parsed
        };
        let announced = BookModel::from_parsed("bk", parsed(), &Layout::default());
        assert_eq!(
            said(&announced),
            pairs(&[
                ("0:0:0", "Lamps."),
                ("0:1:0", "Lamps lit each year"),
                ("0:2:i0", "A table of 2 columns and 3 rows."),
                ("0:2:0", ""),
                ("0:2:1", ""),
                ("0:3:0", ""),
                ("0:3:1", ""),
                ("0:4:0", ""),
                ("0:4:1", ""),
                ("0:5:0", "After the table."),
            ])
        );
        // Its rows say nothing, and leave no silence.
        let rows = &announced.chapters[0].segments[3..9];
        assert!(rows.iter().all(|s| s.pause_after_ms == 0));
        let view = announced.chapter_view(0).unwrap();
        let aside: Vec<&str> = view
            .blocks
            .iter()
            .flat_map(|b| &b.segments)
            .filter(|s| s.aside)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(aside, ["0:2:i0"], "said, not on the page");

        let read = BookModel::from_parsed(
            "bk",
            parsed(),
            &described(Describe {
                tables: TableSpeech::Read,
                ..Describe::default()
            }),
        );
        assert_eq!(
            said(&read)[2..8],
            pairs(&[
                ("0:2:0", ""),
                ("0:2:1", ""),
                ("0:3:0", "Year: 1901"),
                ("0:3:1", "Lamps: 12"),
                ("0:4:0", "Year: 1902"),
                ("0:4:1", "Lamps: 14 and a half"),
            ])
        );
        let skipped = BookModel::from_parsed(
            "bk",
            parsed(),
            &described(Describe {
                tables: TableSpeech::Skip,
                ..Describe::default()
            }),
        );
        assert_eq!(said(&skipped).len(), 9, "no announcement");
        // The text and the sentences' ids are the same whatever is said.
        for model in [&read, &skipped] {
            assert_eq!(model.chapters[0].text, announced.chapters[0].text);
        }
    }

    #[test]
    fn code_and_a_pictures_description_are_said_as_chosen() {
        let coded = || {
            let mut parsed_chapter = chapter(
                "Code",
                vec![
                    block(BlockKind::Paragraph, "Run this."),
                    block(BlockKind::Preformatted, "make lamp"),
                    block(BlockKind::Paragraph, "Done."),
                ],
            );
            parsed_chapter.inserts = vec![
                aoide_document::Insert {
                    before_block: 2,
                    kind: aoide_document::InsertKind::Picture {
                        resource: "a.png".into(),
                        alt: "The lamp lit".into(),
                        caption: None,
                    },
                },
                aoide_document::Insert {
                    before_block: 3,
                    kind: aoide_document::InsertKind::Table {
                        rows: vec![
                            vec!["Month".into(), "Task".into()],
                            vec!["April".into(), "Check the queen".into()],
                        ],
                        header_rows: 1,
                        caption: Some("The year".into()),
                    },
                },
            ];
            parsed_chapter
        };
        let model = |describe| {
            let mut parsed = empty_parsed();
            parsed.chapters = vec![coded()];
            said(&BookModel::from_parsed("bk", parsed, &described(describe)))
        };
        assert_eq!(
            model(Describe::default()),
            pairs(&[
                ("0:0:0", "Run this."),
                ("0:1:0", "make lamp"),
                ("0:2:0", "Done."),
                ("0:2:i0", "A table of 2 columns and 2 rows: The year."),
            ])
        );
        assert_eq!(
            model(Describe {
                code: CodeSpeech::Announce,
                pictures: PictureSpeech::AltAndCaption,
                tables: TableSpeech::Read,
            }),
            pairs(&[
                ("0:0:0", "Run this."),
                ("0:1:i0", "A code sample."),
                ("0:1:0", ""),
                ("0:2:i0", "Picture: The lamp lit."),
                ("0:2:0", "Done."),
                ("0:2:i1", "The year. Month: April. Task: Check the queen."),
            ])
        );
        assert_eq!(
            model(Describe {
                code: CodeSpeech::Skip,
                tables: TableSpeech::Skip,
                ..Describe::default()
            }),
            pairs(&[("0:0:0", "Run this."), ("0:1:0", ""), ("0:2:0", "Done.")])
        );
    }

    #[test]
    fn code_is_drawn_as_written_and_other_blocks_have_no_layout() {
        let view = every_shape().chapter_view(0).unwrap();
        let layouts: Vec<Option<&str>> = view.blocks.iter().map(|b| b.layout.as_deref()).collect();
        assert_eq!(
            layouts,
            [
                None,
                None,
                Some("fn main() {\n    println!(\"tide\");\n\n        let  x = 1;\n}"),
                None
            ]
        );
    }

    #[test]
    fn a_table_is_cut_at_its_cells_and_drawn_as_a_table() {
        let model = tabled();
        let displays: Vec<&str> = model.chapters[0]
            .segments
            .iter()
            .filter(|s| !s.id.contains(":i"))
            .map(|s| s.display.as_str())
            .collect();
        assert_eq!(
            displays,
            [
                "Lamps",
                "Lamps lit each year",
                "Year",
                "Lamps",
                "1901",
                "12",
                "1902",
                "14 and a half",
                "After the table."
            ]
        );
        let view = model.chapter_view(0).unwrap();
        // "Lamps\n\nLamps lit each year\n\n" is 28 characters; the rows follow.
        assert_eq!(
            view.tables,
            [TableView {
                first_block: 2,
                rows: 3,
                header_rows: 1,
                caption_block: Some(1),
                cells: vec![
                    vec![[28, 32], [33, 38]],
                    vec![[40, 44], [45, 47]],
                    vec![[49, 53], [54, 67]]
                ],
            }]
        );
    }

    #[test]
    fn each_word_of_a_sentence_says_where_it_starts_in_the_chapter_text() {
        let mut checked = 0;
        for model in [book(), every_shape()] {
            for (index, chapter) in model.chapters.iter().enumerate() {
                let view = model.chapter_view(index).unwrap();
                let text: Vec<char> = chapter.text.chars().collect();
                let views: Vec<&SegmentView> =
                    view.blocks.iter().flat_map(|b| &b.segments).collect();
                assert_eq!(views.len(), chapter.segments.len());
                for (segment, view) in chapter.segments.iter().zip(views) {
                    let starts: Vec<usize> = segment.words.iter().map(|w| w.start).collect();
                    assert_eq!(view.starts, starts, "{}", segment.id);
                    if view.note {
                        assert!(view.starts.is_empty(), "a spoken note is not in the text");
                        continue;
                    }
                    assert_eq!(view.starts.len(), view.words.len(), "{}", segment.id);
                    for (word, start) in view.words.iter().zip(&view.starts) {
                        let at: String =
                            text[*start..*start + word.chars().count()].iter().collect();
                        assert_eq!(&at, word);
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 40, "{checked}");
    }

    #[test]
    fn saved_offsets_resolve_to_segments() {
        let book = book();
        assert_eq!(book.index_at(0, 0), Some(0));
        assert_eq!(book.index_at(0, 70), Some(2));
        assert_eq!(book.index_at(1, 0), Some(3));
    }

    #[test]
    fn fractions_and_word_index() {
        let book = book();
        assert_eq!(book.fraction(0), 0.0);
        assert!((book.chapter_fraction(2) - 2.0 / 3.0).abs() < 1e-9);
        let segment = book.segment(4).unwrap();
        assert_eq!(word_index(None, segment, 4000, 0), Some(0));
        assert_eq!(word_index(None, segment, 4000, 3999), Some(7));
        assert_eq!(word_index(None, segment, 4000, 9999), Some(7));
        assert_eq!(word_index(None, segment, 0, 10), None);
        // Real timings win; a pause keeps the previous word highlighted.
        let timings: Vec<aoide_text::WordEstimate> = (0..8)
            .map(|i| aoide_text::WordEstimate {
                start_ms: 100 + i * 400,
                end_ms: 300 + i * 400,
            })
            .collect();
        assert_eq!(word_index(Some(&timings), segment, 4000, 50), Some(0));
        assert_eq!(word_index(Some(&timings), segment, 4000, 450), Some(0));
        assert_eq!(word_index(Some(&timings), segment, 4000, 520), Some(1));
        assert_eq!(
            word_index(Some(&timings[..3]), segment, 4000, 3999),
            Some(7)
        );
        assert_eq!(book.warnings.len(), 1);
    }
}
