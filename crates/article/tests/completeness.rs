//! One page per `Completeness` outcome and per wall, each beside a near miss that looks
//! similar and must read as complete.

use aoide_article::{extract, Completeness, ExtractOptions, Extracted, WallKind};
use aoide_document::Warning;
use url::Url;

const SENTENCES: &[&str] = &[
    "The harbour wall was built from stones carried down from the quarry above the town.",
    "Each spring the fishermen paint their boats in the colours their grandparents chose.",
    "A narrow lane climbs from the quay to the chapel, past the bakery and the net loft.",
    "In the evenings the older men sit on the bench by the slipway and argue about weather.",
    "Children learn to row before they learn to ride a bicycle, because the lanes are steep.",
    "The ferry timetable is pinned inside the window of the post office and nowhere else.",
];

/// About `words` words of plain prose in paragraphs of three sentences.
fn prose(words: usize) -> String {
    let mut out = String::new();
    let (mut count, mut i) = (0, 0);
    while count < words {
        out.push_str("<p>");
        for _ in 0..3 {
            let sentence = SENTENCES[i % SENTENCES.len()];
            out.push_str(sentence);
            out.push(' ');
            count += sentence.split_whitespace().count();
            i += 1;
        }
        out.push_str("</p>\n");
    }
    out
}

fn page(head: &str, body: &str) -> String {
    format!("<!doctype html><html lang=\"en\"><head><title>Life on the Quay at Porthallow</title>{head}</head><body>{body}</body></html>")
}

fn json_ld(fields: &str) -> String {
    format!(
        r#"<script type="application/ld+json">{{"@context":"https://schema.org","@type":"NewsArticle","headline":"Life on the Quay",{fields}}}</script>"#
    )
}

fn read(html: &str) -> Extracted {
    let url = Url::parse("https://quay.example/life").unwrap();
    extract(html, &url, &ExtractOptions::default())
}

fn completeness(html: &str) -> Completeness {
    read(html).completeness
}

#[test]
fn a_whole_article_is_complete() {
    let extracted = read(&page("", &format!("<article>{}</article>", prose(400))));
    assert_eq!(extracted.completeness, Completeness::Complete);
    assert!(extracted.meta.word_count >= 400);
    assert_eq!(extracted.meta.declared_words, None);
}

