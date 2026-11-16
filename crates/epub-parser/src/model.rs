pub use aoide_document::*;

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
    /// Its subjects as the package lists them (`dc:subject`), for tags.
    pub subjects: Vec<String>,
    pub cover: Option<Cover>,
}

/// The section kind an `epub:type` or landmark token names.
pub(crate) fn exclusion_from_token(value: &str) -> Option<ExclusionReason> {
    match value {
        "cover" | "doc-cover" => Some(ExclusionReason::Cover),
        "titlepage" | "title-page" => Some(ExclusionReason::TitlePage),
        "copyright-page" => Some(ExclusionReason::Copyright),
        "toc" | "doc-toc" => Some(ExclusionReason::Contents),
        "preface" | "doc-preface" => Some(ExclusionReason::Preface),
        "introduction" | "doc-introduction" => Some(ExclusionReason::Introduction),
        "dedication" | "doc-dedication" => Some(ExclusionReason::Dedication),
        "acknowledgments" | "doc-acknowledgments" => Some(ExclusionReason::Acknowledgments),
        "endnotes" | "doc-endnotes" | "rearnotes" | "footnotes" | "notes" => {
            Some(ExclusionReason::Notes)
        }
        "bibliography" | "doc-bibliography" => Some(ExclusionReason::Bibliography),
        "glossary" | "doc-glossary" => Some(ExclusionReason::Glossary),
        "index" | "doc-index" => Some(ExclusionReason::Index),
        "colophon" | "doc-colophon" => Some(ExclusionReason::Colophon),
        "credits" | "doc-credits" | "other-credits" | "contributors" => {
            Some(ExclusionReason::Credits)
        }
        _ => None,
    }
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
