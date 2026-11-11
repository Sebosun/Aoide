//! [`aoide_tts::SpeechEngine`] adapter for local servers speaking the OpenAI-style speech API
//! (`POST /v1/audio/speech`, `GET /v1/audio/voices`), e.g. Kokoro-FastAPI or openedai-speech.

use std::sync::Arc;
use std::time::Duration;

use aoide_tts::{
    validate_endpoint, BoxFuture, EngineDescriptor, EngineError, EngineProfile, EngineStatus,
    NetworkScope, ParamKind, ParamSpec, ParamValue, Registry, SpeechEngine, Synthesis,
    SynthesisRequest, Voice,
};

pub const KIND: &str = "openai-speech";

pub fn descriptor() -> EngineDescriptor {
    EngineDescriptor {
        kind: KIND.into(),
        name: "OpenAI-compatible speech server".into(),
        adapter_version: 1,
        languages: None,
        word_timings: false,
        max_chars: 1000,
        params: vec![ParamSpec {
            key: "speed".into(),
            label: "Speed".into(),
            kind: ParamKind::Float {
                min: 0.25,
                max: 4.0,
                step: 0.05,
            },
            default: ParamValue::Float(1.0),
        }],
    }
}

pub fn register(registry: &mut Registry) {
    registry.register(descriptor(), create);
}

fn create(profile: &EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError> {
    Ok(Arc::new(OpenAiSpeechEngine::new(Options {
        base_url: profile.base_url.clone(),
        network: profile.network,
        model: profile.model.clone(),
        ..Default::default()
    })?))
}

#[derive(Debug, Clone)]
pub struct Options {
    pub base_url: String,
    pub network: NetworkScope,
    /// Sent as `model`; `None` sends `"tts-1"`.
    pub model: Option<String>,
    pub connect_timeout: Duration,
    pub discovery_timeout: Duration,
    pub generation_timeout: Duration,
    pub max_audio_bytes: usize,
    pub max_text_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8880".into(),
            network: NetworkScope::Loopback,
            model: None,
            connect_timeout: Duration::from_secs(2),
            discovery_timeout: Duration::from_secs(5),
            generation_timeout: Duration::from_secs(10 * 60),
            max_audio_bytes: 64 * 1024 * 1024,
            max_text_bytes: 64 * 1024,
        }
    }
}

pub struct OpenAiSpeechEngine {
    descriptor: EngineDescriptor,
    http: reqwest::Client,
    base_url: reqwest::Url,
    options: Options,
}

const MAX_ERROR_BYTES: usize = 8 * 1024;
const MAX_METADATA_BYTES: usize = 1024 * 1024;
const WAV_TYPES: &[&str] = &["audio/wav", "audio/x-wav", "audio/wave", "audio/vnd.wave"];

impl OpenAiSpeechEngine {
    /// Validates the endpoint for its network scope. Construction makes no requests.
    pub fn new(options: Options) -> Result<Self, EngineError> {
        let invalid = |m: &str| EngineError::InvalidConfig(m.into());
        if [
            options.connect_timeout,
            options.discovery_timeout,
            options.generation_timeout,
        ]
        .iter()
        .any(Duration::is_zero)
        {
            return Err(invalid("deadlines must be positive"));
        }
        if options.max_audio_bytes == 0 || options.max_text_bytes == 0 {
            return Err(invalid("byte budgets must be positive"));
        }
        let base_url = validate_endpoint(&options.base_url, options.network)?;
        let base_url = reqwest::Url::parse(base_url.as_str())
            .map_err(|_| invalid("expected an HTTP(S) URL"))?;
        let http = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(options.connect_timeout)
            .build()
            .map_err(|e| EngineError::InvalidConfig(e.to_string()))?;
        Ok(Self {
            descriptor: descriptor(),
            http,
            base_url,
            options,
        })
    }

    fn model(&self) -> &str {
        self.options
            .model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .unwrap_or("tts-1")
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        let mut url = self.base_url.clone();
        url.set_path(path);
        url
    }

    async fn fetch_voices(&self) -> Result<Vec<Voice>, EngineError> {
        let response = self
            .http
            .get(self.endpoint("/v1/audio/voices"))
            .header(reqwest::header::ACCEPT, "application/json")
            .timeout(self.options.discovery_timeout)
            .send()
            .await
            .map_err(transport)?;
        let response = checked(response).await?;
        let bytes = limited(response, MAX_METADATA_BYTES).await?;
        parse_voices(&bytes)
    }

