//! [`aoide_tts::SpeechEngine`] adapter for devnen/Chatterbox-TTS-Server.

use std::sync::Arc;

use aoide_tts::{
    BoxFuture, EngineDescriptor, EngineError, EngineProfile, EngineStatus, ParamKind, ParamSpec,
    ParamValue, Registry, SpeechEngine, Synthesis, SynthesisRequest, Voice,
};

use crate::{ChatterboxClient, ClientOptions, Error, SpeechRequest};

pub const KIND: &str = "chatterbox";

/// Text above this many chars is split server-side; Aoide normally sends single sentences.
const MAX_UNSPLIT_CHARS: usize = 300;

/// Languages of Chatterbox Multilingual; the English-only models accept `en`.
const LANGUAGES: &[&str] = &[
    "ar", "da", "de", "el", "en", "es", "fi", "fr", "he", "hi", "it", "ja", "ko", "ms", "nl", "no",
    "pl", "pt", "ru", "sv", "sw", "tr", "zh",
];

pub fn descriptor() -> EngineDescriptor {
    let float = |key: &str, label: &str, min, max, step, default| ParamSpec {
        key: key.into(),
        label: label.into(),
        kind: ParamKind::Float { min, max, step },
        default: ParamValue::Float(default),
    };
    EngineDescriptor {
        kind: KIND.into(),
        name: "Chatterbox TTS Server".into(),
        adapter_version: 1,
        languages: Some(LANGUAGES.iter().map(|l| l.to_string()).collect()),
        word_timings: false,
        max_chars: MAX_UNSPLIT_CHARS,
        params: vec![
            float("temperature", "Temperature", 0.05, 2.0, 0.05, 0.8),
            float("exaggeration", "Exaggeration", 0.25, 2.0, 0.05, 0.5),
            float("cfg_weight", "CFG weight", 0.0, 1.0, 0.05, 0.5),
            float("speed_factor", "Speed factor", 0.25, 4.0, 0.05, 1.0),
            ParamSpec {
                key: "seed".into(),
                label: "Seed (0 = random)".into(),
                kind: ParamKind::Int {
                    min: 0,
                    max: i64::from(u32::MAX),
                },
                default: ParamValue::Int(0),
            },
        ],
    }
}

/// Adds the Chatterbox adapter to `registry`.
pub fn register(registry: &mut Registry) {
    registry.register(descriptor(), create);
}

fn create(profile: &EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError> {
    Ok(Arc::new(ChatterboxEngine::new(ClientOptions {
        base_url: profile.base_url.clone(),
        network: profile.network,
        ..Default::default()
    })?))
}

pub struct ChatterboxEngine {
    client: ChatterboxClient,
    descriptor: EngineDescriptor,
}

impl ChatterboxEngine {
    pub fn new(options: ClientOptions) -> Result<Self, EngineError> {
        Ok(Self {
            client: ChatterboxClient::new(options)?,
            descriptor: descriptor(),
        })
    }

    fn speech_request(request: &SynthesisRequest) -> SpeechRequest {
        let float = |key: &str| match request.params.get(key) {
            Some(ParamValue::Float(v)) => Some(*v as f32),
            Some(ParamValue::Int(v)) => Some(*v as f32),
            _ => None,
        };
        let mut speech = SpeechRequest::new(request.text.clone(), request.voice_id.clone());
        let settings = &mut speech.settings;
        settings.split_text = request.text.chars().count() > MAX_UNSPLIT_CHARS;
        settings.language = request.language.clone();
        settings.temperature = float("temperature");
        settings.exaggeration = float("exaggeration");
        settings.cfg_weight = float("cfg_weight");
        settings.speed_factor = float("speed_factor");
        settings.seed = match request.params.get("seed") {
            Some(ParamValue::Int(v)) => u32::try_from(*v).ok(),
            _ => None,
        };
        speech
    }
}

impl SpeechEngine for ChatterboxEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }

    fn status(&self) -> BoxFuture<'_, EngineStatus> {
        Box::pin(async move {
            match self.client.model_info().await {
                Ok(info) if info.loaded => EngineStatus::Ready(info.model_type),
                Ok(_) => EngineStatus::Loading,
                Err(error) => match EngineError::from(error) {
                    EngineError::Unavailable(_) => EngineStatus::Offline,
                    other => EngineStatus::Error(other.to_string()),
                },
            }
        })
    }

    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>> {
        Box::pin(async move {
            Ok(self
                .client
                .voices()
                .await?
                .into_iter()
                .map(|v| Voice {
                    id: v.id,
                    display_name: v.display_name,
                })
                .collect())
        })
    }

    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest,
    ) -> BoxFuture<'a, Result<Synthesis, EngineError>> {
        Box::pin(async move {
            let audio = self.client.generate(&Self::speech_request(request)).await?;
            aoide_tts::decode_wav(audio.as_bytes())
        })
    }
}

impl From<Error> for EngineError {
    fn from(error: Error) -> Self {
        match error {
            Error::InvalidEndpoint(m) | Error::InvalidOptions(m) => Self::InvalidConfig(m.into()),
            Error::InvalidInput(m) => Self::InvalidInput(m.into()),
            Error::Http { status, message } => Self::Http { status, message },
            Error::Timeout => Self::Timeout,
            Error::Transport(e) => Self::Unavailable(e.to_string()),
            Error::InvalidAudio(m) => Self::InvalidAudio(m),
            Error::ResponseTooLarge { .. } | Error::InvalidResponse(_) => {
                Self::InvalidResponse(error.to_string())
            }
            Error::ValidationWorker(e) => Self::InvalidResponse(e.to_string()),
        }
    }
}
