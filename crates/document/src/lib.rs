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
