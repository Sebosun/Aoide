use aoide_article::{canonical_id, extract, normalise, ExtractOptions};
use url::Url;

const BODY: &str = "<article><h1>The Night Ferry</h1>\
    <p>The night ferry crosses the strait twice before dawn, carrying mail, bread and whoever \
    missed the evening boat. Its crew of four knows every regular by name.</p>\
    <p>On calm nights the captain lets passengers stand at the bow and watch the lights of the \
    far harbour come up out of the dark, one by one.</p>\
    <p>In winter the crossing takes twice as long, and the cook keeps a pot of soup going from \
    the first bell until the last.</p></article>";

const JSON_LD: &str = r#"<script type="application/ld+json">
{"@context": "https://schema.org", "@graph": [
  {"@type": "WebSite", "name": "Not the article"},
  {"@type": ["NewsArticle"], "headline": "The Night Ferry (JSON-LD)",
   "author": [{"@type": "Person", "name": "Ada Brightwater"}, {"name": "Coll Fenwick"}],
   "publisher": {"@type": "Organization", "name": "Harbour Weekly"},
   "datePublished": "2026-03-04T05:06:07Z", "inLanguage": "en-GB",
   "url": "https://news.example/ferry", "image": {"url": "/img/ferry.jpg"}}
]}
</script>"#;

const OPEN_GRAPH: &str = r#"
<meta property="og:title" content="The Night Ferry (Open Graph)">
<meta property="og:site_name" content="Harbour Online">
<meta property="og:url" content="https://news.example/og-ferry">
<meta property="og:image" content="https://cdn.example/og.jpg">
<meta property="og:locale" content="fr_FR">
<meta property="article:published_time" content="2025-12-01">
<meta property="article:author" content="Mara Quill">
"#;

const PLAIN: &str = r#"
<title>The Night Ferry Crosses the Strait Before Dawn | Harbour</title>
<meta name="author" content="Tobin Reed">
<link rel="canonical" href="/plain-ferry">
"#;

fn page(head: &str) -> String {
    format!("<!doctype html><html lang=\"de\"><head>{head}</head><body>{BODY}</body></html>")
}

fn url() -> Url {
    Url::parse("https://news.example/2026/ferry?utm_source=feed").unwrap()
}

#[test]
fn json_ld_wins_over_open_graph() {
    let meta = extract(
        &page(&format!("{PLAIN}{OPEN_GRAPH}{JSON_LD}")),
        &url(),
        &ExtractOptions::default(),
    )
    .meta;
    assert_eq!(meta.title.as_deref(), Some("The Night Ferry (JSON-LD)"));
    assert_eq!(
        meta.byline.as_deref(),
        Some("Ada Brightwater, Coll Fenwick")
    );
    assert_eq!(meta.site_name.as_deref(), Some("Harbour Weekly"));
    let published = meta.published.unwrap();
    assert_eq!(published.text, "2026-03-04T05:06:07Z");
    assert_eq!(published.at_ms, Some(1_772_600_767_000));
    assert_eq!(meta.language.as_deref(), Some("en-GB"));
    assert_eq!(
        meta.canonical_url.unwrap().as_str(),
        "https://news.example/ferry"
    );
    assert_eq!(
        meta.lead_image.unwrap().as_str(),
        "https://news.example/img/ferry.jpg"
    );
}

#[test]
fn open_graph_wins_over_the_plain_page() {
    let meta = extract(
        &page(&format!("{PLAIN}{OPEN_GRAPH}")),
        &url(),
        &ExtractOptions::default(),
    )
    .meta;
    assert_eq!(meta.title.as_deref(), Some("The Night Ferry (Open Graph)"));
    assert_eq!(meta.byline.as_deref(), Some("Mara Quill"));
    assert_eq!(meta.site_name.as_deref(), Some("Harbour Online"));
    let published = meta.published.unwrap();
    assert_eq!(published.text, "2025-12-01");
    assert_eq!(published.at_ms, Some(1_764_547_200_000));
    assert_eq!(meta.language.as_deref(), Some("fr-FR"));
    assert_eq!(
        meta.canonical_url.unwrap().as_str(),
        "https://news.example/og-ferry"
    );
    assert_eq!(
        meta.lead_image.unwrap().as_str(),
        "https://cdn.example/og.jpg"
    );
}

#[test]
fn the_plain_page_gives_what_it_has() {
    let extracted = extract(&page(PLAIN), &url(), &ExtractOptions::default());
    let meta = extracted.meta;
    assert_eq!(
        meta.title.as_deref(),
        Some("The Night Ferry Crosses the Strait Before Dawn")
    );
    assert_eq!(meta.byline.as_deref(), Some("Tobin Reed"));
    assert_eq!(meta.site_name, None);
    assert_eq!(meta.published, None);
    assert_eq!(meta.language.as_deref(), Some("de"));
    assert_eq!(
        meta.canonical_url.unwrap().as_str(),
        "https://news.example/plain-ferry"
    );
    assert_eq!(meta.lead_image, None);
    let document = extracted.document;
    assert_eq!(
        document.title.as_deref(),
        Some("The Night Ferry Crosses the Strait Before Dawn")
    );
    assert_eq!(document.authors, ["Tobin Reed"]);
    assert_eq!(document.language.as_deref(), Some("de"));
}

