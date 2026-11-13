//! A parsed, segmented book and the views the UI receives.

use aoide_epub::{BlockKind, ParsedBook, Warning};
use aoide_text::{estimate_word_timings, segment_at, segment_chapter, Segment, SegmentOptions};
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
    pub speakable: bool,
    /// A spoken footnote inserted after its paragraph (not part of the book's text flow).
    pub note: bool,
    pub notes: Vec<NoteMarker>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct BlockView {
    pub kind: &'static str,
    pub level: Option<u8>,
    pub segments: Vec<SegmentView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterSummary {
    pub index: usize,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ChapterView {
    pub index: usize,
    pub title: String,
    pub blocks: Vec<BlockView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct ExcludedView {
    pub title: String,
    pub reason: String,
}

impl BookModel {
    pub fn parse(id: &str, bytes: &[u8], read_notes: bool) -> Result<Self, String> {
        let parsed = aoide_epub::parse_epub(bytes, &aoide_epub::ParseOptions::default())
            .map_err(|e| e.to_string())?;
        Ok(Self::from_parsed(id, parsed, read_notes))
    }

    /// `read_notes` inserts a spoken segment for each referenced note after its paragraph.
    pub fn from_parsed(id: &str, parsed: ParsedBook, read_notes: bool) -> Self {
        let options = SegmentOptions::default();
        let mut first_entry = 0;
        let chapters: Vec<ChapterModel> = parsed
            .chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| {
                let segments = segment_chapter(index as u32, &chapter.blocks, &options);
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
                let (segments, notes) = if read_notes {
                    with_note_segments(index, segments, notes)
                } else {
                    (segments, notes)
                };
                let model = ChapterModel {
                    notes,
                    title: chapter.title.clone(),
                    text: chapter.text.clone(),
                    block_kinds: chapter.blocks.iter().map(|b| b.kind).collect(),
                    first_entry,
                    segments,
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
        self.chapters
            .iter()
            .flat_map(|c| &c.segments)
            .filter(|s| !s.speech.is_empty())
            .map(|s| s.display.split_whitespace().count())
            .sum()
    }

    pub fn chapter_summaries(&self) -> Vec<ChapterSummary> {
        self.chapters
            .iter()
            .enumerate()
            .map(|(index, c)| ChapterSummary {
                index,
                title: c.title.clone(),
            })
            .collect()
    }

    pub fn chapter_view(&self, index: usize) -> Option<ChapterView> {
        let model = self.chapters.get(index)?;
        let mut blocks: Vec<BlockView> = model
            .block_kinds
            .iter()
            .map(|kind| {
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
                    speakable: !segment.speech.is_empty(),
                    note: segment.id.contains(":n"),
                    notes: model.notes[local].clone(),
                });
            }
        }
        blocks.retain(|b| !b.segments.is_empty());
        Some(ChapterView {
            index,
            title: model.title.clone(),
            blocks,
        })
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

/// Appends a spoken segment per referenced note after the last segment of its block.
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
        out_segments.push(segment);
        out_notes.push(markers);
        if blocks.get(i + 1) == Some(&block) {
            continue;
        }
        for (n, marker) in pending.drain(..).enumerate() {
            let display = format!("Note {}. {}", marker.label, marker.text.unwrap_or_default());
            out_segments.push(Segment {
                id: format!("{chapter}:{block}:n{n}"),
                block,
                range: end..end,
                speech: display.clone(),
                display,
                sentences: vec![],
                words: vec![],
                pause_after_ms: 400,
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
mod tests {
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
            sources: vec![SourceLocation {
                path: "a.xhtml".into(),
                fragment: None,
            }],
        }
    }

    fn block(kind: BlockKind, text: &str) -> Block {
        Block {
            kind,
            text: text.into(),
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
        BookModel::from_parsed("bk", parsed, false)
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
            cover: None,
            notes: vec![aoide_epub::Note {
                id: "a#n".into(),
                text: "Built in 1820.".into(),
            }],
            chapters: vec![parsed_chapter],
            warnings: vec![],
            excluded: vec![],
        };
        let view = BookModel::from_parsed("bk", parsed, false)
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
            cover: None,
            notes: vec![aoide_epub::Note {
                id: "a#n".into(),
                text: "Built in 1820.".into(),
            }],
            chapters: vec![parsed_chapter],
            warnings: vec![],
            excluded: vec![],
        };
        let book = BookModel::from_parsed("bk", parsed, true);
        assert_eq!(book.total_entries(), 3);
        assert_eq!(book.segment(1).unwrap().speech, "Note 1. Built in 1820.");
        assert_eq!(book.index_of("0:0:n0"), Some(1));
        let view = book.chapter_view(0).unwrap();
        assert!(view.blocks[0].segments[1].note);
        assert!(!view.blocks[1].segments[0].note);
        assert_eq!(book.index_at(0, 60), Some(2));
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
