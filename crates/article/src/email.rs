//! A saved email (`.eml`) as a page: its HTML part, or its plain text in paragraphs, with the
//! headers an article shows. Pure: nothing it links to is fetched.

use aoide_document::{Block, BlockKind, Chapter, ExcludedSection, ExclusionReason};
use ego_tree::NodeId;
use mail_parser::{MessageParser, PartType};
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::Published;

/// What an email gives an article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailPage {
    /// Its HTML body parts, or its plain ones written as paragraphs, in order.
    pub html: String,
    pub subject: Option<String>,
    /// The sender's name, else their address.
    pub from: Option<String>,
    pub date: Option<Published>,
    /// Without its angle brackets.
    pub message_id: Option<String>,
    /// The newsletter's own copy on the web ("View in your browser"), when it links one.
    pub view_online: Option<Url>,
    /// Attachments, which are left out.
    pub attachments: usize,
}

impl EmailPage {
    /// The page an article keeps: the email's HTML under a head naming its subject, sender and
    /// date, which extraction reads back each time the article is opened.
    pub fn page(&self) -> String {
        let attribute = |text: &str| html_escape::encode_double_quoted_attribute(text).into_owned();
        let mut head = String::from("<meta charset=\"utf-8\">");
        let mut json = serde_json::Map::new();
        json.insert("@context".into(), "https://schema.org".into());
        json.insert("@type".into(), "Article".into());
        if let Some(subject) = &self.subject {
            head.push_str(&format!(
                "<title>{}</title><meta property=\"og:title\" content=\"{}\">",
                html_escape::encode_text(subject),
                attribute(subject)
            ));
            json.insert("headline".into(), subject.as_str().into());
        }
        if let Some(from) = &self.from {
            head.push_str(&format!(
                "<meta name=\"author\" content=\"{0}\"><meta property=\"og:site_name\" content=\"{0}\">",
                attribute(from)
            ));
            json.insert(
                "author".into(),
                serde_json::json!({ "@type": "Person", "name": from }),
            );
            json.insert(
                "publisher".into(),
                serde_json::json!({ "@type": "Organization", "name": from }),
            );
        }
        if let Some(date) = &self.date {
            head.push_str(&format!(
                "<meta property=\"article:published_time\" content=\"{}\">",
                attribute(&date.text)
            ));
            json.insert("datePublished".into(), date.text.as_str().into());
        }
        // A `</script>` in a subject cannot end the block early.
        let json = serde_json::Value::Object(json)
            .to_string()
            .replace("</", "<\\/");
        head.push_str(&format!(
            "<script type=\"application/ld+json\">{json}</script>"
        ));
        format!(
            "<!doctype html><html><head>{head}</head><body>{}</body></html>",
            self.html
        )
    }
}

/// Reads `bytes` as an email; `None` when they are not one (no sender, subject or id).
pub fn from_email(bytes: &[u8]) -> Option<EmailPage> {
    let message = MessageParser::default().parse(bytes)?;
    let named = |text: Option<&str>| {
        text.map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
    };
    let from = message
        .from()
        .and_then(|a| a.first())
        .and_then(|addr| named(addr.name()).or_else(|| named(addr.address())));
    let subject = named(message.subject());
    let message_id = named(
        message
            .message_id()
            .map(|id| id.trim().trim_start_matches('<').trim_end_matches('>')),
    );
    if from.is_none() && subject.is_none() && message_id.is_none() {
        return None;
    }
    let html: Vec<String> = message
        .html_bodies()
        .filter_map(|part| match &part.body {
            PartType::Html(html) => Some(html.to_string()),
            PartType::Text(text) => Some(paragraphs(text)),
            _ => None,
        })
        .collect();
    let html = html.join("\n");
    Some(EmailPage {
        view_online: view_online(&html),
        date: message.date().map(|date| Published {
            text: date.to_rfc3339(),
            at_ms: Some(date.to_timestamp() * 1000),
        }),
        html,
        subject,
        from,
        message_id,
        attachments: message.attachment_count(),
    })
}

