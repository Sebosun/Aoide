//! Fetches a web page once and keeps it as a [`Snapshot`]: what was asked for, where it ended
//! up, and its HTML as text. It never interprets the page. Requests ignore the system proxy,
//! keep no cookies and are never retried; redirects are followed here, one hop at a time, and
//! each hop is checked again. Under [`Reach::Public`] no request may reach a loopback, private,
//! link-local or unspecified address, so a link from a feed or a paste cannot reach the local
//! network.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header::{
    HeaderMap, HeaderValue, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH,
    LAST_MODIFIED, LOCATION,
};
use reqwest::StatusCode;
use url::{Host, Url};

#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// For the whole fetch of one hop, body included.
    pub timeout: Duration,
    pub max_bytes: usize,
    pub max_redirects: usize,
    pub user_agent: String,
    pub reach: Reach,
    /// The Internet Archive's availability endpoint.
    pub archive: String,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            max_bytes: 8 * 1024 * 1024,
            max_redirects: 5,
            user_agent: "Aoide/0.1 (article reader)".into(),
            reach: Reach::Public,
            archive: "https://archive.org/wayback/available".into(),
        }
    }
}

/// Which addresses a fetch may reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Only addresses on the public internet.
    Public,
    /// Any address; for tests against a local server.
    AnyAddress,
}

/// How a snapshot was got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Http,
    /// Through an archived copy of the page.
    Archive,
    /// Rendered in a browser.
    Browser,
}

/// An image as it was fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

/// A page as it was fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub requested: Url,
    pub final_url: Url,
    pub status: u16,
    pub html: String,
    pub fetched_at_ms: i64,
    pub via: Via,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    Refused(String),
    #[error("The page took too long to answer.")]
    Timeout,
    #[error("The page is too large.")]
    TooLarge,
    #[error("Not a web page ({0}).")]
    NotHtml(String),
    #[error("Not a feed ({0}).")]
    NotAFeed(String),
    #[error("The site answered with status {0}.")]
    Status(u16),
    #[error("Could not reach the site: {0}")]
    Transport(String),
}

/// Whether an answer is a feed, from its media type (when it named one) and first bytes.
pub type Sniff = fn(Option<&str>, &[u8]) -> bool;

/// How much of an answer [`Sniff`] is shown.
pub const SNIFF_BYTES: usize = 2048;

/// What a feed was last served with, to ask whether it changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedResponse {
    /// Unchanged since the validators were given.
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
        last_modified: Option<String>,
        final_url: Url,
        /// It was redirected, permanently at every hop: `final_url` is its address now.
        moved: bool,
    },
}

pub struct HttpFetcher {
    client: reqwest::Client,
    options: FetchOptions,
}

impl HttpFetcher {
    pub fn new(options: FetchOptions) -> Result<Self, FetchError> {
        let mut builder = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(options.timeout)
            .user_agent(options.user_agent.clone());
        if options.reach == Reach::Public {
            builder = builder.dns_resolver(Arc::new(PublicOnly));
        }
        let client = builder
            .build()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        Ok(Self { client, options })
    }

    /// Fetches `url`, following redirects (and a page that only refreshes to another, as a
    /// moved static page does), and returns the page as text.
    pub async fn fetch(&self, url: &Url) -> Result<Snapshot, FetchError> {
        let mut current = url.clone();
        let mut refreshes = 0;
        loop {
            let (final_url, response) = self.send(&current).await?;
            let status = response.status().as_u16();
            let content_type = content_type(&response);
            let essence = essence(&content_type);
            if essence != "text/html" && essence != "application/xhtml+xml" {
                return Err(not_html(essence));
            }
            let body = self.body(response, self.options.max_bytes).await?;
            let html = decode(&body, &content_type);
            if let Some(next) = refresh_target(&html, &final_url) {
                if next != final_url && refreshes < self.options.max_redirects {
                    refreshes += 1;
                    current = next;
                    continue;
                }
            }
            return Ok(Snapshot {
                requested: url.clone(),
                final_url,
                status,
                html,
                fetched_at_ms: now_ms(),
                via: Via::Http,
            });
        }
    }

