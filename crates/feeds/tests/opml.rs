//! Subscription lists in and out.
use aoide_feeds::opml::{read, write, Outline};

const EXPORTED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>My feeds</title></head>
  <body>
    <outline text="Harbour Notes" type="rss" xmlUrl="https://harbour.example/feed.xml" htmlUrl="https://harbour.example/"/>
    <outline text="Weather">
      <outline title="Tide &amp; Wind" xmlUrl="https://tides.example/feed.json"/>
      <outline text="Folder in a folder">
        <outline xmlUrl="https://quay.example/feed.atom"/>
      </outline>
    </outline>
    <outline text="A bookmark, not a feed" htmlUrl="https://example.org/"/>
  </body>
</opml>"#;

#[test]
fn nested_outlines_are_read_as_one_list_of_feeds() {
    let feeds = read(EXPORTED);
    assert_eq!(
        feeds,
        [
            Outline {
                title: Some("Harbour Notes".into()),
                xml_url: "https://harbour.example/feed.xml".into(),
                html_url: Some("https://harbour.example/".into()),
            },
            Outline {
                title: Some("Tide & Wind".into()),
                xml_url: "https://tides.example/feed.json".into(),
                html_url: None,
            },
            Outline {
                title: None,
                xml_url: "https://quay.example/feed.atom".into(),
                html_url: None,
            },
        ]
    );
}

#[test]
fn a_written_list_reads_back_the_same() {
    let feeds = vec![
        Outline {
            title: Some("Fish & <Chips> \"Daily\"".into()),
            xml_url: "https://chips.example/rss?a=1&b=2".into(),
            html_url: Some("https://chips.example/".into()),
        },
        Outline {
            title: None,
            xml_url: "https://quay.example/feed.atom".into(),
            html_url: None,
        },
    ];
    let xml = write(&feeds);
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<opml version=\"2.0\">"));
    assert_eq!(read(&xml), feeds);
}

#[test]
fn something_that_is_not_opml_gives_no_feeds() {
    assert!(read("not xml at all <<<").is_empty());
    assert!(read("<html><body><a href='x'>x</a></body></html>").is_empty());
}
