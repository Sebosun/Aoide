//! Regression tests over crawled pages: each folder under `tests/pages/<corpus>/` holds a page as
//! it was fetched (`source.html`, and its address in `url` when known) and what Aoide reads from
//! it: the text (`expected.txt`) and everything else (`expected.json`). The expectations are
//! Aoide's own output, reviewed: a change shows up as a diff to read before accepting it.
//!
//! `AOIDE_WRITE_PAGES=1 cargo test -p aoide-article --test pages` writes them afresh;
//! `AOIDE_PAGE=<name>` limits a run to the folders whose name contains it. `tests/pages/README.md`
//! says where the pages come from, how to add one and what was reviewed.

use std::path::{Path, PathBuf};

use aoide_article::{extract, ExtractOptions, Extracted};
use aoide_document::BlockKind;
use url::Url;

/// The address pages are read as when the crawl did not keep theirs (as Readability's tests do).
const FAKE_URL: &str = "http://fakehost/test/page.html";

fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/pages");
    let only = std::env::var("AOIDE_PAGE").ok();
    let mut cases = Vec::new();
    for corpus in std::fs::read_dir(&root).unwrap() {
        let corpus = corpus.unwrap().path();
        if !corpus.is_dir() {
            continue;
        }
        for case in std::fs::read_dir(&corpus).unwrap() {
            let case = case.unwrap().path();
            let name = case.file_name().unwrap().to_string_lossy().into_owned();
            if case.join("source.html").is_file() && only.iter().all(|o| name.contains(o.as_str()))
            {
                cases.push(case);
            }
        }
    }
    cases.sort();
    cases
}

fn url_of(case: &Path) -> Url {
    let text = std::fs::read_to_string(case.join("url")).unwrap_or_else(|_| FAKE_URL.into());
    Url::parse(text.trim()).unwrap()
}

/// The text as a reader reviews it: headings marked with `#`, list items with `-`, preformatted
/// text fenced, blocks separated by a blank line.
fn text(extracted: &Extracted) -> String {
    let blocks = &extracted.document.chapters[0].blocks;
    let mut out: Vec<String> = blocks
        .iter()
        .map(|b| match b.kind {
            BlockKind::Heading { level } => format!("{} {}", "#".repeat(level.into()), b.text),
            BlockKind::ListItem => format!("- {}", b.text),
            BlockKind::Preformatted => format!("```\n{}\n```", b.text),
            BlockKind::Paragraph => b.text.clone(),
        })
        .collect();
    out.push(String::new());
    out.join("\n\n")
}

fn details(extracted: &Extracted) -> String {
    let meta = &extracted.meta;
    let document = &extracted.document;
    let value = serde_json::json!({
        "title": meta.title,
        "byline": meta.byline,
        "siteName": meta.site_name,
        "published": meta.published.as_ref().map(|p| serde_json::json!({"text": p.text, "atMs": p.at_ms})),
        "language": meta.language,
        "canonicalUrl": meta.canonical_url.as_ref().map(Url::as_str),
        "leadImage": meta.lead_image.as_ref().map(Url::as_str),
        "description": document.description,
        "wordCount": meta.word_count,
        "declaredWords": meta.declared_words,
        "completeness": format!("{:?}", extracted.completeness),
        "feeds": extracted.feeds.iter().map(|f| serde_json::json!({"url": f.url.as_str(), "title": f.title})).collect::<Vec<_>>(),
        "warnings": document.warnings.iter().map(|w| format!("{w:?}")).collect::<Vec<_>>(),
        "pictures": document.chapters.iter().flat_map(|c| &c.inserts).filter(|i| matches!(i.kind, aoide_document::InsertKind::Picture { .. })).count(),
        // Tables kept beside the text: where, and how many rows of how many cells.
        "tables": document.chapters.iter().flat_map(|c| &c.inserts).filter_map(|insert| match &insert.kind {
            aoide_document::InsertKind::Table { rows, header_rows, .. } => Some(format!(
                "before block {}: {} rows ({} heading) of {} cells",
                insert.before_block,
                rows.len(),
                header_rows,
                rows.iter().map(Vec::len).max().unwrap_or(0)
            )),
            _ => None,
        }).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&value).unwrap() + "\n"
}

/// The first lines where `actual` and `expected` part, for the failure message.
fn first_difference(actual: &str, expected: &str) -> String {
    let (a, e): (Vec<&str>, Vec<&str>) = (actual.lines().collect(), expected.lines().collect());
    let at = a
        .iter()
        .zip(&e)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(e.len()));
    let line = |lines: &[&str]| {
        lines
            .get(at)
            .map_or("(end)".to_owned(), |l| l.chars().take(160).collect())
    };
    format!(
        "line {}:\n      got:  {}\n      want: {}",
        at + 1,
        line(&a),
        line(&e)
    )
}

#[test]
fn every_crawled_page_reads_as_reviewed() {
    let write = std::env::var_os("AOIDE_WRITE_PAGES").is_some();
    let cases = cases();
    assert!(!cases.is_empty(), "no pages found");
    let mut failures = Vec::new();
    for case in &cases {
        let name = case
            .strip_prefix(case.parent().unwrap().parent().unwrap())
            .unwrap()
            .display()
            .to_string();
        let html =
            String::from_utf8_lossy(&std::fs::read(case.join("source.html")).unwrap()).into_owned();
        let extracted = extract(&html, &url_of(case), &ExtractOptions::default());
        // The same page reads the same every time.
        assert_eq!(
            extracted,
            extract(&html, &url_of(case), &ExtractOptions::default()),
            "{name}"
        );
        for (file, actual) in [
            ("expected.txt", text(&extracted)),
            ("expected.json", details(&extracted)),
        ] {
            let path = case.join(file);
            if write {
                std::fs::write(&path, &actual).unwrap();
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(expected) if expected == actual => {}
                Ok(expected) => failures.push(format!(
                    "{name}/{file}: {}",
                    first_difference(&actual, &expected)
                )),
                Err(_) => failures.push(format!(
                    "{name}/{file}: missing (AOIDE_WRITE_PAGES=1 writes it)"
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} pages read differently from what was reviewed; read the diff (git diff after \
         AOIDE_WRITE_PAGES=1) before accepting it:\n  {}",
        failures
            .iter()
            .map(|f| f.split('/').take(2).collect::<Vec<_>>())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        cases.len(),
        failures.join("\n  ")
    );
}
