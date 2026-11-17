//! The parsed-text model: what a source (an EPUB, an article page) becomes before the app lays
//! it out for reading and speech. Sources produce it; nothing here knows where text came from.

#[derive(Debug, PartialEq, Eq)]
pub struct ParsedBook {
    pub title: Option<String>,
    /// Every `dc:creator`, in package order.
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    /// `dc:description` with markup removed and whitespace collapsed.
    pub description: Option<String>,
    /// Every `dc:identifier` value, in package order.
    pub identifiers: Vec<String>,
    /// First identifier that is a checksum-valid ISBN-10/13, digits only (with `X` check digit).
    pub isbn: Option<String>,
    /// Its subjects as the source lists them (`dc:subject`), for tags.
    pub subjects: Vec<String>,
    /// The series it belongs to, as the source names it.
    pub series: Option<Series>,
    pub cover: Option<Cover>,
    /// Footnote/endnote bodies detached from the narration, in document order.
    pub notes: Vec<Note>,
    pub chapters: Vec<Chapter>,
    pub warnings: Vec<Warning>,
    pub excluded: Vec<ExcludedSection>,
}

/// A series a book belongs to, and its place in it (which can be fractional: 2.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub name: String,
    /// Always finite.
    pub position: Option<f32>,
}

// Positions are read only when finite, so equality is total.
impl Eq for Series {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionReason {
    Cover,
    TitlePage,
    Copyright,
    Contents,
    Preface,
    Introduction,
    Dedication,
    Acknowledgments,
    Notes,
    Bibliography,
    Glossary,
    Index,
    Colophon,
    Credits,
    /// A newsletter's mailing parts: its "view online" link, its unsubscribe footer, its
    /// tracking images.
    Mailing,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ExcludedSection {
    pub chapter: Chapter,
    pub reason: ExclusionReason,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Warning {
    MissingNavigation,
    InferredChapter {
        path: String,
        title: String,
    },
    AmbiguousFrontMatter {
        title: String,
    },
    NavigationTargetMissing {
        path: String,
        fragment: Option<String>,
    },
    EmptySection {
        path: String,
    },
    /// A note reference whose target is not a detected note body.
    UnresolvedNoteRef {
        path: String,
        target: String,
    },
    /// A note body that nothing references; it is listed in `ParsedBook::notes` only.
    UnreferencedNote {
        id: String,
    },
    /// Parts of an article's page the text does not carry, by kind.
    LeftOut {
        what: LeftOut,
        count: usize,
    },
    /// The page's own markup carried less text than its metadata, so the body was built from
    /// the metadata's copy of it.
    BodyFromMetadata,
}

/// What an article's page shows that its text leaves out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeftOut {
    Image,
    Figure,
    Table,
    Embed,
    Form,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Chapter {
    pub title: String,
    /// Plain visible text with paragraph breaks, including the chapter heading.
    /// Always equal to the `blocks` texts joined with `"\n\n"`.
    pub text: String,
    /// Ordered visible blocks; whitespace is collapsed and no block is empty.
    pub blocks: Vec<Block>,
    /// Note reference markers removed from `blocks`, in reading order.
    pub note_refs: Vec<NoteRef>,
    pub sources: Vec<SourceLocation>,
    /// Pictures and tables shown between blocks, outside the text (no offset moves for
    /// them), in reading order.
    pub inserts: Vec<Insert>,
    /// Tables whose rows are blocks of the text (an EPUB's), with where each cell is.
    pub tables: Vec<TableShape>,
}

/// A table that is part of the text: each row a block, its cells' texts joined by spaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableShape {
    /// The blocks that are its rows.
    pub rows: std::ops::Range<usize>,
    /// The block that is its caption.
    pub caption_block: Option<usize>,
    /// How many rows at the top are headings (every cell a `th`).
    pub header_rows: usize,
    /// Each row's cells, as character ranges in its block's text (an empty cell is empty).
    pub cells: Vec<Vec<std::ops::Range<usize>>>,
}

/// Something shown between a chapter's blocks that takes no part of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insert {
    /// It stands before this block (`blocks.len()`: after the last).
    pub before_block: usize,
    pub kind: InsertKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertKind {
    Picture {
        /// Where the picture is: an archive path for an EPUB, an address for an article.
        resource: String,
        /// Its description (`alt`), empty when it has none.
        alt: String,
        caption: Option<Caption>,
    },
}

/// A picture's caption: a block of the text (an EPUB's `figcaption`, read as a paragraph), or
/// text of its own (an article's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caption {
    Block(usize),
    Text(String),
}

/// The package's declared cover image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cover {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// A footnote or endnote body, removed from the reading flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// `"{archive path}#{element id}"`.
    pub id: String,
    /// Plain text of the note without its back-link marker.
    pub text: String,
}

/// Where a note reference marker (e.g. a superscript "1") stood in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteRef {
    /// Index into `Chapter::blocks`.
    pub block: usize,
    /// Char offset in that block's text where the marker was removed.
    pub offset: usize,
    /// The marker's visible label, e.g. "1" or "*".
    pub label: String,
    /// Index into `ParsedBook::notes`; `None` when the target was not found.
    pub note: Option<usize>,
}

/// One visible line of chapter text and the structure it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    pub text: String,
    /// A scene break stands before this block: a rule (`<hr>`) or an empty element styled as
    /// one. It adds no block of its own, so block indexes (and segment ids) stay the same.
    pub break_before: bool,
}

/// Whether a class or type name marks an element as a scene break (`scene-break`,
/// `sectionbreak`, `transition`, ...). A page marker (`pagebreak`) is not one.
pub fn names_scene_break(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "scenebreak",
        "scene-break",
        "scene_break",
        "sectionbreak",
        "section-break",
        "section_break",
        "spacebreak",
        "space-break",
        "space_break",
        "transition",
    ];
    NAMES.iter().any(|known| known.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    ListItem,
    Preformatted,
}

/// An archive document and, when provided by navigation, its starting anchor.
/// A missing fragment identifies the document, not an exact character range.
#[derive(Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: String,
    pub fragment: Option<String>,
}
