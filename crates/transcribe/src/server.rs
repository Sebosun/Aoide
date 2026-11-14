//! A speech-recognition server that speaks the OpenAI transcription API
//! (`POST /v1/audio/transcriptions` with word timestamps), such as faster-whisper-server.
//! Like the speech clients: this computer only unless the home network is allowed, and no
//! proxies, redirects or retries.

use std::io::Read;
use std::sync::atomic::Ordering;
use std::time::Duration;

use aoide_tts::{validate_endpoint, NetworkScope};
use serde::Deserialize;

use crate::{Error, Request, Transcriber, Word, SAMPLE_RATE};

#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// The server's origin, such as `http://127.0.0.1:8000`.
    pub base_url: String,
    pub network: NetworkScope,
    /// Sent as `model`; `None` sends `"whisper-1"`.
    pub model: Option<String>,
    pub connect_timeout: Duration,
    /// For one block of a few minutes.
    pub request_timeout: Duration,
    pub max_response_bytes: u64,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8000".into(),
            network: NetworkScope::Loopback,
            model: None,
            connect_timeout: Duration::from_secs(2),
            request_timeout: Duration::from_secs(15 * 60),
            max_response_bytes: 16 * 1024 * 1024,
        }
    }
}

/// Holds only its settings: the blocking HTTP client is made for each request, on the caller's
/// thread, so dropping a transcriber is safe anywhere (a blocking client may not be dropped
/// inside an async runtime).
pub struct ServerTranscriber {
    options: ServerOptions,
    endpoint: reqwest::Url,
    model: String,
    id: String,
    max_response_bytes: u64,
}

/// What is kept of an error body.
const MAX_ERROR_CHARS: usize = 300;

impl ServerTranscriber {
    /// Checks the address against its network scope. Makes no requests.
    pub fn new(options: ServerOptions) -> Result<Self, Error> {
        let origin = validate_endpoint(&options.base_url, options.network)
            .map_err(|e| Error::Model(e.to_string()))?;
        let mut endpoint = reqwest::Url::parse(origin.as_str())
            .map_err(|_| Error::Model("expected an HTTP(S) URL".into()))?;
        endpoint.set_path("/v1/audio/transcriptions");
        let model = options
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or("whisper-1")
            .to_owned();
        Ok(Self {
            endpoint,
            id: format!("openai:{model}"),
            model,
            max_response_bytes: options.max_response_bytes,
            options,
        })
    }

    fn client(&self) -> Result<reqwest::blocking::Client, Error> {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(self.options.connect_timeout)
            .timeout(self.options.request_timeout)
            .build()
            .map_err(|e| Error::Failed(e.to_string()))
    }
}

#[derive(Deserialize)]
struct Transcription {
    words: Option<Vec<ServerWord>>,
}

#[derive(Deserialize)]
struct ServerWord {
    word: String,
    start: f64,
    end: f64,
    probability: Option<f32>,
}

impl Transcriber for ServerTranscriber {
    fn id(&self) -> &str {
        &self.id
    }

    fn transcribe(&self, request: &Request<'_>) -> Result<Vec<Word>, Error> {
        let stopped = || {
            request
                .abort
                .as_ref()
                .is_some_and(|a| a.load(Ordering::Relaxed))
        };
        if stopped() {
            return Err(Error::Aborted);
        }
        let failed = |e: reqwest::Error| Error::Failed(e.without_url().to_string());
        let file = reqwest::blocking::multipart::Part::bytes(wav(request.samples))
            .file_name("block.wav")
            .mime_str("audio/wav")
            .map_err(failed)?;
        let mut form = reqwest::blocking::multipart::Form::new()
            .text("model", self.model.clone())
            .text("response_format", "verbose_json")
            .text("timestamp_granularities[]", "word")
            .part("file", file);
        if let Some(language) = request.language {
            form = form.text("language", language.to_owned());
        }
        let response = self
            .client()?
            .post(self.endpoint.clone())
            .multipart(form)
            .send()
            .map_err(failed)?;
        if stopped() {
            return Err(Error::Aborted);
        }
        let status = response.status();
        let mut body = Vec::new();
        response
            .take(self.max_response_bytes + 1)
            .read_to_end(&mut body)
            .map_err(|e| Error::Failed(e.to_string()))?;
        if !status.is_success() {
            let text: String = String::from_utf8_lossy(&body)
                .chars()
                .take(MAX_ERROR_CHARS)
                .collect();
            return Err(Error::Failed(format!(
                "the server answered {status}: {text}"
            )));
        }
        if body.len() as u64 > self.max_response_bytes {
            return Err(Error::Failed("the server's answer is too large".into()));
        }
        let transcription: Transcription = serde_json::from_slice(&body)
            .map_err(|e| Error::Failed(format!("unexpected answer from the server: {e}")))?;
        let words = transcription.words.ok_or_else(|| {
            Error::Failed(
                "the server did not return word times (it needs word timestamps support)".into(),
            )
        })?;
        let ms = |seconds: f64| (seconds.max(0.0) * 1000.0).round() as u64;
        Ok(words
            .into_iter()
            .map(|w| Word {
                text: w.word.trim().to_owned(),
                start_ms: ms(w.start),
                end_ms: ms(w.end).max(ms(w.start)),
                probability: w.probability.unwrap_or(1.0),
            })
            .filter(|w| !w.text.is_empty())
            .collect())
    }
}

/// 16-bit mono PCM WAV of 16 kHz samples.
fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}
