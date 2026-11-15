//! Feeds as Aoide reads them: RSS (0.9x, 1.0, 2.0), Atom and JSON Feed, parsed by `feed-rs`
//! behind these types. Pure: bytes in, entries out, no network and no clock.

use std::cmp::Ordering;
use std::fmt;

use feed_rs::model;
use sha2::{Digest, Sha256};
use url::Url;

pub mod opml;

/// The most entries kept from one feed: the newest.
pub const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFeed {
    pub title: Option<String>,
    /// The site the feed belongs to.
    pub site_url: Option<Url>,
    /// Newest first; undated entries last, in the feed's order.
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The feed's id for it; else its link; else a hash of its title and date.
    pub guid: String,
    /// Resolved against the feed's address.
    pub url: Option<Url>,
    pub title: Option<String>,
    pub author: Option<String>,
    /// Its published date, else its updated one.
    pub published_ms: Option<i64>,
    pub summary_html: Option<String>,
    pub content_html: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedError {
    /// It looked like a feed but could not be read.
    Malformed(String),
    /// It is not a feed at all.
    NotAFeed,
}

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(why) => write!(f, "The feed could not be read: {why}"),
            Self::NotAFeed => f.write_str("That is not a feed."),
        }
    }
}

impl std::error::Error for FeedError {}

/// Parses a feed fetched from `feed_url`.
pub fn parse(bytes: &[u8], feed_url: &Url) -> Result<ParsedFeed, FeedError> {
    // Ids are Aoide's to give (the library's would be random for an entry with no link).
    let parser = feed_rs::parser::Builder::new()
        .id_generator(|_, _, _| String::new())
        .build();
    let feed = parser.parse(bytes).map_err(error)?;
    let resolve = |href: &str| feed_url.join(href.trim()).ok();
    let site_url = feed
        .links
        .iter()
        .filter(|l| matches!(l.rel.as_deref(), None | Some("alternate")))
        .filter_map(|l| resolve(&l.href))
        .find(|u| u != feed_url);
    let mut entries: Vec<Entry> = feed
        .entries
        .into_iter()
        .map(|e| entry(e, &resolve))
        .collect();
    entries.sort_by(|a, b| match (a.published_ms, b.published_ms) {
        (Some(a), Some(b)) => b.cmp(&a),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    entries.truncate(MAX_ENTRIES);
    Ok(ParsedFeed {
        title: feed.title.as_ref().and_then(text),
        site_url,
        entries,
    })
}

fn error(error: feed_rs::parser::ParseFeedError) -> FeedError {
    use feed_rs::parser::{ParseErrorKind, ParseFeedError};
    match error {
        ParseFeedError::ParseError(ParseErrorKind::NoFeedRoot) => FeedError::NotAFeed,
        other => FeedError::Malformed(other.to_string()),
    }
}

fn entry(e: model::Entry, resolve: &impl Fn(&str) -> Option<Url>) -> Entry {
    let url = e
        .links
        .iter()
        .find(|l| matches!(l.rel.as_deref(), None | Some("alternate")))
        .or_else(|| e.links.first())
        .and_then(|l| resolve(&l.href));
    let title = e.title.as_ref().and_then(text);
    let published_ms = e.published.or(e.updated).map(|t| t.timestamp_millis());
    let guid = if !e.id.trim().is_empty() {
        e.id.trim().to_owned()
    } else if let Some(url) = &url {
        url.to_string()
    } else {
        let mut hash = Sha256::new();
        hash.update(title.as_deref().unwrap_or_default());
        hash.update(published_ms.map(|t| t.to_string()).unwrap_or_default());
        let digest = hash.finalize();
        let hex: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
        format!("sha256:{hex}")
    };
    let nonempty = |s: Option<String>| s.filter(|s| !s.trim().is_empty());
    Entry {
        guid,
        url,
        title,
        author: e
            .authors
            .first()
            .map(|p| p.name.trim().to_owned())
            .filter(|n| !n.is_empty()),
        published_ms,
        summary_html: nonempty(e.summary.map(|s| s.content)),
        content_html: nonempty(e.content.and_then(|c| c.body)),
    }
}

/// A title as plain text: markup removed when it is HTML, entities decoded (feeds often encode
/// them twice), spaces collapsed.
fn text(text: &model::Text) -> Option<String> {
    let html = text.content_type.to_string().starts_with("text/html");
    let content = if html {
        strip_tags(&text.content)
    } else {
        text.content.clone()
    };
    let decoded = html_escape::decode_html_entities(&content);
    let plain = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    (!plain.is_empty()).then_some(plain)
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Whether a response is a feed rather than a page, from its content type and first bytes.
/// The bytes decide: servers often send feeds as `text/html` or plain XML.
pub fn looks_like_feed(content_type: Option<&str>, head: &[u8]) -> bool {
    let head = head.strip_prefix(b"\xef\xbb\xbf").unwrap_or(head);
    let start = head
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(head.len());
    let head = String::from_utf8_lossy(&head[start..head.len().min(start + 2048)]).to_lowercase();
    if head.starts_with('{') {
        return head.contains("jsonfeed.org/version");
    }
    if !head.starts_with('<') {
        return false;
    }
    let roots = ["<rss", "<feed", "<rdf:rdf"];
    let found = roots.iter().any(|root| head.contains(root));
    let page = head.contains("<html") || head.starts_with("<!doctype html");
    let typed = content_type
        .map(|t| t.to_ascii_lowercase())
        .is_some_and(|t| t.contains("rss") || t.contains("atom") || t.contains("rdf"));
    found && (!page || typed)
}
