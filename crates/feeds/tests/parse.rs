//! Feeds parsed from synthetic fixtures with original text.
use aoide_feeds::{looks_like_feed, parse, FeedError, MAX_ENTRIES};
use url::Url;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/feeds/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn url(text: &str) -> Url {
    Url::parse(text).unwrap()
}

#[test]
fn rss_entries_carry_their_ids_links_dates_and_text() {
    let feed = parse(
        &fixture("rss.xml"),
        &url("https://harbour.example/feed.xml"),
    )
    .unwrap();
    assert_eq!(feed.title.as_deref(), Some("Harbour Notes"));
    assert_eq!(feed.site_url, Some(url("https://harbour.example/")));
    assert_eq!(feed.entries.len(), 4);

    let ferry = &feed.entries[0];
    assert_eq!(ferry.guid, "harbour-ferry-2026");
    assert_eq!(ferry.url, Some(url("https://harbour.example/2026/ferry")));
    // Entity-encoded twice, as some feeds do: the title reads as plain text.
    assert_eq!(
        ferry.title.as_deref(),
        Some("Tom & Jerry\u{2019}s new ferry")
    );
    assert_eq!(ferry.author.as_deref(), Some("Ada Brightwater"));
    assert_eq!(ferry.published_ms, Some(1_772_524_800_000));
    assert!(ferry
        .summary_html
        .as_deref()
        .unwrap()
        .contains("<b>ferry</b>"));
    assert!(ferry
        .content_html
        .as_deref()
        .unwrap()
        .contains("practised the crossing"));

    // A relative link is resolved against the feed, and it stands in for a missing id.
    let lamps = &feed.entries[1];
    assert_eq!(lamps.url, Some(url("https://harbour.example/2026/lamps")));
    assert_eq!(lamps.guid, "https://harbour.example/2026/lamps");
    assert_eq!(lamps.content_html, None, "summary only");
}

#[test]
fn an_entry_with_no_id_or_link_gets_the_same_id_every_time() {
    let bytes = fixture("rss.xml");
    let base = url("https://harbour.example/feed.xml");
    let first = parse(&bytes, &base).unwrap();
    let again = parse(&bytes, &base).unwrap();
    let note = &first.entries[2];
    assert_eq!(note.title.as_deref(), Some("A note without a link"));
    assert_eq!(note.url, None);
    assert!(!note.guid.is_empty());
    assert_eq!(note.guid, again.entries[2].guid);
    assert_ne!(note.guid, first.entries[3].guid);
}

#[test]
fn entries_come_newest_first_and_undated_ones_last() {
    let feed = parse(
        &fixture("rss.xml"),
        &url("https://harbour.example/feed.xml"),
    )
    .unwrap();
    let dates: Vec<Option<i64>> = feed.entries.iter().map(|e| e.published_ms).collect();
    assert!(dates[..3].windows(2).all(|w| w[0] > w[1]), "{dates:?}");
    assert_eq!(feed.entries[3].title.as_deref(), Some("Undated notice"));
    assert_eq!(dates[3], None);
}

#[test]
fn atom_resolves_relative_links_and_reads_html_titles_as_text() {
    let feed = parse(&fixture("atom.xml"), &url("https://quay.example/feed.atom")).unwrap();
    assert_eq!(feed.title.as_deref(), Some("Quay Weekly"));
    assert_eq!(feed.site_url, Some(url("https://quay.example/")));
    let lights = &feed.entries[0];
    assert_eq!(lights.guid, "urn:quay-weekly:lights");
    assert_eq!(
        lights.url,
        Some(url("https://quay.example/articles/lights"))
    );
    assert_eq!(lights.author.as_deref(), Some("Coll Fenwick"));
    assert_eq!(lights.published_ms, Some(1_775_034_000_000));
    assert_eq!(
        lights.summary_html.as_deref(),
        Some("Every evening at dusk the lamps are lit by hand.")
    );
    // No published date: the updated one stands in.
    let updated = &feed.entries[1];
    assert_eq!(updated.published_ms, Some(1_774_861_200_000));
    assert!(updated
        .content_html
        .as_deref()
        .unwrap()
        .contains("full content"));
}

