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
fn nested_outlines_are_read_as_feeds_in_folders() {
    let feeds = read(EXPORTED);
    assert_eq!(
        feeds,
        [
            Outline {
                title: Some("Harbour Notes".into()),
                xml_url: "https://harbour.example/feed.xml".into(),
                html_url: Some("https://harbour.example/".into()),
                folder: None,
            },
            Outline {
                title: Some("Tide & Wind".into()),
                xml_url: "https://tides.example/feed.json".into(),
                html_url: None,
                folder: Some("Weather".into()),
            },
            Outline {
                title: None,
                xml_url: "https://quay.example/feed.atom".into(),
                html_url: None,
                folder: Some("Weather / Folder in a folder".into()),
            },
        ]
    );
}

#[test]
fn a_feed_after_a_closed_folder_is_not_in_it() {
    let xml = r#"<opml version="2.0"><body>
      <outline text="News"><outline xmlUrl="https://a.example/feed"/></outline>
      <outline xmlUrl="https://b.example/feed"/>
      <outline text="Empty"></outline>
      <outline text="Self-closed folder"/>
      <outline xmlUrl="https://c.example/feed"/>
    </body></opml>"#;
    let folders: Vec<_> = read(xml).into_iter().map(|f| f.folder).collect();
    assert_eq!(folders, [Some("News".into()), None, None]);
}

#[test]
fn a_feed_with_children_does_not_become_a_folder() {
    let xml = r#"<opml version="2.0"><body>
      <outline text="Odd" xmlUrl="https://a.example/feed"><outline xmlUrl="https://b.example/feed"/></outline>
    </body></opml>"#;
    let folders: Vec<_> = read(xml).into_iter().map(|f| f.folder).collect();
    assert_eq!(folders, [None, None]);
}

#[test]
fn a_written_list_reads_back_the_same() {
    let feeds = vec![
        Outline {
            title: Some("Fish & <Chips> \"Daily\"".into()),
            xml_url: "https://chips.example/rss?a=1&b=2".into(),
            html_url: Some("https://chips.example/".into()),
            folder: None,
        },
        Outline {
            title: None,
            xml_url: "https://quay.example/feed.atom".into(),
            html_url: None,
            folder: Some("Harbour & <Sea>".into()),
        },
        Outline {
            title: Some("Deep".into()),
            xml_url: "https://deep.example/feed".into(),
            html_url: None,
            folder: Some("Weather / Tides".into()),
        },
        Outline {
            title: Some("Second in the folder".into()),
            xml_url: "https://second.example/feed".into(),
            html_url: None,
            folder: Some("Harbour & <Sea>".into()),
        },
    ];
    let xml = write(&feeds);
    assert!(xml.starts_with("<?xml"));
    assert!(xml.contains("<opml version=\"2.0\">"));
    let mut by_folder = feeds.clone();
    by_folder.sort_by(|a, b| a.folder.cmp(&b.folder));
    let mut back = read(&xml);
    back.sort_by(|a, b| a.folder.cmp(&b.folder));
    assert_eq!(back, by_folder);
}

#[test]
fn something_that_is_not_opml_gives_no_feeds() {
    assert!(read("not xml at all <<<").is_empty());
    assert!(read("<html><body><a href='x'>x</a></body></html>").is_empty());
}
