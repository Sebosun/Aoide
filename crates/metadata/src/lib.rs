//! Open Library lookup: finds candidate matches for a book and fetches descriptions and covers.
//!
//! Only the title, authors and ISBN of a book are sent. Nothing here writes files or decides
//! which match is right; the caller shows the matches and the user picks one.

use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Options {
    /// Open Library origin, e.g. `https://openlibrary.org` (tests use a loopback server).
    pub base_url: String,
    /// Cover image origin, e.g. `https://covers.openlibrary.org`.
    pub covers_url: String,
    /// Total deadline per request, including the body.
    pub timeout: Duration,
    /// Largest JSON response accepted.
    pub max_json_bytes: usize,
    /// Largest cover image accepted.
    pub max_cover_bytes: usize,
    /// Generic product token; never personal details.
    pub user_agent: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_url: "https://openlibrary.org".into(),
            covers_url: "https://covers.openlibrary.org".into(),
            timeout: Duration::from_secs(15),
            max_json_bytes: 4 * 1024 * 1024,
            max_cover_bytes: 8 * 1024 * 1024,
            user_agent: "Aoide/0.1 (local audiobook player)".into(),
        }
    }
}

/// What is known about the book locally.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    pub isbn: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
}

/// One candidate work from a search.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "MetadataMatch"))]
pub struct Match {
    /// Work key, e.g. `/works/OL45883W`.
    pub key: String,
    pub title: String,
    pub authors: Vec<String>,
    /// First publish year, as text.
    pub published: Option<String>,
    /// First listed publisher.
    pub publisher: Option<String>,
    /// The queried ISBN when the match lists it, else the first 13-digit ISBN, else the first ISBN.
    pub isbn: Option<String>,
    /// At most 10 subjects, in the order given.
    pub subjects: Vec<String>,
    /// `{covers_url}/b/id/{cover_i}-M.jpg` when the result has a `cover_i`.
    pub cover_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid metadata options: {0}")]
    InvalidOptions(String),
    #[error("invalid metadata request: {0}")]
    InvalidInput(String),
    #[error("Open Library returned HTTP {0}")]
    Http(u16),
    #[error("Open Library response exceeds {0} bytes")]
    TooLarge(usize),
    #[error("Open Library returned an unexpected response: {0}")]
    InvalidResponse(String),
    #[error("could not reach Open Library: {0}")]
    Transport(String),
    #[error("Open Library request timed out")]
    Timeout,
}

pub struct OpenLibrary {
    http: reqwest::Client,
    base: url::Url,
    covers: url::Url,
    options: Options,
}

const FIELDS: &str = "key,title,author_name,first_publish_year,publisher,isbn,subject,cover_i";
const LIMIT: &str = "8";

#[derive(serde::Deserialize)]
struct SearchResponse {
    #[serde(default)]
    docs: Vec<Doc>,
}

#[derive(serde::Deserialize, Default)]
#[serde(default)]
struct Doc {
    key: Option<String>,
    title: Option<String>,
    author_name: Vec<String>,
    first_publish_year: Option<i64>,
    publisher: Vec<String>,
    isbn: Vec<String>,
    subject: Vec<String>,
    cover_i: Option<i64>,
}

fn origin(value: &str, what: &str) -> Result<url::Url, Error> {
    let invalid = || Error::InvalidOptions(format!("{what} must be an http(s) origin"));
    let url = url::Url::parse(value).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
    {
        return Err(invalid());
    }
    Ok(url)
}

fn same_origin(a: &url::Url, b: &url::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn transport(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Transport(error.to_string())
    }
}