#[test]
fn a_heading_that_repeats_the_title_is_not_kept_at_the_top() {
    let head = "<meta property=\"og:title\" content=\"The night  ferry\">";
    let document = extract(&page(head), &url(), &ExtractOptions::default()).document;
    let first = &document.chapters[0].blocks[0];
    assert!(
        first.text.starts_with("The night ferry crosses"),
        "{first:?}"
    );
    assert_eq!(document.chapters[0].title, "The night ferry");
}

#[test]
fn urls_are_normalised_before_they_name_an_article() {
    let n = |s: &str| normalise(&Url::parse(s).unwrap()).to_string();
    assert_eq!(
        n("HTTPS://News.Example/Story?utm_source=x&id=7&fbclid=y&page=2&gclid=z#comments"),
        "https://news.example/Story?id=7&page=2"
    );
    assert_eq!(
        n("https://news.example/story?utm_medium=mail"),
        "https://news.example/story"
    );
    assert_eq!(n("https://news.example"), n("https://news.example/"));
    assert_ne!(
        n("https://news.example/story/"),
        n("https://news.example/story")
    );
    assert_eq!(
        n("https://news.example/a?b=2&a=1"),
        "https://news.example/a?b=2&a=1"
    );

    let id = |s: &str| canonical_id(&Url::parse(s).unwrap());
    assert_eq!(
        id("https://news.example/story#top"),
        id("https://NEWS.example/story?utm_campaign=c")
    );
    assert_ne!(
        id("https://news.example/story"),
        id("https://news.example/other")
    );
    let hash = id("https://news.example/story");
    assert_eq!(hash.len(), 64);
    assert!(hash
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
}

#[test]
fn feeds_the_page_links_to_are_found() {
    let head = r#"
        <link rel="alternate" type="application/rss+xml" title="Harbour news" href="/feed.xml">
        <link rel="alternate" type="application/atom+xml" href="https://news.example/atom">
        <link rel="alternate" type="application/rss+xml" href="/feed.xml">
        <link rel="alternate" hreflang="fr" href="/fr/">
        <link rel="stylesheet" type="text/css" href="/site.css">"#;
    let feeds = extract(&page(head), &url(), &ExtractOptions::default()).feeds;
    let found: Vec<(&str, Option<&str>)> = feeds
        .iter()
        .map(|f| (f.url.as_str(), f.title.as_deref()))
        .collect();
    assert_eq!(
        found,
        [
            ("https://news.example/feed.xml", Some("Harbour news")),
            ("https://news.example/atom", None),
        ]
    );
}

#[test]
fn a_json_ld_headline_that_is_not_the_pages_title_gives_way_to_its_name() {
    // An encyclopedia's JSON-LD: `headline` holds a one-line description of the subject.
    let head = r#"<title>Lighthouse - Open Encyclopedia</title>
<script type="application/ld+json">
{"@context": "https://schema.org", "@type": "Article", "name": "Lighthouse",
 "headline": "structure designed to emit light to aid navigation"}
</script>"#;
    let meta = extract(&page(head), &url(), &ExtractOptions::default()).meta;
    assert_eq!(meta.title.as_deref(), Some("Lighthouse"));

    // Neither agrees with the page: its own title is used.
    let head = r#"<title>Harbour lights - Open Encyclopedia</title>
<script type="application/ld+json">
{"@context": "https://schema.org", "@type": "Article", "name": "Q1234",
 "headline": "structure designed to emit light to aid navigation"}
</script>"#;
    let meta = extract(&page(head), &url(), &ExtractOptions::default()).meta;
    assert_ne!(
        meta.title.as_deref(),
        Some("structure designed to emit light to aid navigation")
    );
    assert!(meta
        .title
        .as_deref()
        .is_some_and(|t| t.starts_with("Harbour lights")));
}

#[test]
fn a_sites_name_after_the_title_is_taken_off() {
    let head = r#"<title>Announcing the Night Ferry 2.0 | Harbour Blog</title>
<meta property="og:title" content="Announcing the Night Ferry 2.0 | Harbour Blog">"#;
    let body = BODY.replace(
        "<h1>The Night Ferry</h1>",
        "<h1>Announcing the Night Ferry 2.0</h1>",
    );
    let html = format!("<!doctype html><html><head>{head}</head><body>{body}</body></html>");
    let meta = extract(&html, &url(), &ExtractOptions::default()).meta;
    assert_eq!(
        meta.title.as_deref(),
        Some("Announcing the Night Ferry 2.0")
    );
}

#[test]
fn a_short_title_loses_its_sites_name_when_a_heading_says_it_alone() {
    // Three words before the site's name: content finders keep the whole title.
    let html = format!(
        "<!doctype html><html><head><title>Night Ferry 2.0 | Harbour Blog</title></head>\
         <body><header><h2>Night Ferry 2.0</h2></header>{BODY}</body></html>"
    );
    let meta = extract(&html, &url(), &ExtractOptions::default()).meta;
    assert_eq!(meta.title.as_deref(), Some("Night Ferry 2.0"));
}