    async fn speak(&self, request: &SynthesisRequest) -> Result<Synthesis, EngineError> {
        let text = request.text.trim();
        if text.is_empty() || !text.chars().any(char::is_alphanumeric) {
            return Err(EngineError::InvalidInput("text is empty".into()));
        }
        if request.text.len() > self.options.max_text_bytes {
            return Err(EngineError::InvalidInput("text is too long".into()));
        }
        if request.voice_id.trim().is_empty() {
            return Err(EngineError::InvalidInput("no voice selected".into()));
        }
        let speed = match request.params.get("speed") {
            Some(ParamValue::Float(v)) => *v,
            Some(ParamValue::Int(v)) => *v as f64,
            _ => 1.0,
        };
        let mut body = serde_json::json!({
            "model": self.model(),
            "input": request.text,
            "voice": request.voice_id,
            "response_format": "wav",
            "speed": speed,
        });
        if let Some(language) = &request.language {
            body["language"] = serde_json::Value::String(language.clone());
        }
        let response = self
            .http
            .post(self.endpoint("/v1/audio/speech"))
            .header(reqwest::header::ACCEPT, "audio/wav")
            .timeout(self.options.generation_timeout)
            .json(&body)
            .send()
            .await
            .map_err(transport)?;
        let response = checked(response).await?;
        if !media_type(&response)
            .is_some_and(|t| WAV_TYPES.iter().any(|w| t.eq_ignore_ascii_case(w)))
        {
            return Err(EngineError::InvalidResponse(
                "expected WAV audio (response_format wav)".into(),
            ));
        }
        let bytes = limited(response, self.options.max_audio_bytes).await?;
        aoide_tts::decode_wav(&bytes)
    }
}

impl SpeechEngine for OpenAiSpeechEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn status(&self) -> BoxFuture<'_, EngineStatus> {
        Box::pin(async move {
            match self.fetch_voices().await {
                Ok(_) => EngineStatus::Ready(Some(self.model().to_owned())),
                Err(EngineError::Unavailable(_)) => EngineStatus::Offline,
                Err(error) => EngineStatus::Error(error.to_string()),
            }
        })
    }

    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>> {
        Box::pin(self.fetch_voices())
    }

    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest,
    ) -> BoxFuture<'a, Result<Synthesis, EngineError>> {
        Box::pin(self.speak(request))
    }
}

fn transport(error: reqwest::Error) -> EngineError {
    if error.is_timeout() {
        EngineError::Timeout
    } else {
        EngineError::Unavailable(error.to_string())
    }
}

fn media_type(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)?
        .to_str()
        .ok()?
        .split(';')
        .next()
        .map(|t| t.trim().to_ascii_lowercase())
}

async fn limited(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, EngineError> {
    let too_large = || EngineError::InvalidResponse(format!("response exceeds {limit} bytes"));
    if response.content_length().is_some_and(|l| l > limit as u64) {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if chunk.len() > limit - bytes.len() {
            return Err(too_large());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// Passes 200 through; otherwise reads a bounded error body into `EngineError::Http`.
async fn checked(mut response: reqwest::Response) -> Result<reqwest::Response, EngineError> {
    let status = response.status();
    if status == reqwest::StatusCode::OK {
        return Ok(response);
    }
    let kind = media_type(&response).unwrap_or_default();
    let mut bytes = Vec::new();
    while bytes.len() < MAX_ERROR_BYTES {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                bytes.extend_from_slice(&chunk[..chunk.len().min(MAX_ERROR_BYTES - bytes.len())])
            }
            _ => break,
        }
    }
    Err(EngineError::Http {
        status: status.as_u16(),
        message: error_message(&kind, &bytes)
            .unwrap_or_else(|| status.canonical_reason().unwrap_or("error").to_owned()),
    })
}

fn error_message(media_type: &str, bytes: &[u8]) -> Option<String> {
    let clean = |s: &str| {
        let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
        let s: String = s.chars().take(500).collect();
        (!s.is_empty() && !s.contains('<')).then_some(s)
    };
    if media_type.contains("json") {
        let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let text = |v: &serde_json::Value| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Null => None,
            other => Some(other.to_string()),
        };
        let message = value
            .get("detail")
            .and_then(text)
            .or_else(|| {
                value
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(text)
            })
            .or_else(|| value.get("error").and_then(text))
            .or_else(|| value.get("message").and_then(text))?;
        return clean(&message);
    }
    if media_type.starts_with("text/plain") {
        return clean(&String::from_utf8_lossy(bytes));
    }
    None
}

fn parse_voices(bytes: &[u8]) -> Result<Vec<Voice>, EngineError> {
    let invalid = || EngineError::InvalidResponse("voice list is not recognised JSON".into());
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let list = match &value {
        serde_json::Value::Array(items) => items,
        serde_json::Value::Object(map) => match map.get("voices") {
            Some(serde_json::Value::Array(items)) => items,
            _ => return Err(invalid()),
        },
        _ => return Err(invalid()),
    };
    list.iter()
        .map(|item| {
            let field = |keys: &[&str]| {
                keys.iter()
                    .find_map(|k| item.get(*k).and_then(|v| v.as_str()))
                    .map(str::to_owned)
            };
            let id = match item {
                serde_json::Value::String(s) => Some(s.clone()),
                _ => field(&["id", "voice_id", "name"]),
            }
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(invalid)?;
            let display_name = field(&["display_name", "name"]).unwrap_or_else(|| id.clone());
            Ok(Voice { id, display_name })
        })
        .collect()
}
