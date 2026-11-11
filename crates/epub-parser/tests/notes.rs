mod support;

use aoide_epub::{parse_epub, ExclusionReason, Note, NoteRef, ParseOptions, ParsedBook, Warning};

const OPS: &str = r#"xmlns:epub="http://www.idpf.org/2007/ops""#;

fn single(body: &str) -> ParsedBook {
    parse_epub(
        &support::single(&format!("<html {OPS}><body>{body}</body></html>")),
        &ParseOptions::default(),
    )
    .unwrap()
}

#[test]
fn inline_footnotes_are_detached_and_markers_are_not_in_the_text() {
    let book = single(
        r##"<h1>One</h1><p>The sea<a epub:type="noteref" href="#n1">1</a> was calm.</p>
           <aside epub:type="footnote" id="n1"><p>Mostly calm.</p></aside><p>Next.</p>"##,
    );
    let chapter = &book.chapters[0];
    assert_eq!(chapter.text, "One\n\nThe sea was calm.\n\nNext.");
    assert_eq!(
        book.notes,
        vec![Note {
            id: "OPS/chapter.xhtml#n1".into(),
            text: "Mostly calm.".into()
        }]
    );
    assert_eq!(
        chapter.note_refs,
        vec![NoteRef {
            block: 1,
            offset: 7,
            label: "1".into(),
            note: Some(0)
        }]
    );
    assert!(
        !book.warnings.iter().any(|w| matches!(
            w,
            Warning::UnresolvedNoteRef { .. } | Warning::UnreferencedNote { .. }
        )),
        "{:?}",
        book.warnings
    );
}

#[test]
fn aria_roles_and_backlinks_inside_notes_are_handled() {
    let book = single(
        r##"<p>Waves<a role="doc-noteref" href="#fn">*</a>.</p>
           <div role="doc-footnote" id="fn"><p><a href="#back">*</a> Small ones.</p></div>"##,
    );
    assert_eq!(book.chapters[0].text, "Waves.");
    assert_eq!(book.notes[0].text, "Small ones.");
    assert_eq!(book.chapters[0].note_refs[0].label, "*");
    assert_eq!(book.chapters[0].note_refs[0].offset, 5);
}

#[test]
fn superscript_links_into_an_endnotes_document_resolve_and_the_notes_are_back_matter() {
    let bytes = support::epub(
        r#"<item id="c" href="c.xhtml" media-type="application/xhtml+xml"/><item id="n" href="notes.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="c"/><itemref idref="n"/>"#,
        &[
            (
                "OPS/c.xhtml",
                r#"<h1>One</h1><p>The harbour<sup><a href="notes.xhtml#e1" id="r1">2</a></sup> was quiet. See <a href="notes.xhtml">the notes section</a>.</p>"#,
            ),
            (
                "OPS/notes.xhtml",
                &format!(
                    r##"<html {OPS}><body><section epub:type="endnotes"><h1>Notes</h1><ol><li id="e1"><p>Built in 1820. <a href="c.xhtml#r1">↩</a></p></li></ol></section></body></html>"##
                ),
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(
        book.chapters[0].text,
        "One\n\nThe harbour was quiet. See the notes section."
    );
    assert_eq!(
        book.notes,
        vec![Note {
            id: "OPS/notes.xhtml#e1".into(),
            text: "Built in 1820.".into()
        }]
    );
    assert_eq!(book.chapters[0].note_refs[0].note, Some(0));
    assert_eq!(book.chapters[0].note_refs[0].offset, 11);
    assert_eq!(book.excluded[0].reason, ExclusionReason::Notes);
}

#[test]
fn unresolved_references_are_removed_with_a_warning() {
    let book = single(r##"<p>Rope<a epub:type="noteref" href="#gone">3</a> held.</p>"##);
    assert_eq!(book.chapters[0].text, "Rope held.");
    assert_eq!(book.chapters[0].note_refs[0].note, None);
    assert!(book.warnings.contains(&Warning::UnresolvedNoteRef {
        path: "OPS/chapter.xhtml".into(),
        target: "OPS/chapter.xhtml#gone".into(),
    }));
}

#[test]
fn unreferenced_notes_are_kept_in_the_notes_list_with_a_warning() {
    let book = single(
        r#"<p>Plain.</p><aside epub:type="footnote" id="lonely"><p>Orphan note.</p></aside>"#,
    );
    assert_eq!(book.chapters[0].text, "Plain.");
    assert_eq!(book.notes[0].text, "Orphan note.");
    assert!(book.warnings.contains(&Warning::UnreferencedNote {
        id: "OPS/chapter.xhtml#lonely".into()
    }));
}

#[test]
fn ordinary_links_and_superscripts_stay_in_the_text() {
    let book = single(
        r##"<p>E = mc<sup>2</sup> and <a href="#top">back to top</a>.</p><p id="top">End.</p>"##,
    );
    assert_eq!(book.chapters[0].text, "E = mc2 and back to top.\n\nEnd.");
    assert!(book.chapters[0].note_refs.is_empty());
    assert!(book.notes.is_empty());
}

#[test]
fn plain_links_to_a_note_in_another_document_are_markers_even_without_sup() {
    // Some layout tools set the marker in superscript with a class instead of <sup>, and only
    // the note itself carries epub:type.
    let bytes = support::epub(
        r#"<item id="c" href="c.xhtml" media-type="application/xhtml+xml"/><item id="n" href="notes.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="c"/><itemref idref="n"/>"#,
        &[
            (
                "OPS/c.xhtml",
                r#"<h1>One</h1><p>The gate was shut.”<span class="raised"><span id="fn-7-back"><a class="notelink" href="notes.xhtml#fn-7">83</a></span></span> The keeper slept. See <a href="notes.xhtml#fn-7">the note on gates</a>.</p>"#,
            ),
            (
                "OPS/notes.xhtml",
                &format!(
                    r##"<html {OPS}><body><div id="fn-7" epub:type="footnote"><p><a class="notelink" href="c.xhtml#fn-7-back">83</a> Gates were shut at dusk.</p></div></body></html>"##
                ),
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters[0].text,
        "One\n\nThe gate was shut.” The keeper slept. See the note on gates."
    );
    assert_eq!(book.notes[0].text, "Gates were shut at dusk.");
    assert_eq!(
        book.chapters[0].note_refs.len(),
        1,
        "{:?}",
        book.chapters[0].note_refs
    );
    assert_eq!(book.chapters[0].note_refs[0].label, "83");
    assert_eq!(book.chapters[0].note_refs[0].note, Some(0));
}