#[test]
fn far_fewer_words_than_declared_is_cut_short() {
    let html = page(
        &json_ld(r#""wordCount": 1000"#),
        &format!("<article>{}</article>", prose(200)),
    );
    let extracted = read(&html);
    let captured = extracted.meta.word_count;
    assert_eq!(
        extracted.completeness,
        Completeness::CutShort {
            captured,
            declared: 1000
        }
    );
    assert_eq!(extracted.meta.declared_words, Some(1000));

    // Near misses: most of what was declared, or too little declared to judge.
    let most = page(
        &json_ld(r#""wordCount": 1000"#),
        &format!("<article>{}</article>", prose(650)),
    );
    assert_eq!(completeness(&most), Completeness::Complete);
    let small = page(
        &json_ld(r#""wordCount": "250""#),
        &format!("<article>{}</article>", prose(100)),
    );
    assert_eq!(completeness(&small), Completeness::Complete);
}

#[test]
fn a_longer_body_in_json_ld_is_used_and_said() {
    let body: Vec<String> = (0..60)
        .map(|i| {
            format!(
                "Paragraph {i} of the full story. {}",
                SENTENCES[i % SENTENCES.len()]
            )
        })
        .collect();
    let field = format!(
        r#""articleBody": {}"#,
        serde_json::to_string(&body.join("\n\n")).unwrap()
    );
    let extracted = read(&page(
        &json_ld(&field),
        &format!("<article>{}</article>", prose(150)),
    ));
    let chapter = &extracted.document.chapters[0];
    assert_eq!(chapter.blocks.len(), 60);
    assert!(chapter.blocks[0]
        .text
        .starts_with("Paragraph 0 of the full story."));
    assert!(extracted
        .document
        .warnings
        .contains(&Warning::BodyFromMetadata));
    assert_eq!(extracted.completeness, Completeness::Complete);

    // Near miss: a shorter articleBody (a summary) leaves the page's text alone, and still
    // declares how long the article is.
    let field = r#""articleBody": "A short summary of the story.""#;
    let extracted = read(&page(
        &json_ld(field),
        &format!("<article>{}</article>", prose(150)),
    ));
    assert!(!extracted
        .document
        .warnings
        .contains(&Warning::BodyFromMetadata));
    assert!(extracted.document.chapters[0].blocks.len() > 1);
    assert_eq!(extracted.meta.declared_words, Some(6));

    // Near miss: an articleBody a little longer than the page's text (the same story with its
    // captions) leaves the page's text, and its headings, alone.
    let story = prose(300);
    let plain = story.replace("<p>", "").replace("</p>", " ");
    let field = format!(
        r#""articleBody": {}"#,
        serde_json::to_string(&format!(
            "{plain} Photograph: the harbour at dawn, from the quay."
        ))
        .unwrap()
    );
    let extracted = read(&page(
        &json_ld(&field),
        &format!("<article><h2>The harbour</h2>{story}</article>"),
    ));
    assert!(!extracted
        .document
        .warnings
        .contains(&Warning::BodyFromMetadata));
    assert!(extracted.document.chapters[0]
        .blocks
        .iter()
        .any(|b| b.text == "The harbour"));
}

#[test]
fn a_page_not_free_with_a_short_body_is_a_paywall() {
    let html = page(
        &json_ld(r#""isAccessibleForFree": false"#),
        &format!("<article>{}</article>", prose(90)),
    );
    assert_eq!(completeness(&html), Completeness::Wall(WallKind::Paywall));

    // Near miss: marked not free, but the whole text is there (a subscriber's copy).
    let html = page(
        &json_ld(r#""isAccessibleForFree": "False""#),
        &format!("<article>{}</article>", prose(500)),
    );
    assert_eq!(completeness(&html), Completeness::Complete);
}

#[test]
fn a_known_paywall_container_is_a_paywall() {
    let html = page(
        "",
        &format!(
            "<article>{}</article><div class=\"article-paywall overlay\"><p>Subscribe to keep reading.</p></div>",
            prose(120)
        ),
    );
    assert_eq!(completeness(&html), Completeness::Wall(WallKind::Paywall));

    // Near miss: an article about paywalls says the word but has no such container.
    let html = page(
        "",
        &format!(
            "<article><p>Why every paywall annoys readers.</p>{}</article>",
            prose(300)
        ),
    );
    assert_eq!(completeness(&html), Completeness::Complete);
}

#[test]
fn a_login_form_with_almost_no_text_is_a_sign_in_wall() {
    let html = page(
        "",
        "<main><h1>Sign in to continue</h1><form action=\"/login\" method=\"post\">\
         <input name=\"email\" type=\"email\"><input name=\"password\" type=\"password\">\
         <button>Sign in</button></form><p>Members can read every story.</p></main>",
    );
    assert_eq!(completeness(&html), Completeness::Wall(WallKind::SignIn));

    // Near miss: a full article with a small login box in the sidebar.
    let html = page(
        "",
        &format!(
            "<aside><form><input type=\"password\" name=\"pw\"><button>Log in</button></form></aside>\
             <article>{}</article>",
            prose(400)
        ),
    );
    assert_eq!(completeness(&html), Completeness::Complete);
}

#[test]
fn a_challenge_page_is_a_check_wall() {
    let html = "<!doctype html><html><head><title>Just a moment...</title></head><body>\
        <h1>Checking your browser before accessing quay.example.</h1>\
        <p>This process is automatic. Your browser will redirect to your requested content shortly.</p>\
        <noscript>Please enable JavaScript and Cookies to continue.</noscript></body></html>";
    assert_eq!(completeness(html), Completeness::Wall(WallKind::Check));

    // Near miss: an article that quotes the phrase.
    let html = page(
        "",
        &format!(
            "<article><p>The message said it was checking your browser, and then nothing happened.</p>{}</article>",
            prose(300)
        ),
    );
    assert_eq!(completeness(&html), Completeness::Complete);
}

#[test]
fn too_few_words_is_empty() {
    let html = page(
        "",
        "<article><p>Closed for the season. Back in April.</p></article>",
    );
    assert_eq!(completeness(&html), Completeness::Empty);

    // Near miss: a short but real piece.
    let html = page("", &format!("<article>{}</article>", prose(45)));
    assert_eq!(completeness(&html), Completeness::Complete);
}

#[test]
fn a_whole_article_on_a_site_that_has_a_paywall_is_complete() {
    // The site marks every page (a class on `body`); this article came through whole.
    let html = format!(
        "<!doctype html><html lang=\"en\"><head><title>Life on the Quay at Porthallow</title></head>\
         <body class=\"story user-anonymous paywall\"><article>{}</article></body></html>",
        prose(800)
    );
    assert_eq!(completeness(&html), Completeness::Complete);
}
