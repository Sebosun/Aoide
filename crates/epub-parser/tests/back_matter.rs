mod support;

use aoide_epub::{parse_epub, ExclusionReason, ParseOptions, Warning};

fn book(sections: &[(&str, &str)]) -> aoide_epub::ParsedBook {
    let manifest: String = sections
        .iter()
        .enumerate()
        .map(|(i, _)| {
            format!(r#"<item id="s{i}" href="s{i}.xhtml" media-type="application/xhtml+xml"/>"#)
        })
        .collect();
    let spine: String = (0..sections.len())
        .map(|i| format!(r#"<itemref idref="s{i}"/>"#))
        .collect();
    let files: Vec<(String, String)> = sections
        .iter()
        .enumerate()
        .map(|(i, (ty, body))| {
            let attr = if ty.is_empty() { String::new() } else { format!(r#" epub:type="{ty}""#) };
            (
                format!("OPS/s{i}.xhtml"),
                format!(r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><section{attr}>{body}</section></body></html>"#),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_str()))
        .collect();
    parse_epub(
        &support::epub(&manifest, &spine, &refs),
        &ParseOptions::default(),
    )
    .unwrap()
}

#[test]
fn structurally_marked_back_matter_is_excluded_with_reasons() {
    let parsed = book(&[
        ("chapter", "<h1>One</h1><p>The story.</p>"),
        ("afterword", "<h1>Afterword</h1><p>Kept words.</p>"),
        ("appendix", "<h1>Appendix</h1><p>Kept table.</p>"),
        ("endnotes", "<h1>Notes</h1><p>1. A note.</p>"),
        ("bibliography", "<h1>Sources</h1><p>A book.</p>"),
        ("glossary", "<h1>Words</h1><p>mare: sea</p>"),
        ("index", "<h1>Index</h1><p>boat, 3</p>"),
        ("colophon", "<h1>Colophon</h1><p>Set in type.</p>"),
    ]);
    let kept: Vec<&str> = parsed.chapters.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(kept, vec!["One", "Afterword", "Appendix"]);
    let reasons: Vec<ExclusionReason> = parsed.excluded.iter().map(|e| e.reason).collect();
    assert_eq!(
        reasons,
        vec![
            ExclusionReason::Notes,
            ExclusionReason::Bibliography,
            ExclusionReason::Glossary,
            ExclusionReason::Index,
            ExclusionReason::Colophon,
        ]
    );
}

#[test]
fn back_matter_titles_without_markup_are_kept_with_a_warning() {
    let parsed = book(&[
        ("", "<h1>One</h1><p>The story.</p>"),
        ("", "<h1>Index</h1><p>boat, 3</p>"),
        ("", "<h1>About the Author</h1><p>Lives by the sea.</p>"),
    ]);
    assert_eq!(parsed.chapters.len(), 3);
    for title in ["Index", "About the Author"] {
        assert!(
            parsed.warnings.contains(&Warning::AmbiguousFrontMatter {
                title: title.into()
            }),
            "{title}: {:?}",
            parsed.warnings
        );
    }
}
