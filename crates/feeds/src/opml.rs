//! OPML subscription lists: read from any reader's export (folders flattened), written as
//! OPML 2.0.

use quick_xml::escape::escape;
use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

/// One subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub title: Option<String>,
    /// The feed's address.
    pub xml_url: String,
    /// The site's address.
    pub html_url: Option<String>,
}

/// Every outline with a feed address, in document order, however deeply nested. Input that is
/// not OPML, or breaks off, gives what was read before it.
pub fn read(xml: &str) -> Vec<Outline> {
    let mut reader = Reader::from_str(xml);
    let mut feeds = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) if tag.name().as_ref() == b"outline" => {
                feeds.extend(outline(&tag));
            }
            Ok(Event::Eof) | Err(_) => return feeds,
            Ok(_) => {}
        }
    }
}

fn outline(tag: &BytesStart) -> Option<Outline> {
    let attribute = |name: &[u8]| {
        tag.attributes()
            .flatten()
            .find(|a| a.key.as_ref() == name)
            .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok())
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    Some(Outline {
        xml_url: attribute(b"xmlUrl")?,
        title: attribute(b"title").or_else(|| attribute(b"text")),
        html_url: attribute(b"htmlUrl"),
    })
}

/// `feeds` as an OPML 2.0 document.
pub fn write(feeds: &[Outline]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n  \
         <head><title>Aoide feeds</title></head>\n  <body>\n",
    );
    for feed in feeds {
        out.push_str("    <outline type=\"rss\"");
        if let Some(title) = &feed.title {
            let title = escape(title.as_str());
            out.push_str(&format!(" text=\"{title}\" title=\"{title}\""));
        }
        out.push_str(&format!(" xmlUrl=\"{}\"", escape(feed.xml_url.as_str())));
        if let Some(site) = &feed.html_url {
            out.push_str(&format!(" htmlUrl=\"{}\"", escape(site.as_str())));
        }
        out.push_str("/>\n");
    }
    out.push_str("  </body>\n</opml>\n");
    out
}