/// Plain text as HTML paragraphs, split at blank lines, its lines kept.
fn paragraphs(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    text.split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| format!("<p>{}</p>", html_escape::encode_text(p)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What a newsletter's link to its own web copy says.
const VIEW_ONLINE: &[&str] = &[
    "view online",
    "view it online",
    "view in browser",
    "view in your browser",
    "view in a browser",
    "view this email in your browser",
    "view this email online",
    "open in browser",
    "open in your browser",
    "read online",
    "read in your browser",
    "web version",
    "view on the web",
    "read on the web",
    "view as a web page",
    "view as a webpage",
];

/// The first link whose text names the email's web copy, when it is a web address.
fn view_online(html: &str) -> Option<Url> {
    let page = scraper::Html::parse_document(html);
    let links = scraper::Selector::parse("a[href]").ok()?;
    page.select(&links).find_map(|link| {
        let text = link
            .text()
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if !VIEW_ONLINE.iter().any(|v| text.contains(v)) {
            return None;
        }
        let url = Url::parse(link.value().attr("href")?.trim()).ok()?;
        matches!(url.scheme(), "http" | "https").then_some(url)
    })
}

/// What an unsubscribe footer's links say.
const FOOTER_LINKS: &[&str] = &[
    "unsubscribe",
    "manage your subscription",
    "manage subscription",
    "manage your preferences",
    "update your preferences",
    "email preferences",
    "opt out",
    "opt-out",
];

/// Words a "view online" line may have.
const HEADER_WORDS: usize = 30;
/// Words a mailing footer may have.
const FOOTER_WORDS: usize = 80;

/// `html` without a newsletter's mailing parts (its "view online" line, its unsubscribe
/// footer, its tracking images), and each part that was taken out.
pub(crate) fn clean(html: &str) -> (String, Vec<ExcludedSection>) {
    let mut page = Html::parse_document(html);
    let Some(body) = select(&page, "body").into_iter().next() else {
        return (html.to_owned(), Vec::new());
    };
    let total = words(page.tree.get(body).and_then(ElementRef::wrap));
    let mut excluded = Vec::new();
    let header = select(&page, "a[href]")
        .into_iter()
        .find(|&id| link_says(&page, id, VIEW_ONLINE))
        .and_then(|link| part(&page, link, HEADER_WORDS, total));
    if let Some(id) = header {
        excluded.push(section("View online link", text(&page, id)));
        detach(&mut page, id);
    }
    let footer = select(&page, "a")
        .into_iter()
        .rev()
        .find(|&id| link_says(&page, id, FOOTER_LINKS))
        .and_then(|link| part(&page, link, FOOTER_WORDS, total));
    if let Some(id) = footer {
        excluded.push(section("Mailing footer", text(&page, id)));
        detach(&mut page, id);
    }
    let pixels: Vec<NodeId> = select(&page, "img")
        .into_iter()
        .filter(|&id| tracking(&page, id))
        .collect();
    if !pixels.is_empty() {
        let count = pixels.len();
        excluded.push(section(
            "Tracking image",
            format!("{count} image{}", if count == 1 { "" } else { "s" }),
        ));
        for id in pixels {
            detach(&mut page, id);
        }
    }
    (page.html(), excluded)
}

fn select(page: &Html, selector: &str) -> Vec<NodeId> {
    let selector = Selector::parse(selector).expect("a valid selector");
    page.select(&selector).map(|e| e.id()).collect()
}

fn element(page: &Html, id: NodeId) -> Option<ElementRef<'_>> {
    page.tree.get(id).and_then(ElementRef::wrap)
}

fn text(page: &Html, id: NodeId) -> String {
    element(page, id)
        .map(|e| e.text().collect::<Vec<_>>().join(" "))
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn words(element: Option<ElementRef<'_>>) -> usize {
    element.map_or(0, |e| e.text().flat_map(str::split_whitespace).count())
}

/// Whether link `id`'s text, or its address, says one of `phrases`.
fn link_says(page: &Html, id: NodeId, phrases: &[&str]) -> bool {
    let said = text(page, id).to_lowercase();
    let href = element(page, id)
        .and_then(|e| e.value().attr("href"))
        .unwrap_or_default()
        .to_lowercase();
    phrases
        .iter()
        .any(|p| said.contains(p) || (p.len() > 8 && href.contains(&p.replace(' ', ""))))
}

/// The mailing part around `link`: the largest block holding it that has at most `limit`
/// words and less than half of the letter's `total`; `None` when even the closest is more.
fn part(page: &Html, link: NodeId, limit: usize, total: usize) -> Option<NodeId> {
    let fits = |id: NodeId| {
        let n = words(element(page, id));
        n <= limit && n * 2 < total
    };
    let mut best = None;
    let mut node = page.tree.get(link)?;
    loop {
        let Some(e) = ElementRef::wrap(node) else {
            break;
        };
        if matches!(e.value().name(), "body" | "html") || !fits(node.id()) {
            break;
        }
        best = Some(node.id());
        let Some(parent) = node.parent() else {
            break;
        };
        node = parent;
    }
    best
}

/// An image that only tells the sender the email was opened: one pixel, or hidden.
fn tracking(page: &Html, id: NodeId) -> bool {
    let Some(img) = element(page, id) else {
        return false;
    };
    let tiny = |name: &str| {
        img.value()
            .attr(name)
            .is_some_and(|v| matches!(v.trim().trim_end_matches("px"), "0" | "1"))
    };
    let style = img
        .value()
        .attr("style")
        .unwrap_or_default()
        .replace(' ', "")
        .to_lowercase();
    tiny("width")
        || tiny("height")
        || style.contains("display:none")
        || style.contains("width:1px")
        || style.contains("height:1px")
}

fn detach(page: &mut Html, id: NodeId) {
    if let Some(mut node) = page.tree.get_mut(id) {
        node.detach();
    }
}

fn section(title: &str, text: String) -> ExcludedSection {
    let blocks = if text.is_empty() {
        Vec::new()
    } else {
        vec![Block {
            kind: BlockKind::Paragraph,
            text: text.clone(),
            break_before: false,
        }]
    };
    ExcludedSection {
        chapter: Chapter {
            title: title.into(),
            text,
            blocks,
            note_refs: Vec::new(),
            inserts: Vec::new(),
            tables: Vec::new(),
            sources: Vec::new(),
        },
        reason: ExclusionReason::Mailing,
    }
}