    /// Fetches the feed at `url`, asking only for a newer copy than `validators` describe.
    /// Redirects, size, time and reach are as for pages. XML and JSON types are accepted, and
    /// so are text and untyped answers, since servers often send feeds as `text/html` or
    /// `text/plain`: `sniff` tells a feed from a page by their type and first bytes (at most
    /// [`SNIFF_BYTES`]), and the rest of a page is not downloaded.
    pub async fn fetch_feed(
        &self,
        url: &Url,
        validators: &Validators,
        sniff: Sniff,
    ) -> Result<FeedResponse, FetchError> {
        let mut headers = HeaderMap::new();
        let mut add = |name, value: &Option<String>| {
            if let Some(value) = value.as_deref().and_then(|v| HeaderValue::from_str(v).ok()) {
                headers.insert(name, value);
            }
        };
        add(IF_NONE_MATCH, &validators.etag);
        add(IF_MODIFIED_SINCE, &validators.last_modified);
        let (final_url, response, moved) = self.send_with(url, headers).await?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(FeedResponse::NotModified);
        }
        let essence = essence(&content_type(&response));
        let feed_type = essence.ends_with("/xml")
            || essence.ends_with("+xml")
            || essence.ends_with("/json")
            || essence.ends_with("+json");
        let loose = essence.is_empty()
            || essence.starts_with("text/")
            || essence == "application/octet-stream";
        if !feed_type && !loose {
            return Err(FetchError::NotAFeed(essence));
        }
        let header = |name| {
            response
                .headers()
                .get(name)
                .and_then(|v: &HeaderValue| v.to_str().ok())
                .map(str::to_owned)
        };
        let (etag, last_modified) = (header(ETAG), header(LAST_MODIFIED));
        let typed = (!essence.is_empty()).then_some(essence.as_str());
        let looks = |head: &[u8]| feed_type || sniff(typed, head);
        let bytes = self
            .sniffed_body(response, self.options.max_bytes, looks)
            .await?
            .ok_or_else(|| FetchError::NotAFeed(essence.clone()))?;
        Ok(FeedResponse::Body {
            bytes,
            etag,
            last_modified,
            final_url,
            moved,
        })
    }

    /// Fetches the image at `url`, of at most `max_bytes`.
    pub async fn image(&self, url: &Url, max_bytes: usize) -> Result<Image, FetchError> {
        let (_, response) = self.send(url).await?;
        let media_type = essence(&content_type(&response));
        if !media_type.starts_with("image/") {
            return Err(not_html(media_type));
        }
        let bytes = self.body(response, max_bytes).await?;
        Ok(Image { media_type, bytes })
    }

    /// The Internet Archive's closest copy of `url`, as it was captured (without the archive's
    /// page around it); `None` when it has none.
    pub async fn archived(&self, url: &Url) -> Result<Option<Url>, FetchError> {
        let mut ask =
            Url::parse(&self.options.archive).map_err(|e| FetchError::Refused(e.to_string()))?;
        ask.query_pairs_mut().append_pair("url", url.as_str());
        let (_, response) = self.send(&ask).await?;
        let body = self.body(response, 256 * 1024).await?;
        let answer: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|e| FetchError::Transport(format!("unreadable archive answer: {e}")))?;
        let closest = &answer["archived_snapshots"]["closest"];
        if closest["available"].as_bool() != Some(true) {
            return Ok(None);
        }
        let Some(timestamp) = closest["timestamp"]
            .as_str()
            .filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
        else {
            return Ok(None);
        };
        let copy = format!("https://web.archive.org/web/{timestamp}id_/{url}");
        Ok(Url::parse(&copy).ok())
    }

    /// Asks for `url`, following redirects one hop at a time; a successful answer.
    async fn send(&self, url: &Url) -> Result<(Url, reqwest::Response), FetchError> {
        let (url, response, _) = self.send_with(url, HeaderMap::new()).await?;
        Ok((url, response))
    }

    /// Sends a GET with `headers`, following redirects one checked hop at a time. Returns the
    /// final address, its response (a success, or `304 Not Modified`) and whether every hop
    /// was a permanent redirect.
    async fn send_with(
        &self,
        url: &Url,
        headers: HeaderMap,
    ) -> Result<(Url, reqwest::Response, bool), FetchError> {
        let mut current = url.clone();
        let mut hops = 0;
        let mut permanent = true;
        loop {
            self.allow(&current).await?;
            let response = self
                .client
                .get(current.clone())
                .headers(headers.clone())
                .send()
                .await
                .map_err(failure)?;
            let status = response.status();
            if status == StatusCode::NOT_MODIFIED {
                return Ok((current, response, hops > 0 && permanent));
            }
            if status.is_redirection() {
                if let Some(location) = response.headers().get(LOCATION) {
                    permanent &= matches!(
                        status,
                        StatusCode::MOVED_PERMANENTLY | StatusCode::PERMANENT_REDIRECT
                    );
                    hops += 1;
                    if hops > self.options.max_redirects {
                        return Err(FetchError::Refused(format!(
                            "The page redirected more than {} times.",
                            self.options.max_redirects
                        )));
                    }
                    current = location
                        .to_str()
                        .ok()
                        .and_then(|l| current.join(l.trim()).ok())
                        .ok_or_else(|| {
                            FetchError::Refused(
                                "The page redirected to an unreadable address.".into(),
                            )
                        })?;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(FetchError::Status(status.as_u16()));
            }
            return Ok((current, response, hops > 0 && permanent));
        }
    }

    /// Refuses addresses this fetcher may not reach.
    async fn allow(&self, url: &Url) -> Result<(), FetchError> {
        check_reach(url, self.options.reach).await
    }

    async fn body(&self, response: reqwest::Response, max: usize) -> Result<Vec<u8>, FetchError> {
        let body = self.sniffed_body(response, max, |_| true).await?;
        Ok(body.expect("every body is wanted"))
    }

    /// The body, of at most `max` bytes; `None`, read no further, when `wanted` refuses its
    /// first [`SNIFF_BYTES`] (or all of it, when shorter).
    async fn sniffed_body(
        &self,
        mut response: reqwest::Response,
        max: usize,
        wanted: impl Fn(&[u8]) -> bool,
    ) -> Result<Option<Vec<u8>>, FetchError> {
        let declared = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok());
        if declared.is_some_and(|n| n > max) {
            return Err(FetchError::TooLarge);
        }
        let mut body = Vec::new();
        let mut sniffed = false;
        while let Some(chunk) = response.chunk().await.map_err(failure)? {
            if body.len() + chunk.len() > max {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
            if !sniffed && body.len() >= SNIFF_BYTES {
                sniffed = true;
                if !wanted(&body[..SNIFF_BYTES]) {
                    return Ok(None);
                }
            }
        }
        if !sniffed && !wanted(&body) {
            return Ok(None);
        }
        Ok(Some(body))
    }
}

