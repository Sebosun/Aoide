//! Loopback-only client for devnen/Chatterbox-TTS-Server, independent of EPUBs and UI.
//!
//! ```no_run
//! use aoide_chatterbox::{ChatterboxClient, ClientOptions, SpeechRequest};
//! # async fn example() -> Result<(), aoide_chatterbox::Error> {
//! let client = ChatterboxClient::new(ClientOptions::default())?;
//! let voices = client.voices().await?;
//! if let Some(voice) = voices.first() {
//!     let audio = client.generate(&SpeechRequest::new("An original sentence.", &voice.id)).await?;
//!     assert!(audio.metadata().frames > 0);
//! }
//! # Ok(())
//! # }
//! ```
use std::time::Duration;
mod audio;
mod endpoint;
pub mod engine;
mod request;
mod response;
mod voices;
pub use audio::{AudioMetadata, GeneratedAudio, SampleEncoding};

pub struct SpeechRequest {
    /// Sent verbatim; the client does not edit or normalize chapter text.
    pub text: String,
    /// A predefined WAV/MP3 filename obtained from `voices()`.
    pub voice_id: String,
    pub settings: GenerationSettings,
}

impl SpeechRequest {
    pub fn new(text: impl Into<String>, voice_id: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice_id: voice_id.into(),
            settings: GenerationSettings::default(),
        }
    }
}

pub struct GenerationSettings {
    /// Let the server split long text into chunks (and stitch the audio).
    pub split_text: bool,
    /// Approximate server-side target length, in characters; valid range 50..=500.
    pub chunk_size: u16,
    pub seed: Option<u32>,
    pub language: Option<String>,
    /// Engine sampling overrides; `None` uses the server's configured defaults.
    pub temperature: Option<f32>,
    pub exaggeration: Option<f32>,
    pub cfg_weight: Option<f32>,
    pub speed_factor: Option<f32>,
}

impl Default for GenerationSettings {
    fn default() -> Self {
        Self {
            split_text: true,
            chunk_size: 240,
            seed: None,
            language: None,
            temperature: None,
            exaggeration: None,
            cfg_weight: None,
            speed_factor: None,
        }
    }
}

/// What `/api/model-info` reports about the loaded model.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct ModelInfo {
    pub loaded: bool,
    /// `"original"`, `"turbo"` or `"multilingual"` when loaded.
    #[serde(rename = "type")]
    pub model_type: Option<String>,
}

#[derive(Debug)]
pub struct ClientOptions {
    pub base_url: String,
    /// Which hosts `base_url` may name. Loopback unless the user opted in to LAN access.
    pub network: aoide_tts::NetworkScope,
    pub connect_timeout: Duration,
    pub discovery_timeout: Duration,
    /// Total HTTP deadline, including the response body (not CPU-side validation).
    pub generation_timeout: Duration,
    pub max_audio_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_error_bytes: usize,
    pub max_text_bytes: usize,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8004".into(),
            network: aoide_tts::NetworkScope::Loopback,
            connect_timeout: Duration::from_secs(2),
            discovery_timeout: Duration::from_secs(5),
            generation_timeout: Duration::from_secs(30 * 60),
            max_audio_bytes: 256 * 1024 * 1024,
            max_metadata_bytes: 1024 * 1024,
            max_error_bytes: 8 * 1024,
            max_text_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Voice {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid local Chatterbox endpoint: {0}")]
    InvalidEndpoint(&'static str),
    #[error("invalid Chatterbox client options: {0}")]
    InvalidOptions(&'static str),
    #[error("invalid speech request: {0}")]
    InvalidInput(&'static str),
    #[error("Chatterbox returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("Chatterbox {kind} response exceeds {limit} bytes")]
    ResponseTooLarge { kind: &'static str, limit: usize },
    #[error("Chatterbox client: {0}")]
    InvalidResponse(String),
    #[error("could not communicate with Chatterbox: {0}")]
    Transport(#[source] reqwest::Error),
    #[error("Chatterbox request timed out")]
    Timeout,
    #[error("Chatterbox returned invalid WAV audio: {0}")]
    InvalidAudio(String),
    #[error("WAV validation worker failed: {0}")]
    ValidationWorker(#[from] tokio::task::JoinError),
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else {
            Self::Transport(error)
        }
    }
}

pub struct ChatterboxClient {
    http: reqwest::Client,
    options: ClientOptions,
    base_url: reqwest::Url,
}

impl ChatterboxClient {
    /// Generate one chapter, without retries, file writes, model switching, or hidden voice selection.
    pub async fn generate(&self, request: &SpeechRequest) -> Result<GeneratedAudio, Error> {
        request::validate(request, self.options.max_text_bytes)?;
        let response = self
            .http
            .post(self.endpoint("/tts"))
            .header(reqwest::header::ACCEPT, "audio/wav")
            .timeout(self.options.generation_timeout)
            .json(&request::WireRequest::from(request))
            .send()
            .await?;
        let response = response::checked(response, self.options.max_error_bytes).await?;
        response::expect_type(
            &response,
            &["audio/wav", "audio/x-wav", "audio/wave", "audio/vnd.wave"],
        )?;
        let bytes = response::limited(response, self.options.max_audio_bytes, "audio").await?;
        // Full sample validation can be substantial for a chapter. Keep it off
        // the async runtime's I/O thread without retaining a decoded sample buffer.
        tokio::task::spawn_blocking(move || audio::read(bytes)).await?
    }

    /// Validate local-only configuration. Construction makes no HTTP requests.
    pub fn new(options: ClientOptions) -> Result<Self, Error> {
        if [
            options.connect_timeout,
            options.discovery_timeout,
            options.generation_timeout,
        ]
        .iter()
        .any(Duration::is_zero)
        {
            return Err(Error::InvalidOptions("deadlines must be positive"));
        }
        if [
            options.max_audio_bytes,
            options.max_metadata_bytes,
            options.max_error_bytes,
            options.max_text_bytes,
        ]
        .contains(&0)
        {
            return Err(Error::InvalidOptions("byte budgets must be positive"));
        }
        let base_url = endpoint::parse(&options.base_url, options.network)?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(options.connect_timeout)
            .build()?;
        Ok(Self {
            http,
            options,
            base_url,
        })
    }

    /// List predefined voices. Success does not imply the GPU/model is ready.
    pub async fn voices(&self) -> Result<Vec<Voice>, Error> {
        let response = self
            .http
            .get(self.endpoint("/get_predefined_voices"))
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(self.options.discovery_timeout)
            .send()
            .await?;
        let response = response::checked(response, self.options.max_error_bytes).await?;
        response::expect_type(&response, &["application/json"])?;
        let bytes = response::limited(response, self.options.max_metadata_bytes, "voices").await?;
        voices::read(&bytes)
    }

    /// Report the loaded model. A server answering here may still be loading its model.
    pub async fn model_info(&self) -> Result<ModelInfo, Error> {
        let response = self
            .http
            .get(self.endpoint("/api/model-info"))
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(self.options.discovery_timeout)
            .send()
            .await?;
        let response = response::checked(response, self.options.max_error_bytes).await?;
        response::expect_type(&response, &["application/json"])?;
        let bytes =
            response::limited(response, self.options.max_metadata_bytes, "model info").await?;
        serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidResponse("model info is not the expected JSON".into()))
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut url = self.base_url.clone();
        url.set_path(path);
        url
    }
}
