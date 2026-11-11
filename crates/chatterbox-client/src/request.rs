use crate::{Error, SpeechRequest};

pub(crate) fn validate(request: &SpeechRequest, max_text_bytes: usize) -> Result<(), Error> {
    if request.text.trim().is_empty() {
        return Err(Error::InvalidInput("text must not be blank"));
    }
    if request.text.len() > max_text_bytes {
        return Err(Error::InvalidInput(
            "text exceeds the configured byte limit",
        ));
    }
    if !valid_voice_id(&request.voice_id) {
        return Err(Error::InvalidInput(
            "voice must be a WAV/MP3 filename without path separators or control characters",
        ));
    }
    if !(50..=500).contains(&request.settings.chunk_size) {
        return Err(Error::InvalidInput(
            "chunk size must be between 50 and 500 characters",
        ));
    }
    if request
        .settings
        .language
        .as_ref()
        .is_some_and(|s| s.trim().is_empty())
    {
        return Err(Error::InvalidInput(
            "language must not be blank when provided",
        ));
    }
    Ok(())
}

pub(crate) fn valid_voice_id(id: &str) -> bool {
    !id.trim().is_empty()
        && id.trim() == id
        && !id.contains(['/', '\\', ':'])
        && !id.chars().any(char::is_control)
        && id.rsplit_once('.').is_some_and(|(stem, extension)| {
            !stem.is_empty() && matches!(extension.to_ascii_lowercase().as_str(), "wav" | "mp3")
        })
}

#[derive(serde::Serialize)]
pub(crate) struct WireRequest<'a> {
    text: &'a str,
    voice_mode: &'static str,
    predefined_voice_id: &'a str,
    output_format: &'static str,
    split_text: bool,
    chunk_size: u16,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exaggeration: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cfg_weight: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed_factor: Option<f32>,
}

impl<'a> From<&'a SpeechRequest> for WireRequest<'a> {
    fn from(request: &'a SpeechRequest) -> Self {
        Self {
            text: &request.text,
            voice_mode: "predefined",
            predefined_voice_id: &request.voice_id,
            output_format: "wav",
            split_text: request.settings.split_text,
            chunk_size: request.settings.chunk_size,
            stream: false,
            seed: request.settings.seed,
            language: request.settings.language.as_deref(),
            temperature: request.settings.temperature,
            exaggeration: request.settings.exaggeration,
            cfg_weight: request.settings.cfg_weight,
            speed_factor: request.settings.speed_factor,
        }
    }
}