/// Refuses `url` unless it is a web address that `reach` allows: under [`Reach::Public`], its
/// name must resolve to public addresses only.
pub async fn check_reach(url: &Url, reach: Reach) -> Result<(), FetchError> {
    if !matches!(url.scheme(), "http" | "https") {
        return Err(FetchError::Refused(format!(
            "Only web addresses can be fetched, not {}:.",
            url.scheme()
        )));
    }
    if reach == Reach::AnyAddress {
        return Ok(());
    }
    let local =
        || FetchError::Refused("The address is on this computer or its local network.".into());
    let port = url.port_or_known_default().unwrap_or(80);
    let addresses: Vec<IpAddr> = match url.host() {
        Some(Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
        Some(Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
        Some(Host::Domain(name)) => {
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            if name == "localhost" || name.ends_with(".localhost") {
                return Err(local());
            }
            let found = tokio::net::lookup_host((name.as_str(), port))
                .await
                .map_err(|e| FetchError::Transport(e.to_string()))?;
            found.map(|a| a.ip()).collect()
        }
        None => return Err(FetchError::Refused("The address has no host.".into())),
    };
    if addresses.is_empty() || !addresses.iter().all(|ip| public(*ip)) {
        return Err(local());
    }
    Ok(())
}

fn content_type(response: &reqwest::Response) -> String {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

/// The media type without its parameters, lower case.
fn essence(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn not_html(essence: String) -> FetchError {
    FetchError::NotHtml(if essence.is_empty() {
        "no type".into()
    } else {
        essence
    })
}

fn failure(error: reqwest::Error) -> FetchError {
    if error.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::Transport(error.without_url().to_string())
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Whether `ip` is on the public internet: not loopback, private, link-local, unspecified,
/// shared (carrier-grade NAT), broadcast or multicast.
pub fn public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || a == 0
                || (a == 100 && (64..128).contains(&b)))
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return public(IpAddr::V4(v4));
            }
            let first = v6.segments()[0];
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (first & 0xfe00) == 0xfc00
                || (first & 0xffc0) == 0xfe80)
        }
    }
}

