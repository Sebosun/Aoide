//! OPML subscription lists: read from any reader's export, written as OPML 2.0. Folders are one
//! level: nested folders are read as one name joined with " / ".

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
    /// The folder it is in, nested folders joined with " / ".
    pub folder: Option<String>,
}

/// What joins the names of nested folders into one.
pub const FOLDER_SEPARATOR: &str = " / ";

/// Every outline with a feed address, in document order, however deeply nested, each in the
/// folder of the outlines without a feed address around it. Input that is not OPML, or breaks
/// off, gives what was read before it.
pub fn read(xml: &str) -> Vec<Outline> {
    let mut reader = Reader::from_str(xml);
    let mut feeds = Vec::new();
    // One entry per open outline: its name when it is a folder.
    let mut open: Vec<Option<String>> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag)) if tag.name().as_ref() == b"outline" => {
                let feed = outline(&tag, &open);
                open.push(match feed {
                    Some(_) => None,
                    None => title(&tag),
                });
                feeds.extend(feed);
            }
            Ok(Event::Empty(tag)) if tag.name().as_ref() == b"outline" => {
                feeds.extend(outline(&tag, &open));
            }
            Ok(Event::End(tag)) if tag.name().as_ref() == b"outline" => {
                open.pop();
            }
            Ok(Event::Eof) | Err(_) => return feeds,
            Ok(_) => {}
        }
    }
}

fn attribute(tag: &BytesStart, name: &[u8]) -> Option<String> {
    tag.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == name)
        .and_then(|a| a.normalized_value(XmlVersion::Implicit1_0).ok())
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn title(tag: &BytesStart) -> Option<String> {
    attribute(tag, b"title").or_else(|| attribute(tag, b"text"))
}

fn outline(tag: &BytesStart, open: &[Option<String>]) -> Option<Outline> {
    let folders: Vec<&str> = open.iter().flatten().map(String::as_str).collect();
    Some(Outline {
        xml_url: attribute(tag, b"xmlUrl")?,
        title: title(tag),
        html_url: attribute(tag, b"htmlUrl"),
        folder: (!folders.is_empty()).then(|| folders.join(FOLDER_SEPARATOR)),
    })
}

/// `feeds` as an OPML 2.0 document: feeds without a folder first, then one outline per folder,
/// in the order each folder first appears.
pub fn write(feeds: &[Outline]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n  \
         <head><title>Aoide feeds</title></head>\n  <body>\n",
    );
    let mut folders: Vec<&str> = Vec::new();
    for feed in feeds {
        match &feed.folder {
            None => write_feed(&mut out, feed, "    "),
            Some(folder) if !folders.contains(&folder.as_str()) => folders.push(folder),
            Some(_) => {}
        }
    }
    for folder in folders {
        let name = escape(folder);
        out.push_str(&format!("    <outline text=\"{name}\" title=\"{name}\">\n"));
        for feed in feeds.iter().filter(|f| f.folder.as_deref() == Some(folder)) {
            write_feed(&mut out, feed, "      ");
        }
        out.push_str("    </outline>\n");
    }
    out.push_str("  </body>\n</opml>\n");
    out
}

fn write_feed(out: &mut String, feed: &Outline, indent: &str) {
    out.push_str(indent);
    out.push_str("<outline type=\"rss\"");
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