#[test]
fn json_feeds_are_read_too() {
    let feed = parse(
        &fixture("feed.json"),
        &url("https://tides.example/feed.json"),
    )
    .unwrap();
    assert_eq!(feed.title.as_deref(), Some("Tide Tables"));
    assert_eq!(feed.site_url, Some(url("https://tides.example/")));
    let may = &feed.entries[0];
    assert_eq!(may.guid, "tides-may");
    assert_eq!(may.url, Some(url("https://tides.example/may")));
    assert_eq!(may.author.as_deref(), Some("Mara Quill"));
    assert!(may
        .content_html
        .as_deref()
        .unwrap()
        .contains("High water at six"));
    assert_eq!(
        may.summary_html.as_deref(),
        Some("High and low water for May.")
    );
}

#[test]
fn malformed_or_foreign_input_is_an_error() {
    let base = url("https://harbour.example/feed.xml");
    // Cut off, or broken inside: an error either way, never a panic.
    assert!(parse(b"<rss><channel><item><title>Cut", &base).is_err());
    assert!(matches!(
        parse(
            br#"{"version": "https://jsonfeed.org/version/1.1", "items": [{"id": }]}"#,
            &base
        ),
        Err(FeedError::Malformed(_))
    ));
    assert!(parse(b"<html><body><p>A page.</p></body></html>", &base).is_err());
    assert!(parse(b"", &base).is_err());
}

#[test]
fn a_huge_feed_keeps_its_newest_entries() {
    let filler = "Tide and weather. ".repeat(1000);
    let mut xml = String::from("<rss version=\"2.0\"><channel><title>Big</title>");
    for day in 0..600 {
        let (d, h) = (day / 24, day % 24);
        xml.push_str(&format!(
            "<item><title>Report {day}</title><guid>r{day}</guid>\
             <pubDate>{:02} Jan 2026 {h:02}:00:00 GMT</pubDate><description>{filler}</description></item>",
            d + 1
        ));
    }
    xml.push_str("</channel></rss>");
    assert!(xml.len() > 10 * 1024 * 1024);
    let feed = parse(xml.as_bytes(), &url("https://big.example/feed")).unwrap();
    assert_eq!(feed.entries.len(), MAX_ENTRIES);
    assert_eq!(feed.entries[0].guid, "r599");
    assert_eq!(feed.entries[MAX_ENTRIES - 1].guid, "r100");
}

#[test]
fn feeds_are_told_from_pages_by_type_and_first_bytes() {
    assert!(looks_like_feed(
        Some("application/rss+xml"),
        b"<?xml version=\"1.0\"?><rss>"
    ));
    assert!(looks_like_feed(
        Some("text/xml; charset=utf-8"),
        b"\xef\xbb\xbf<?xml?>\n<feed xmlns=\"x\">"
    ));
    assert!(looks_like_feed(None, b"  <rdf:RDF xmlns:rdf=\"r\">"));
    assert!(looks_like_feed(
        Some("application/feed+json"),
        br#"{"version": "https://jsonfeed.org/version/1.1", "items": []}"#
    ));
    // A feed served with a page's type is still a feed; a page is not.
    assert!(looks_like_feed(
        Some("text/html"),
        b"<?xml version=\"1.0\"?><rss version=\"2.0\">"
    ));
    assert!(!looks_like_feed(
        Some("text/html"),
        b"<!doctype html><html><head>"
    ));
    assert!(!looks_like_feed(
        Some("application/json"),
        br#"{"name": "not a feed"}"#
    ));
}

#[test]
fn markup_becomes_plain_text_with_its_words_counted() {
    use aoide_feeds::{plain_text, word_count};
    let html = "<p>The new <b>ferry</b> arrives&nbsp;in spring.</p>\n<p>Tom &amp; Jerry</p><script>x()</script>";
    assert_eq!(
        plain_text(html),
        "The new ferry arrives in spring. Tom & Jerry"
    );
    assert_eq!(word_count(html), 9);
}

#[test]
fn an_rss_author_is_read_as_a_name() {
    let item = |author: &str| {
        format!(
            "<rss version=\"2.0\"><channel><title>T</title><item><title>A</title><guid>a</guid>\
             <author>{author}</author></item></channel></rss>"
        )
    };
    let author = |text: &str| {
        parse(item(text).as_bytes(), &url("https://h.example/feed"))
            .unwrap()
            .entries[0]
            .author
            .clone()
    };
    assert_eq!(author("Coll Fenwick").as_deref(), Some("Coll Fenwick"));
    assert_eq!(
        author("coll@harbour.example (Coll Fenwick)").as_deref(),
        Some("Coll Fenwick")
    );
    assert_eq!(
        author("coll@harbour.example"),
        None,
        "an address is not a byline"
    );
}
