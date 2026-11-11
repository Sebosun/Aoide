mod support;

use aoide_epub::{parse_epub, ExclusionReason, ParseOptions, SourceLocation, Warning};

#[test]
fn explicit_body_matter_is_not_excluded_by_an_earlier_introduction_guide_marker() {
    let bytes = support::epub_with_guide(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        r#"<reference type="introduction" title="Introduction" href="chapter.xhtml#intro"/>"#,
        &[(
            "OPS/chapter.xhtml",
            r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><section epub:type="introduction" id="intro"><p>About this book.</p></section><section epub:type="bodymatter"><p>Keep the actual story.</p></section></body></html>"#,
        )],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].text, "Keep the actual story.");
    assert_eq!(book.excluded[0].chapter.text, "About this book.");
}

#[test]
fn resolves_legacy_named_anchors_in_epub2_documents() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>The Journey</text></navLabel><content src="chapter.xhtml#start"/></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<a name="start"></a><p>The story begins.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters[0].title, "The Journey");
    assert_eq!(book.chapters[0].text, "The story begins.");
    assert!(!book
        .warnings
        .iter()
        .any(|w| matches!(w, Warning::NavigationTargetMissing { .. })));
}

#[test]
fn keeps_table_cells_separate_without_joining_unrelated_words() {
    let bytes = support::single("<h1>Vocabulary</h1><table><tr><th>Latin</th><th>English</th></tr><tr><td>mare</td><td>sea</td></tr></table>");
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters[0].text,
        "Vocabulary\n\nLatin English\n\nmare sea"
    );
}

#[test]
fn extracts_metadata_and_readable_chapter_from_a_minimal_epub() {
    let bytes = support::single(
        "<html><body><h1>The Arrival</h1><p>A boat reached the shore.</p></body></html>",
    );

    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();

    assert_eq!(book.title.as_deref(), Some("A Small Journey"));
    assert_eq!(book.authors, vec!["Test Author"]);
    assert_eq!(book.language.as_deref(), Some("en"));
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].title, "The Arrival");
    assert_eq!(
        book.chapters[0].text,
        "The Arrival\n\nA boat reached the shore."
    );
}