/// Resolves names to public addresses only, so the client cannot be sent elsewhere by a second
/// lookup that answers differently.
struct PublicOnly;

impl reqwest::dns::Resolve for PublicOnly {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|a| public(a.ip()))
                .collect();
            if addresses.is_empty() {
                return Err("the name resolves to no public address".into());
            }
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// The page's bytes as text: the charset from the `Content-Type` header, else from a `<meta>`
/// near the start of the page, else UTF-8, with unreadable bytes replaced.
fn decode(body: &[u8], content_type: &str) -> String {
    let from_header = content_type
        .split(';')
        .skip(1)
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case("charset"))
        .map(|(_, v)| v.trim().trim_matches(|c| c == '"' || c == '\'').to_owned());
    let label = from_header.or_else(|| meta_charset(body));
    let encoding = label
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    encoding.decode(body).0.into_owned()
}

/// The charset a `<meta>` tag in the first 1024 bytes names (`charset=` in either form).
fn meta_charset(body: &[u8]) -> Option<String> {
    let head = &body[..body.len().min(1024)];
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();
    let mut rest = head.as_str();
    while let Some(at) = rest.find("<meta") {
        rest = &rest[at + 5..];
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(at) = tag.find("charset=") {
            let value = tag[at + 8..].trim_start_matches(['"', '\'', ' ']);
            let end = value
                .find(|c: char| c == '"' || c == '\'' || c == ';' || c == '/' || c.is_whitespace())
                .unwrap_or(value.len());
            if end > 0 {
                return Some(value[..end].to_owned());
            }
        }
    }
    None
}

/// How soon a refresh must come to count as a redirect rather than a page that reloads itself.
const REFRESH_SECONDS: f64 = 5.0;

/// Where a `<meta http-equiv="refresh">` sends the page at once, if anywhere.
fn refresh_target(html: &str, base: &Url) -> Option<Url> {
    // ASCII lowercasing keeps every byte offset, so `lower`'s positions index `html`.
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<meta") {
        let start = from + at;
        let end = lower[start..].find('>').map_or(lower.len(), |e| start + e);
        let tag = &html[start..end];
        from = end;
        if attribute(tag, "http-equiv").is_some_and(|v| v.eq_ignore_ascii_case("refresh")) {
            return refresh_url(&attribute(tag, "content")?, base);
        }
    }
    None
}

/// The value of attribute `name` in the start tag `tag`.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name) {
        let start = from + at;
        from = start + name.len();
        if !lower[..start].ends_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let Some(rest) = tag[from..].trim_start().strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next()? {
            quote @ ('"' | '\'') => rest[1..].split(quote).next()?,
            _ => rest
                .split(|c: char| c.is_ascii_whitespace() || c == '>')
                .next()?,
        };
        return Some(value.replace("&amp;", "&"));
    }
    None
}

/// The address in a refresh's `content` ("0; url=/new/"), when it comes within
/// [`REFRESH_SECONDS`].
fn refresh_url(content: &str, base: &Url) -> Option<Url> {
    let (delay, rest) = content.split_once([';', ','])?;
    if delay.trim().parse::<f64>().ok()? > REFRESH_SECONDS {
        return None;
    }
    let rest = rest.trim();
    let rest = match rest.get(..3) {
        Some(key) if key.eq_ignore_ascii_case("url") => rest[3..].trim_start().strip_prefix('=')?,
        _ => rest,
    };
    let target = rest.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    if target.is_empty() {
        return None;
    }
    base.join(target).ok()
}
