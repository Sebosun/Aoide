/// Resource budgets apply before any book content is decompressed or parsed.
#[derive(Debug)]
pub struct ParseOptions {
    pub exclude_front_matter: bool,
    pub max_archive_bytes: u64,
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_uncompressed_bytes: u64,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            exclude_front_matter: true,
            max_archive_bytes: 64 * 1024 * 1024,
            max_entries: 10_000,
            max_entry_bytes: 16 * 1024 * 1024,
            max_uncompressed_bytes: 128 * 1024 * 1024,
        }
    }
}

/// Package metadata only, from [`crate::read_metadata`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub isbn: Option<String>,
    pub cover: Option<Cover>,
}

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
    pub cover: Option<Cover>,
    /// Footnote/endnote bodies detached from the narration, in document order.
    pub notes: Vec<Note>,
    pub chapters: Vec<Chapter>,
    pub warnings: Vec<Warning>,
    pub excluded: Vec<ExcludedSection>,
}

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
}

impl ExclusionReason {
    pub(crate) fn from_token(value: &str) -> Option<Self> {
        match value {
            "cover" | "doc-cover" => Some(Self::Cover),
            "titlepage" | "title-page" => Some(Self::TitlePage),
            "copyright-page" => Some(Self::Copyright),
            "toc" | "doc-toc" => Some(Self::Contents),
            "preface" | "doc-preface" => Some(Self::Preface),
            "introduction" | "doc-introduction" => Some(Self::Introduction),
            "dedication" | "doc-dedication" => Some(Self::Dedication),
            "acknowledgments" | "doc-acknowledgments" => Some(Self::Acknowledgments),
            "endnotes" | "doc-endnotes" | "rearnotes" | "footnotes" | "notes" => Some(Self::Notes),
            "bibliography" | "doc-bibliography" => Some(Self::Bibliography),
            "glossary" | "doc-glossary" => Some(Self::Glossary),
            "index" | "doc-index" => Some(Self::Index),
            "colophon" | "doc-colophon" => Some(Self::Colophon),
            "credits" | "doc-credits" | "other-credits" | "contributors" => Some(Self::Credits),
            _ => None,
        }
    }
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

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("invalid EPUB: {0}")]
    Invalid(String),
    #[error("unsafe archive path: {0}")]
    UnsafePath(String),
    #[error("EPUB exceeds limit: {0}")]
    LimitExceeded(&'static str),
    #[error("encrypted book content is unsupported: {0}")]
    UnsupportedEncryption(String),
}