#[test]
fn allows_front_matter_to_be_kept_explicitly() {
    let bytes = support::single(
        r#"<section role="doc-preface"><h1>Before we begin</h1><p>Keep this preface.</p></section>"#,
    );
    let book = parse_epub(
        &bytes,
        &ParseOptions {
            exclude_front_matter: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(book.excluded.is_empty());
    assert_eq!(
        book.chapters[0].text,
        "Before we begin\n\nKeep this preface."
    );
}

#[test]
fn reports_each_supported_structural_front_matter_kind() {
    for (token, reason) in [
        ("cover", ExclusionReason::Cover),
        ("titlepage", ExclusionReason::TitlePage),
        ("copyright-page", ExclusionReason::Copyright),
        ("toc", ExclusionReason::Contents),
        ("preface", ExclusionReason::Preface),
        ("introduction", ExclusionReason::Introduction),
        ("dedication", ExclusionReason::Dedication),
        ("acknowledgments", ExclusionReason::Acknowledgments),
    ] {
        let bytes = support::single(&format!(
            r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body epub:type="{token}"><p>Some front matter.</p></body></html>"#
        ));
        let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
        assert!(book.chapters.is_empty(), "kept {token}");
        assert_eq!(book.excluded.len(), 1, "did not report {token}");
        assert_eq!(book.excluded[0].reason, reason);
    }
}

#[test]
fn duplicate_navigation_targets_do_not_duplicate_text_or_create_empty_chapters() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>The Journey</text></navLabel><content src="chapter.xhtml#start"/></navPoint><navPoint><navLabel><text>Another label</text></navLabel><content src="chapter.xhtml#start"/></navPoint></navMap></ncx>"##,
            ),
            ("OPS/chapter.xhtml", r#"<p id="start">The whole story.</p>"#),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].title, "The Journey");
    assert_eq!(book.chapters[0].text, "The whole story.");
    assert_eq!(book, parse_epub(&bytes, &ParseOptions::default()).unwrap());
}

#[test]
fn attaches_leading_continuation_text_before_the_next_documents_chapter_anchor() {
    let bytes = support::epub(
        r#"<item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="one"/><itemref idref="two"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>Departure</text></navLabel><content src="one.xhtml"/></navPoint><navPoint><navLabel><text>Arrival</text></navLabel><content src="two.xhtml#arrival"/></navPoint></navMap></ncx>"##,
            ),
            ("OPS/one.xhtml", "<p>The boat left.</p>"),
            (
                "OPS/two.xhtml",
                r#"<p>It crossed the lake.</p><p id="arrival">It reached the shore.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 2);
    assert_eq!(
        book.chapters[0].text,
        "The boat left.\n\nIt crossed the lake."
    );
    assert_eq!(book.chapters[0].sources.len(), 2);
    assert_eq!(book.chapters[1].text, "It reached the shore.");
}

#[test]
fn filters_structurally_marked_front_matter_even_without_heading_elements() {
    let bytes = support::single(
        r#"<html><body><section role="doc-introduction"><p>About this edition.</p></section><p>The story begins here.</p></body></html>"#,
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].text, "The story begins here.");
    assert_eq!(book.excluded.len(), 1);
    assert_eq!(book.excluded[0].reason, ExclusionReason::Introduction);
    assert_eq!(book.excluded[0].chapter.text, "About this edition.");
}

#[test]
fn uses_epub3_landmarks_to_filter_a_preface_and_keeps_the_toc_chapter_title() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/nav.xhtml",
                r##"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="landmarks"><ol><li><a epub:type="preface" href="chapter.xhtml#preface">Before reading</a></li><li><a epub:type="bodymatter" href="chapter.xhtml#story">Start reading</a></li></ol></nav><nav epub:type="toc"><ol><li><a href="chapter.xhtml#preface">Preface</a></li><li><a href="chapter.xhtml#story">The Journey</a></li></ol></nav></body></html>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<p id="preface">About this edition.</p><p id="story">A boat set sail.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].title, "The Journey");
    assert_eq!(book.chapters[0].text, "A boat set sail.");
    assert_eq!(
        book.chapters[0].sources[0],
        SourceLocation {
            path: "OPS/chapter.xhtml".into(),
            fragment: Some("story".into())
        }
    );
    assert_eq!(book.excluded[0].reason, ExclusionReason::Preface);
}

