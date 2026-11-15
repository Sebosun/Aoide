//! Fetches a web page once and keeps it as a [`Snapshot`]: what was asked for, where it ended
//! up, and its HTML as text. It never interprets the page. Requests ignore the system proxy,
//! keep no cookies and are never retried; redirects are followed here, one hop at a time, and
//! each hop is checked again. Under [`Reach::Public`] no request may reach a loopback, private,
//! link-local or unspecified address, so a link from a feed or a paste cannot reach the local
//! network.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
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
    #[error("The site answered with status {0}.")]
    Status(u16),
    #[error("Could not reach the site: {0}")]
    Transport(String),
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

    /// Fetches `url`, following redirects, and returns the page as text.
    pub async fn fetch(&self, url: &Url) -> Result<Snapshot, FetchError> {
        let (final_url, response) = self.send(url).await?;
        let status = response.status().as_u16();
        let content_type = content_type(&response);
        let essence = essence(&content_type);
        if essence != "text/html" && essence != "application/xhtml+xml" {
            return Err(not_html(essence));
        }
        let body = self.body(response, self.options.max_bytes).await?;
        Ok(Snapshot {
            requested: url.clone(),
            final_url,
            status,
            html: decode(&body, &content_type),
            fetched_at_ms: now_ms(),
            via: Via::Http,
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
        let mut current = url.clone();
        let mut hops = 0;
        loop {
            self.allow(&current).await?;
            let response = self
                .client
                .get(current.clone())
                .send()
                .await
                .map_err(failure)?;
            let status = response.status();
            if status.is_redirection() {
                if let Some(location) = response.headers().get(LOCATION) {
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
            return Ok((current, response));
        }
    }

    /// Refuses addresses this fetcher may not reach.
    async fn allow(&self, url: &Url) -> Result<(), FetchError> {
        if !matches!(url.scheme(), "http" | "https") {
            return Err(FetchError::Refused(format!(
                "Only web addresses can be fetched, not {}:.",
                url.scheme()
            )));
        }
        if self.options.reach == Reach::AnyAddress {
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

    async fn body(
        &self,
        mut response: reqwest::Response,
        max: usize,
    ) -> Result<Vec<u8>, FetchError> {
        let declared = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok());
        if declared.is_some_and(|n| n > max) {
            return Err(FetchError::TooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failure)? {
            if body.len() + chunk.len() > max {
                return Err(FetchError::TooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
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
