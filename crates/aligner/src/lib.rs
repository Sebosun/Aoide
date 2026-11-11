//! Client for Aoide's local forced-alignment service (`tools/aligner/aoide_aligner.py`).
//!
//! Sends a segment's words and audio; receives each word's start and end in milliseconds, or
//! `None` for words the aligner could not place. Loopback-only by default; no proxies,
//! redirects or retries.

use std::time::Duration;

pub use aoide_tts::NetworkScope;

#[derive(Debug, Clone)]
pub struct Options {
    pub base_url: String,
    pub network: NetworkScope,
    pub connect_timeout: Duration,
    pub timeout: Duration,
    pub max_response_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8890".into(),
            network: NetworkScope::Loopback,
            connect_timeout: Duration::from_secs(2),
            timeout: Duration::from_secs(60),
            max_response_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid aligner configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid alignment request: {0}")]
    InvalidInput(String),
    #[error("aligner is unavailable: {0}")]
    Unavailable(String),
    #[error("aligner request timed out")]
    Timeout,
    #[error("aligner returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("aligner returned an invalid response: {0}")]
    InvalidResponse(String),
}

pub struct Aligner {
    http: reqwest::Client,
    base: reqwest::Url,
    options: Options,
}

const MAX_ERROR_BYTES: usize = 8 * 1024;

fn transport(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        Error::Timeout
    } else {
        Error::Unavailable(error.to_string())
    }
}

impl Aligner {
    /// Validates the address for its network scope. Construction makes no requests.
    pub fn new(options: Options) -> Result<Self, Error> {
        let base = aoide_tts::validate_endpoint(&options.base_url, options.network)
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        let base =
            reqwest::Url::parse(base.as_str()).map_err(|e| Error::InvalidConfig(e.to_string()))?;
        if options.connect_timeout.is_zero()
            || options.timeout.is_zero()
            || options.max_response_bytes == 0
        {
            return Err(Error::InvalidConfig(
                "timeouts and limits must be positive".into(),
            ));
        }
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(options.connect_timeout)
            .timeout(options.timeout)
            .build()
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        Ok(Self {
            http,
            base,
            options,
        })
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut url = self.base.clone();
        url.set_path(path);
        url
    }

    /// `GET /health`: whether the model is loaded.
    pub async fn ready(&self) -> Result<bool, Error> {
        let response = self
            .http
            .get(self.endpoint("/health"))
            .send()
            .await
            .map_err(transport)?;
        let value = self.json(response).await?;
        Ok(value
            .get("ready")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// `POST /align` with the words and mono audio (sent as 16-bit PCM). Returns one entry per
    /// word. Spans must have `start <= end`; a reply of the wrong length is `InvalidResponse`.
    pub async fn align(
        &self,
        words: &[String],
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<Vec<Option<(u32, u32)>>, Error> {
        use base64::Engine;
        if words.is_empty() {
            return Ok(Vec::new());
        }
        if samples.is_empty() || sample_rate == 0 {
            return Err(Error::InvalidInput("no audio".into()));
        }
        let pcm: Vec<u8> = samples
            .iter()
            .flat_map(|s| ((s * 32768.0).round().clamp(-32768.0, 32767.0) as i16).to_le_bytes())
            .collect();
        let body = serde_json::json!({
            "words": words,
            "sampleRate": sample_rate,
            "audio": base64::engine::general_purpose::STANDARD.encode(pcm),
        });
        let response = self
            .http
            .post(self.endpoint("/align"))
            .json(&body)
            .send()
            .await
            .map_err(transport)?;
        let value = self.json(response).await?;
        let invalid = |m: &str| Error::InvalidResponse(m.into());
        let list = value
            .get("words")
            .and_then(|v| v.as_array())
            .ok_or_else(|| invalid("missing words"))?;
        if list.len() != words.len() {
            return Err(invalid("word count does not match"));
        }
        list.iter()
            .map(|item| {
                if item.is_null() {
                    return Ok(None);
                }
                let pair = item
                    .as_array()
                    .filter(|a| a.len() == 2)
                    .ok_or_else(|| invalid("span is not a pair"))?;
                let ms = |v: &serde_json::Value| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| invalid("span is not milliseconds"))
                };
                let (start, end) = (ms(&pair[0])?, ms(&pair[1])?);
                if start > end {
                    return Err(invalid("span ends before it starts"));
                }
                Ok(Some((start, end)))
            })
            .collect()
    }

    async fn json(&self, mut response: reqwest::Response) -> Result<serde_json::Value, Error> {
        let status = response.status();
        let limit = if status == reqwest::StatusCode::OK {
            self.options.max_response_bytes
        } else {
            MAX_ERROR_BYTES
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if chunk.len() > limit - bytes.len() {
                if status == reqwest::StatusCode::OK {
                    return Err(Error::InvalidResponse(format!(
                        "response exceeds {limit} bytes"
                    )));
                }
                bytes.extend_from_slice(&chunk[..limit - bytes.len()]);
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        if status != reqwest::StatusCode::OK {
            let message = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_owned))
                .filter(|m| !m.contains('<'))
                .unwrap_or_else(|| status.canonical_reason().unwrap_or("error").to_owned());
            return Err(Error::Http {
                status: status.as_u16(),
                message,
            });
        }
        serde_json::from_slice(&bytes).map_err(|e| Error::InvalidResponse(e.to_string()))
    }
}