#[test]
fn uses_epub2_guide_markers_to_exclude_cover_and_contents_without_swallowing_the_story() {
    let bytes = support::epub_with_guide(
        r#"<item id="cover" href="cover.xhtml" media-type="application/xhtml+xml"/><item id="tocpage" href="contents.xhtml" media-type="application/xhtml+xml"/><item id="story" href="story.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="cover"/><itemref idref="tocpage"/><itemref idref="story"/>"#,
        r#"<reference type="cover" title="Cover" href="cover.xhtml"/><reference type="toc" title="Contents" href="contents.xhtml"/>"#,
        &[
            ("OPS/cover.xhtml", "<img src='cover.jpg'/>"),
            ("OPS/contents.xhtml", "<p>A printed contents list.</p>"),
            ("OPS/story.xhtml", "<p>The actual story survives.</p>"),
            (
                "OPS/toc.ncx",
                r#"<ncx><navMap><navPoint><navLabel><text>Cover</text></navLabel><content src="cover.xhtml"/></navPoint></navMap></ncx>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].text, "The actual story survives.");
    assert_eq!(
        book.excluded.iter().map(|s| s.reason).collect::<Vec<_>>(),
        [ExclusionReason::Cover, ExclusionReason::Contents]
    );
}

#[test]
fn does_not_promote_unlisted_subsection_headings_to_chapters() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>Chapter One</text></navLabel><content src="chapter.xhtml"/><navPoint><navLabel><text>1.1 Basics</text></navLabel><content src="chapter.xhtml#basics"/></navPoint></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<p>Chapter One</p><h1 id="basics">1.1 Basics</h1><p>Some basics.</p><h1>1.2 More</h1><p>Still chapter one.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(
        book.chapters[0].text,
        "Chapter One\n\n1.1 Basics\n\nSome basics.\n\n1.2 More\n\nStill chapter one."
    );
}

#[test]
fn reports_image_only_documents_instead_of_creating_empty_audiobook_chapters() {
    let bytes =
        support::single("<html><body><img src='cover.jpg' alt='Cover image'/></body></html>");
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert!(book.chapters.is_empty());
    assert!(book.warnings.contains(&Warning::EmptySection {
        path: "OPS/chapter.xhtml".into()
    }));
}

#[test]
fn accepts_external_ncx_doctypes_without_loading_external_resources() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "https://invalid.example/ncx.dtd"><n:ncx xmlns:n="http://www.daisy.org/z3986/2005/ncx/"><n:navMap><n:navPoint><n:navLabel><n:text>Named in the TOC</n:text></n:navLabel><n:content src="chapter.xhtml#start"/></n:navPoint></n:navMap></n:ncx>"##,
            ),
            ("OPS/chapter.xhtml", r#"<p id="start">A short story.</p>"#),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters[0].title, "Named in the TOC");
    assert!(!book.warnings.contains(&Warning::MissingNavigation));
}

#[test]
fn warns_about_broken_navigation_anchors_and_retains_all_readable_content() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>Missing</text></navLabel><content src="chapter.xhtml#absent"/></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/chapter.xhtml",
                "<h1>Real chapter</h1><p>Keep every word.</p>",
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters[0].text, "Real chapter\n\nKeep every word.");
    assert!(book.warnings.contains(&Warning::NavigationTargetMissing {
        path: "OPS/chapter.xhtml".into(),
        fragment: Some("absent".into())
    }));
}

#[test]
fn resolves_relative_percent_encoded_package_and_navigation_references() {
    let bytes = support::epub(
        r#"<item id="chapter" href="Text/chapter%201.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="Nav/toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/Nav/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>A New Day</text></navLabel><content src="../Text/chapter%201.xhtml#new%20day"/></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/Text/chapter 1.xhtml",
                r#"<p id="new day">The sun rose.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters[0].title, "A New Day");
    assert_eq!(book.chapters[0].text, "The sun rose.");
}

#[test]
fn retains_an_ambiguous_introduction_title_and_reports_it_for_review() {
    let bytes = support::single("<h1>Introduction</h1><p>This is the real first chapter.</p><h1>An Introduction to Sailing</h1><p>This is also real content.</p>");
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 2);
    assert!(book.excluded.is_empty());
    assert!(book.warnings.contains(&Warning::AmbiguousFrontMatter {
        title: "Introduction".into()
    }));
    assert!(!book.warnings.contains(&Warning::AmbiguousFrontMatter {
        title: "An Introduction to Sailing".into()
    }));
}

#[test]
fn excludes_structurally_marked_introduction_without_losing_the_following_chapter() {
    let bytes = support::single(
        r#"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><section epub:type="introduction"><h1>Editor's introduction</h1><p>About this edition.</p></section><section epub:type="bodymatter"><h1>Chapter One</h1><p>The actual story.</p></section></body></html>"#,
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1);
    assert_eq!(book.chapters[0].text, "Chapter One\n\nThe actual story.");
    assert_eq!(book.excluded.len(), 1);
    assert_eq!(book.excluded[0].reason, ExclusionReason::Introduction);
    assert_eq!(
        book.excluded[0].chapter.text,
        "Editor's introduction\n\nAbout this edition."
    );
}

