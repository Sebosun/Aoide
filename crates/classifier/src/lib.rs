//! Tags for library items from a local language model, over the OpenAI-compatible chat API that
//! Ollama, llama.cpp's server and LM Studio offer (`POST /v1/chat/completions`).
//!
//! The prompt ([`prompt`]) and the reading of the answer ([`parse_tags`]) are pure. The client
//! talks only to loopback (or, when chosen, private-network) addresses, with proxies, redirects
//! and retries off, like the speech engines.

use std::time::Duration;

use aoide_tts::NetworkScope;
use serde::{Deserialize, Serialize};

/// Tags at most per item.
pub const MAX_TAGS: usize = 5;
/// A tag is at most this many characters.
const MAX_TAG_CHARS: usize = 40;
/// The item's own text sent at most, in characters.
const MAX_EXCERPT_CHARS: usize = 6_000;
/// Tags in use sent at most.
const MAX_KNOWN: usize = 200;

/// What the model is told of an item.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    pub title: String,
    pub authors: Vec<String>,
    /// Its blurb or synopsis.
    pub description: Option<String>,
    /// The start of its text, when there is no description.
    pub excerpt: Option<String>,
}

/// One chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// The messages that ask for an item's tags, naming the tags in use so they are preferred.
pub fn prompt(item: &Item, known: &[String]) -> Vec<Message> {
    let system = format!(
        "You tag items in a personal library of books, audiobooks and articles. Reply with only \
         a JSON array of 1 to {MAX_TAGS} short tags (genres, subjects, forms), the most fitting \
         first. Use a tag from the tags in use whenever one fits. No explanations."
    );
    let mut asked = String::new();
    if !known.is_empty() {
        let shown: Vec<&str> = known.iter().take(MAX_KNOWN).map(String::as_str).collect();
        asked.push_str(&format!("Tags in use: {}\n", shown.join(", ")));
    }
    asked.push_str(&format!("Title: {}\n", item.title));
    if !item.authors.is_empty() {
        asked.push_str(&format!("By: {}\n", item.authors.join(", ")));
    }
    if let Some(description) = item.description.as_deref().filter(|d| !d.trim().is_empty()) {
        asked.push_str(&format!("Description: {}\n", clip(description)));
    } else if let Some(excerpt) = item.excerpt.as_deref().filter(|e| !e.trim().is_empty()) {
        asked.push_str(&format!("Opening: {}\n", clip(excerpt)));
    }
    vec![
        Message {
            role: "system".into(),
            content: system,
        },
        Message {
            role: "user".into(),
            content: asked,
        },
    ]
}

fn clip(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.char_indices().nth(MAX_EXCERPT_CHARS) {
        Some((at, _)) => format!("{}…", &text[..at]),
        None => text,
    }
}

/// The tags in a model's answer: a JSON array when it gave one, else its lines or commas, without
/// numbering, bullets or quotes. A tag in use is given as it is written there; repeats (ignoring
/// case) and over-long ones are left out, and at most [`MAX_TAGS`] kept.
pub fn parse_tags(answer: &str, known: &[String]) -> Vec<String> {
    let array = answer
        .find('[')
        .zip(answer.rfind(']'))
        .filter(|(start, end)| start < end)
        .and_then(|(start, end)| serde_json::from_str::<Vec<String>>(&answer[start..=end]).ok());
    let raw: Vec<String> = match array {
        Some(list) => list,
        None => answer
            .split(['\n', ','])
            .map(|part| {
                part.trim()
                    .trim_start_matches(|c: char| {
                        c.is_ascii_digit() || matches!(c, '.' | ')' | '-' | '*' | '•')
                    })
                    .to_owned()
            })
            .collect(),
    };
    let mut tags: Vec<String> = Vec::new();
    for tag in raw {
        let tag = tag
            .trim()
            .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | '.' | '#'))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS {
            continue;
        }
        let tag = known
            .iter()
            .find(|k| k.eq_ignore_ascii_case(&tag))
            .cloned()
            .unwrap_or(tag);
        if !tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
            tags.push(tag);
        }
        if tags.len() == MAX_TAGS {
            break;
        }
    }
    tags
}

/// Where the model is and how long to wait for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The server's origin, such as `http://127.0.0.1:11434` for Ollama.
    pub base_url: String,
    /// The model to ask, as the server names it.
    pub model: String,
    pub network: NetworkScope,
    pub connect_timeout: Duration,
    /// A small model on a CPU may take a while.
    pub answer_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:11434".into(),
            model: String::new(),
            network: NetworkScope::Loopback,
            connect_timeout: Duration::from_secs(2),
            answer_timeout: Duration::from_secs(120),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    InvalidOptions(String),
    #[error("the tagging model is unavailable: {0}")]
    Unavailable(String),
    #[error("the tagging model took too long to answer")]
    Timeout,
    #[error("the tagging model answered with HTTP {status}: {body}")]
    Status { status: u16, body: String },
    #[error("the tagging model's answer could not be read: {0}")]
    Malformed(String),
}

impl Error {
    /// What went wrong, in a few words for people.
    pub fn summary(&self) -> &'static str {
        match self {
            Error::InvalidOptions(_) => "The tagging model's settings are not valid",
            Error::Unavailable(_) => "The tagging model is unavailable",
            Error::Timeout => "The tagging model took too long to answer",
            Error::Status { .. } | Error::Malformed(_) => "The tagging model could not answer",
        }
    }
}

/// A client for one model on one server.
pub struct Classifier {
    http: reqwest::Client,
    endpoint: url::Url,
    options: Options,
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    messages: Vec<Message>,
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

#[derive(Deserialize)]
struct Answer {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

impl Classifier {
    /// Checks the options; makes no request.
    pub fn new(options: Options) -> Result<Self, Error> {
        if options.model.trim().is_empty() {
            return Err(Error::InvalidOptions("choose a model to tag with".into()));
        }
        let origin = aoide_tts::validate_endpoint(&options.base_url, options.network)
            .map_err(|e| Error::InvalidOptions(e.to_string()))?;
        let endpoint = origin
            .join("v1/chat/completions")
            .map_err(|e| Error::InvalidOptions(e.to_string()))?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(options.connect_timeout)
            .build()
            .map_err(|e| Error::InvalidOptions(e.to_string()))?;
        Ok(Self {
            http,
            endpoint,
            options,
        })
    }

    /// Asks the model for an item's tags, preferring those in `known`.
    pub async fn suggest(&self, item: &Item, known: &[String]) -> Result<Vec<String>, Error> {
        let request = Request {
            model: &self.options.model,
            messages: prompt(item, known),
            temperature: 0.0,
            max_tokens: 120,
            stream: false,
        };
        let response = self
            .http
            .post(self.endpoint.clone())
            .timeout(self.options.answer_timeout)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    Error::Timeout
                } else {
                    Error::Unavailable(e.to_string())
                }
            })?;
        let status = response.status();
        let body = response.text().await.map_err(|e| {
            if e.is_timeout() {
                Error::Timeout
            } else {
                Error::Unavailable(e.to_string())
            }
        })?;
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                body: body.chars().take(300).collect(),
            });
        }
        let answer: Answer =
            serde_json::from_str(&body).map_err(|e| Error::Malformed(e.to_string()))?;
        let content = answer
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| Error::Malformed("no choices".into()))?;
        Ok(parse_tags(&content, known))
    }
}