fn normalise_isbn(isbn: &str) -> String {
    isbn.chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn is_work_key(key: &str) -> bool {
    key.strip_prefix("/works/OL")
        .and_then(|rest| rest.strip_suffix('W'))
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

impl OpenLibrary {
    /// Validates options (http/https origins without path/query, positive limits). No requests.
    pub fn new(options: Options) -> Result<Self, Error> {
        let base = origin(&options.base_url, "base_url")?;
        let covers = origin(&options.covers_url, "covers_url")?;
        if options.timeout.is_zero() || options.max_json_bytes == 0 || options.max_cover_bytes == 0
        {
            return Err(Error::InvalidOptions("limits must be positive".into()));
        }
        if options.user_agent.trim().is_empty() {
            return Err(Error::InvalidOptions("user agent must not be empty".into()));
        }
        let http = reqwest::Client::builder()
            .no_proxy()
            .user_agent(options.user_agent.clone())
            .timeout(options.timeout)
            .redirect(reqwest::redirect::Policy::limited(3))
            .build()
            .map_err(|e| Error::InvalidOptions(e.to_string()))?;
        Ok(Self {
            http,
            base,
            covers,
            options,
        })
    }

    /// Up to 8 matches. With an ISBN, first `GET /search.json?isbn=<digits>&fields=..&limit=8`;
    /// when that finds nothing (or there is no ISBN), `GET /search.json?title=<t>&author=<first
    /// author>&fields=..&limit=8` (author omitted when unknown). Blank title and no ISBN is
    /// `InvalidInput` without a request.
    pub async fn search(&self, query: &Query) -> Result<Vec<Match>, Error> {
        let isbn = query
            .isbn
            .as_deref()
            .map(normalise_isbn)
            .filter(|i| i.len() >= 10);
        let title = query.title.trim();
        if isbn.is_none() && title.is_empty() {
            return Err(Error::InvalidInput("a title or ISBN is required".into()));
        }
        if let Some(isbn) = &isbn {
            let docs = self.search_docs(&[("isbn", isbn)]).await?;
            let matches = self.matches(docs, Some(isbn));
            if !matches.is_empty() || title.is_empty() {
                return Ok(matches);
            }
        }
        let author = query
            .authors
            .iter()
            .map(|a| a.trim())
            .find(|a| !a.is_empty());
        let mut params = vec![("title", title)];
        if let Some(author) = author {
            params.push(("author", author));
        }
        let docs = self.search_docs(&params).await?;
        Ok(self.matches(docs, isbn.as_deref()))
    }

    async fn search_docs(&self, params: &[(&str, &str)]) -> Result<Vec<Doc>, Error> {
        let mut url = self.base.clone();
        url.set_path("/search.json");
        {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in params {
                pairs.append_pair(key, value);
            }
            pairs.append_pair("fields", FIELDS);
            pairs.append_pair("limit", LIMIT);
        }
        let bytes = self.get(url, self.options.max_json_bytes, None).await?.1;
        let response: SearchResponse = serde_json::from_slice(&bytes)
            .map_err(|e| Error::InvalidResponse(format!("search results: {e}")))?;
        Ok(response.docs)
    }

    fn matches(&self, docs: Vec<Doc>, isbn: Option<&str>) -> Vec<Match> {
        docs.into_iter()
            .filter_map(|doc| {
                let key = doc.key.filter(|k| !k.trim().is_empty())?;
                let title = doc.title.filter(|t| !t.trim().is_empty())?;
                let listed = |wanted: &str| {
                    doc.isbn
                        .iter()
                        .find(|i| normalise_isbn(i) == wanted)
                        .cloned()
                };
                let isbn = isbn
                    .and_then(listed)
                    .or_else(|| {
                        doc.isbn
                            .iter()
                            .find(|i| normalise_isbn(i).len() == 13)
                            .cloned()
                    })
                    .or_else(|| doc.isbn.first().cloned());
                let cover_url = doc.cover_i.filter(|id| *id > 0).map(|id| {
                    let mut url = self.covers.clone();
                    url.set_path(&format!("/b/id/{id}-M.jpg"));
                    url.to_string()
                });
                Some(Match {
                    key,
                    title,
                    authors: doc.author_name,
                    published: doc.first_publish_year.map(|y| y.to_string()),
                    publisher: doc.publisher.into_iter().next(),
                    isbn,
                    subjects: doc.subject.into_iter().take(10).collect(),
                    cover_url,
                })
            })
            .collect()
    }

    /// The work's description from `GET {key}.json`; accepts a string or `{"value": ..}`.
    /// `key` must look like `/works/OL<digits>W`; anything else is `InvalidInput`.
    pub async fn description(&self, key: &str) -> Result<Option<String>, Error> {
        if !is_work_key(key) {
            return Err(Error::InvalidInput("not an Open Library work key".into()));
        }
        let mut url = self.base.clone();
        url.set_path(&format!("{key}.json"));
        let bytes = self.get(url, self.options.max_json_bytes, None).await?.1;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| Error::InvalidResponse(format!("work: {e}")))?;
        let text = match value.get("description") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(serde_json::Value::Object(o)) => {
                o.get("value").and_then(|v| v.as_str()).map(str::to_owned)
            }
            _ => None,
        };
        Ok(text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()))
    }

    /// Downloads a cover image (`image/jpeg` or `image/png`), returning (media type, bytes).
    /// Only URLs under `covers_url` are fetched; follows at most 3 redirects.
    pub async fn cover(&self, url: &str) -> Result<(String, Vec<u8>), Error> {
        let parsed =
            url::Url::parse(url).map_err(|_| Error::InvalidInput("invalid cover URL".into()))?;
        if !same_origin(&parsed, &self.covers) {
            return Err(Error::InvalidInput(
                "cover URL is not on the covers server".into(),
            ));
        }
        let (media_type, bytes) = self
            .get(
                parsed,
                self.options.max_cover_bytes,
                Some(&["image/jpeg", "image/png"]),
            )
            .await?;
        Ok((media_type, bytes))
    }

    async fn get(
        &self,
        url: url::Url,
        limit: usize,
        types: Option<&[&str]>,
    ) -> Result<(String, Vec<u8>), Error> {
        let url =
            reqwest::Url::parse(url.as_str()).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let mut response = self.http.get(url).send().await.map_err(transport)?;
        let status = response.status();
        if status != reqwest::StatusCode::OK {
            return Err(Error::Http(status.as_u16()));
        }
        let media_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(|v| v.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if let Some(types) = types {
            if !types.contains(&media_type.as_str()) {
                return Err(Error::InvalidResponse(format!(
                    "unexpected content type {media_type:?}"
                )));
            }
        }
        if response.content_length().is_some_and(|l| l > limit as u64) {
            return Err(Error::TooLarge(limit));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if chunk.len() > limit - bytes.len() {
                return Err(Error::TooLarge(limit));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((media_type, bytes))
    }
}