#[test]
fn recovers_missing_peer_chapters_from_an_incomplete_toc() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>I</text></navLabel><content src="chapter.xhtml#one"/></navPoint><navPoint><navLabel><text>III</text></navLabel><content src="chapter.xhtml#three"/></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<h1 id="one">I</h1><p>First.</p><h2>A detail</h2><p>Also first.</p><h1>II</h1><p>Second.</p><h1 id="three">III</h1><p>Third.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["I", "II", "III"]
    );
    assert_eq!(book.chapters[1].text, "II\n\nSecond.");
    assert!(book.warnings.contains(&Warning::InferredChapter {
        path: "OPS/chapter.xhtml".into(),
        title: "II".into()
    }));
}

#[test]
fn falls_back_to_peer_headings_and_preserves_unlabelled_leading_text() {
    let bytes = support::single("<p>A prologue without a label.</p><h1>Departure</h1><p>We left.</p><h2>A detail</h2><p>Still leaving.</p><h1>Arrival</h1><p>We arrived.</p>");
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["Untitled section", "Departure", "Arrival"]
    );
    assert_eq!(book.chapters[0].text, "A prologue without a label.");
    assert_eq!(
        book.chapters[1].text,
        "Departure\n\nWe left.\n\nA detail\n\nStill leaving."
    );
    assert!(book.warnings.contains(&Warning::MissingNavigation));
    assert!(book.warnings.contains(&Warning::InferredChapter {
        path: "OPS/chapter.xhtml".into(),
        title: "Arrival".into()
    }));
}

#[test]
fn extracts_visible_unicode_text_without_markup_noise_or_duplicate_nested_blocks() {
    let bytes = support::single(
        r#"<!doctype html><html><head><title>Not spoken</title><style>p { color: red }</style></head><body>
        <h1>Café &amp; 海</h1><nav>Skip navigation</nav><script>not spoken()</script>
        <p>“Hello,” <em>Zoë</em> said.
        A &lt; B &amp; B &gt; C.</p>
        <div hidden>Hidden</div><p aria-hidden="true">Also hidden</p><span style="display: none">Invisible</span>
        <div><p>A second paragraph.</p></div><ul><li>One <strong>item</strong>.</li></ul>
    </body></html>"#,
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters[0].title, "Café & 海");
    assert_eq!(
        book.chapters[0].text,
        "Café & 海\n\n“Hello,” Zoë said. A < B & B > C.\n\nA second paragraph.\n\nOne item."
    );
}

#[test]
fn uses_epub3_toc_not_page_lists_and_keeps_nested_sections_together() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/nav.xhtml",
                r##"<html xmlns:epub="http://www.idpf.org/2007/ops"><body><nav epub:type="page-list"><ol><li><a href="chapter.xhtml#detail">Page 2</a></li></ol></nav><nav epub:type="toc"><ol><li><a href="chapter.xhtml#start">The River</a><ol><li><a href="chapter.xhtml#detail">A Crossing</a></li></ol></li><li><a href="chapter.xhtml#end">The Sea</a></li></ol></nav></body></html>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<p id="start">We followed the river.</p><p id="detail">We crossed a bridge.</p><p id="end">We reached the sea.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["The River", "The Sea"]
    );
    assert_eq!(
        book.chapters[0].text,
        "We followed the river.\n\nWe crossed a bridge."
    );
    assert_eq!(book.chapters[1].text, "We reached the sea.");
}

#[test]
fn keeps_nested_ncx_subsections_inside_their_parent_chapter() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>Chapter One</text></navLabel><content src="chapter.xhtml#one"/><navPoint><navLabel><text>Details</text></navLabel><content src="chapter.xhtml#details"/></navPoint></navPoint><navPoint><navLabel><text>Chapter Two</text></navLabel><content src="chapter.xhtml#two"/></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<h1 id="one">Chapter One</h1><p>The start.</p><h2 id="details">Details</h2><p>Still chapter one.</p><h1 id="two">Chapter Two</h1><p>The end.</p>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 2);
    assert_eq!(
        book.chapters[0].text,
        "Chapter One\n\nThe start.\n\nDetails\n\nStill chapter one."
    );
}

