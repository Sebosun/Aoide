use aoide_epub::{parse_epub, ParseOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: cargo run --example inspect -- BOOK.epub")?;
    let options = ParseOptions::default();
    if std::fs::metadata(&path)?.len() > options.max_archive_bytes {
        return Err("book exceeds the default archive-size limit".into());
    }
    let book = parse_epub(&std::fs::read(path)?, &options)?;
    println!(
        "Title: {:?}\nChapters: {}\nExcluded: {}\nWarnings: {}",
        book.title,
        book.chapters.len(),
        book.excluded.len(),
        book.warnings.len()
    );
    for (index, chapter) in book.chapters.iter().take(40).enumerate() {
        println!(
            "{:>3}. {} ({} characters, {} source documents)",
            index + 1,
            chapter.title,
            chapter.text.chars().count(),
            chapter.sources.len()
        );
    }
    Ok(())
}
