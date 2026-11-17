//! A saved email (`.eml`) as a page: its HTML part, or its plain text in paragraphs, with the
//! headers an article shows. Pure: nothing it links to is fetched.

use mail_parser::{MessageParser, PartType};
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