#[test]
fn joins_continuation_documents_until_the_next_chapter_boundary() {
    let bytes = support::epub(
        r#"<item id="one" href="one.xhtml" media-type="application/xhtml+xml"/><item id="more" href="more.xhtml" media-type="application/xhtml+xml"/><item id="two" href="two.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="one"/><itemref idref="more"/><itemref idref="two"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r#"<ncx><navMap><navPoint><navLabel><text>Departure</text></navLabel><content src="one.xhtml"/></navPoint><navPoint><navLabel><text>Return</text></navLabel><content src="two.xhtml"/></navPoint></navMap></ncx>"#,
            ),
            ("OPS/one.xhtml", "<h1>Departure</h1><p>The boat left.</p>"),
            ("OPS/more.xhtml", "<p>It crossed the lake.</p>"),
            ("OPS/two.xhtml", "<h1>Return</h1><p>It came home.</p>"),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 2);
    assert_eq!(
        book.chapters[0].text,
        "Departure\n\nThe boat left.\n\nIt crossed the lake."
    );
    assert_eq!(
        book.chapters[0].sources,
        [
            SourceLocation {
                path: "OPS/one.xhtml".into(),
                fragment: None
            },
            SourceLocation {
                path: "OPS/more.xhtml".into(),
                fragment: None
            },
        ]
    );
    assert_eq!(book.chapters[1].text, "Return\n\nIt came home.");
}

#[test]
fn splits_chapters_at_ncx_anchors_inside_one_document() {
    let bytes = support::epub(
        r#"<item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="toc" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#,
        r#"<itemref idref="chapter"/>"#,
        &[
            (
                "OPS/toc.ncx",
                r##"<ncx><navMap><navPoint><navLabel><text>The Shore</text></navLabel><content src="chapter.xhtml#one"/></navPoint><navPoint><navLabel><text>The Hill</text></navLabel><content src="chapter.xhtml#two"/></navPoint></navMap></ncx>"##,
            ),
            (
                "OPS/chapter.xhtml",
                r#"<html><body><p id="one">A boat arrived.</p><p id="two">We climbed uphill.</p></body></html>"#,
            ),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 2);
    assert_eq!(
        (
            book.chapters[0].title.as_str(),
            book.chapters[0].text.as_str()
        ),
        ("The Shore", "A boat arrived.")
    );
    assert_eq!(
        (
            book.chapters[1].title.as_str(),
            book.chapters[1].text.as_str()
        ),
        ("The Hill", "We climbed uphill.")
    );
}

#[test]
fn follows_spine_order_instead_of_filenames_or_zip_order() {
    let bytes = support::epub(
        r#"<item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="z" href="z.xhtml" media-type="application/xhtml+xml"/>"#,
        r#"<itemref idref="z"/><itemref idref="a"/>"#,
        &[
            ("OPS/a.xhtml", "<h1>Second</h1><p>The journey ends.</p>"),
            ("OPS/z.xhtml", "<h1>First</h1><p>The journey begins.</p>"),
        ],
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(
        book.chapters
            .iter()
            .map(|c| c.title.as_str())
            .collect::<Vec<_>>(),
        ["First", "Second"]
    );
}

#[test]
fn reads_xhtml_with_self_closing_title_and_anchors_as_xml_intends() {
    // An HTML parser would read everything after `<title/>` as the title, leaving no body.
    let bytes = support::single(
        r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title/><link href="s.css" rel="stylesheet"/></head><body><h1><a id="p1"/> The Harbour</h1><p>Boats rocked at <span/>anchor.</p><p>Night fell.</p></body></html>"#,
    );
    let book = parse_epub(&bytes, &ParseOptions::default()).unwrap();
    assert_eq!(book.chapters.len(), 1, "{:?}", book.warnings);
    assert_eq!(book.chapters[0].title, "The Harbour");
    assert_eq!(
        book.chapters[0].text,
        "The Harbour\n\nBoats rocked at anchor.\n\nNight fell."
    );
}
