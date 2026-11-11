use crate::{chapters::ReadingSection, Chapter, ExcludedSection, ParseOptions, Warning};

pub(crate) fn partition(
    sections: Vec<ReadingSection>,
    options: &ParseOptions,
    warnings: &mut Vec<Warning>,
) -> (Vec<Chapter>, Vec<ExcludedSection>) {
    let mut included = Vec::new();
    let mut excluded = Vec::new();
    for section in sections {
        let chapter = section.chapter;
        if let Some(reason) = section.reason.filter(|_| options.exclude_front_matter) {
            excluded.push(ExcludedSection { chapter, reason });
        } else if chapter.text.is_empty() {
            warnings.push(Warning::EmptySection {
                path: chapter.sources[0].path.clone(),
            });
        } else {
            if section.reason.is_none() && ambiguous_title(&chapter.title) {
                warnings.push(Warning::AmbiguousFrontMatter {
                    title: chapter.title.clone(),
                });
            }
            included.push(chapter);
        }
    }
    (included, excluded)
}

fn ambiguous_title(title: &str) -> bool {
    matches!(
        title.trim().to_ascii_lowercase().as_str(),
        "introduction"
            | "preface"
            | "foreword"
            | "cover"
            | "title page"
            | "copyright"
            | "contents"
            | "table of contents"
            | "dedication"
            | "acknowledgments"
            | "index"
            | "notes"
            | "endnotes"
            | "bibliography"
            | "glossary"
            | "colophon"
            | "about the author"
            | "also by the same author"
    )
}
