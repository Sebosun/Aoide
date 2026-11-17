//! Front and back matter told by chapter title: the chapters most listeners leave out of the
//! narration (the copyright page, the contents, the index, the notes). A proposal only; the
//! titles are matched whole, so a chapter that merely begins with "Notes" is not proposed.

/// Titles matched whole, after [`normalize`].
struct Titles {
    whole: &'static [&'static str],
    /// Titles that are front or back matter whatever follows them ("Also by Mira Vale").
    leading: &'static [&'static str],
}

const ENGLISH: Titles = Titles {
    whole: &[
        "copyright",
        "copyright page",
        "contents",
        "table of contents",
        "acknowledgements",
        "acknowledgments",
        "notes",
        "endnotes",
        "bibliography",
        "references",
        "further reading",
        "index",
        "about the author",
        "about the authors",
        "title page",
        "cover",
        "credits",
        "colophon",
    ],
    leading: &["also by", "other books by", "books by", "praise for"],
};

const GERMAN: Titles = Titles {
    whole: &[
        "impressum",
        "inhalt",
        "inhaltsverzeichnis",
        "danksagung",
        "dank",
        "register",
        "anmerkungen",
        "literatur",
        "literaturverzeichnis",
        "über den autor",
        "über die autorin",
        "über die autoren",
    ],
    leading: &[
        "von derselben autorin",
        "vom selben autor",
        "weitere bücher von",
    ],
};

const FRENCH: Titles = Titles {
    whole: &[
        "table des matières",
        "sommaire",
        "remerciements",
        "bibliographie",
        "à propos de l auteur",
        "à propos de l autrice",
    ],
    leading: &["du même auteur", "de la même autrice", "du même autrice"],
};

const SPANISH: Titles = Titles {
    whole: &[
        "índice",
        "agradecimientos",
        "notas",
        "bibliografía",
        "sobre el autor",
        "sobre la autora",
        "créditos",
    ],
    leading: &["del mismo autor", "de la misma autora", "otros libros de"],
};

/// The indexes of the chapters whose titles name front or back matter, in order. English
/// titles are known in every book (many carry an English copyright page); `language` (BCP-47)
/// adds its own where Aoide has a list for it.
pub fn front_and_back_matter(titles: &[&str], language: Option<&str>) -> Vec<usize> {
    let primary = language
        .and_then(|l| l.split(['-', '_']).next())
        .map(str::to_lowercase);
    let mut lists = vec![&ENGLISH];
    match primary.as_deref() {
        Some("de") => lists.push(&GERMAN),
        Some("fr") => lists.push(&FRENCH),
        Some("es") => lists.push(&SPANISH),
        _ => {}
    }
    titles
        .iter()
        .enumerate()
        .filter(|(_, title)| {
            let title = normalize(title);
            lists.iter().any(|list| {
                list.whole.contains(&title.as_str())
                    || list.leading.iter().any(|lead| {
                        title == *lead
                            || title
                                .strip_prefix(lead)
                                .is_some_and(|rest| rest.starts_with(' '))
                    })
            })
        })
        .map(|(index, _)| index)
        .collect()
}

/// Lower case, with every run of characters that are not letters or digits made one space.
fn normalize(title: &str) -> String {
    title
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
