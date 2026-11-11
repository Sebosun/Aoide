//! Deterministic, offline EPUB chapter extraction, independent of Tauri and TTS.
mod archive;
mod boundaries;
mod chapters;
mod content;
mod filter;
mod location;
mod model;
mod navigation;
mod package;

pub use model::*;

/// Reads only the package metadata and cover (no chapters): a fast look at a book.
pub fn read_metadata(bytes: &[u8], options: &ParseOptions) -> Result<BookMetadata, ParseError> {
    let mut archive = archive::Archive::open(bytes, options)?;
    let package = package::read(&mut archive)?;
    Ok(BookMetadata {
        isbn: package.identifiers.iter().find_map(|id| package::isbn(id)),
        title: package.title,
        authors: package.authors,
        language: package.language,
        publisher: package.publisher,
        description: package.description,
        cover: package.cover,
    })
}

/// Parse a complete EPUB archive without writing files or making network calls.
/// Ambiguous content is preserved; inferred chapters and exclusions are inspectable.
pub fn parse_epub(bytes: &[u8], options: &ParseOptions) -> Result<ParsedBook, ParseError> {
    let mut archive = archive::Archive::open(bytes, options)?;
    let package = package::read(&mut archive)?;
    if package.spine.is_empty() {
        return Err(ParseError::Invalid("empty spine".into()));
    }
    let navigation = navigation::read(&mut archive, package.navigation.as_ref())?;
    let mut extraction = chapters::extract(&mut archive, &package, &navigation)?;
    let (chapters, excluded) =
        filter::partition(extraction.sections, options, &mut extraction.warnings);
    Ok(ParsedBook {
        title: package.title,
        isbn: package.identifiers.iter().find_map(|id| package::isbn(id)),
        authors: package.authors,
        language: package.language,
        publisher: package.publisher,
        description: package.description,
        identifiers: package.identifiers,
        cover: package.cover,
        notes: extraction.notes,
        chapters,
        warnings: extraction.warnings,
        excluded,
    })
}
