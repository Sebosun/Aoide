//! Aoide's text-to-speech engine interface.
//!
//! An adapter implements [`SpeechEngine`] for one engine protocol and registers a factory in a
//! [`Registry`] under its `kind`. Engine profiles saved by the user name a kind, an endpoint and
//! default voice/parameters; the registry turns a profile into a live engine.
//!
//! Adapters must not retry, follow redirects, or use proxies, and must only contact endpoints
//! accepted by [`validate_endpoint`] for the profile's [`NetworkScope`].

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[cfg(feature = "testing")]
pub mod testing;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Static description of what an adapter supports. Drives the generic settings UI.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EngineDescriptor {
    /// Registry key, e.g. `"chatterbox"`.
    pub kind: String,
    pub name: String,
    /// Bump when the adapter changes what audio it produces for the same input; part of cache keys.
    pub adapter_version: u32,
    /// BCP-47 primary language subtags (`"en"`); `None` means the engine accepts any language.
    pub languages: Option<Vec<String>>,
    /// Whether `Synthesis::word_timings` is populated.
    pub word_timings: bool,
    /// Longest text (in chars) one request should carry.
    pub max_chars: usize,
    pub params: Vec<ParamSpec>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ParamSpec {
    pub key: String,
    pub label: String,
    pub kind: ParamKind,
    pub default: ParamValue,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ParamKind {
    Float { min: f64, max: f64, step: f64 },
    Int { min: i64, max: i64 },
    Choice { options: Vec<ChoiceOption> },
    Bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ChoiceOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ParamValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

pub type Params = BTreeMap<String, ParamValue>;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", content = "detail", rename_all = "camelCase")]
pub enum EngineStatus {
    /// Nothing is listening at the endpoint.
    Offline,
    /// The server answers but its model is not ready.
    Loading,
    /// Ready; optional loaded-model description.
    Ready(Option<String>),
    /// The server answered with an error.
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Voice {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesisRequest {
    pub text: String,
    pub voice_id: String,
    pub language: Option<String>,
    /// Complete parameter set, normally produced by [`resolve_params`].
    pub params: Params,
}

/// Mono audio for one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Synthesis {
    pub sample_rate: u32,
    /// Mono samples in `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
    /// `[start_ms, end_ms)` per whitespace-separated word of the request text, when supported.
    pub word_timings: Option<Vec<(u32, u32)>>,
}

impl Synthesis {
    pub fn duration_ms(&self) -> u64 {
        if self.sample_rate == 0 {
            return 0;
        }
        self.samples.len() as u64 * 1000 / u64::from(self.sample_rate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("invalid engine configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid request: {0}")]
    InvalidInput(String),
    #[error("engine is unavailable: {0}")]
    Unavailable(String),
    #[error("engine request timed out")]
    Timeout,
    #[error("engine returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("engine returned an invalid response: {0}")]
    InvalidResponse(String),
    #[error("engine returned invalid audio: {0}")]
    InvalidAudio(String),
}

/// One TTS engine instance (an adapter bound to an endpoint).
pub trait SpeechEngine: Send + Sync {
    fn descriptor(&self) -> &EngineDescriptor;
    fn status(&self) -> BoxFuture<'_, EngineStatus>;
    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>>;
    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest,
    ) -> BoxFuture<'a, Result<Synthesis, EngineError>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum NetworkScope {
    /// 127.0.0.0/8, ::1 and `localhost` (pinned to 127.0.0.1).
    #[default]
    Loopback,
    /// Loopback plus RFC 1918 / link-local IPv4 and unique-local / link-local IPv6 addresses.
    PrivateNetwork,
}

/// Validates an engine origin: HTTP(S), IP literal or `localhost`, no credentials, path, query
/// or fragment, and an address inside `scope`. Returns the normalised origin.
pub fn validate_endpoint(value: &str, scope: NetworkScope) -> Result<url::Url, EngineError> {
    let invalid = |message: &str| EngineError::InvalidConfig(message.into());
    let mut url = url::Url::parse(value).map_err(|_| invalid("expected an HTTP(S) URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            "use an HTTP(S) origin without credentials, path, query, or fragment",
        ));
    }
    let ip = match url.host() {
        Some(url::Host::Domain("localhost")) => {
            // Pin localhost to loopback; do not depend on DNS or the hosts file.
            url.set_host(Some("127.0.0.1"))
                .map_err(|_| invalid("invalid loopback host"))?;
            return Ok(url);
        }
        Some(url::Host::Ipv4(ip)) => std::net::IpAddr::V4(ip),
        Some(url::Host::Ipv6(ip)) => std::net::IpAddr::V6(ip),
        _ => return Err(invalid("use localhost or an IP address, not a host name")),
    };
    let allowed = ip.is_loopback() || (scope == NetworkScope::PrivateNetwork && private(ip));
    if !allowed {
        return Err(invalid(match scope {
            NetworkScope::Loopback => "only loopback addresses are permitted for this engine",
            NetworkScope::PrivateNetwork => {
                "only loopback or private-network addresses are permitted"
            }
        }));
    }
    Ok(url)
}

fn private(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        std::net::IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// Fills defaults for missing parameters and validates every value against its spec.
/// Unknown keys, wrong types and out-of-range values are rejected.
pub fn resolve_params(
    descriptor: &EngineDescriptor,
    overrides: &Params,
) -> Result<Params, EngineError> {
    if let Some(key) = overrides
        .keys()
        .find(|key| !descriptor.params.iter().any(|p| &p.key == *key))
    {
        return Err(EngineError::InvalidInput(format!(
            "unknown parameter `{key}`"
        )));
    }
    descriptor
        .params
        .iter()
        .map(|spec| {
            let value = overrides.get(&spec.key).unwrap_or(&spec.default);
            check_param(spec, value).map(|value| (spec.key.clone(), value))
        })
        .collect()
}

/// The value normalised to the spec's type, or an error naming the parameter.
pub(crate) fn check_param(spec: &ParamSpec, value: &ParamValue) -> Result<ParamValue, EngineError> {
    let bad = || EngineError::InvalidInput(format!("invalid value for `{}`", spec.key));
    match (&spec.kind, value) {
        (ParamKind::Float { min, max, .. }, ParamValue::Float(v))
            if v.is_finite() && (*min..=*max).contains(v) =>
        {
            Ok(ParamValue::Float(*v))
        }
        (ParamKind::Float { min, max, .. }, ParamValue::Int(v))
            if (*min..=*max).contains(&(*v as f64)) =>
        {
            Ok(ParamValue::Float(*v as f64))
        }
        (ParamKind::Int { min, max }, ParamValue::Int(v)) if (*min..=*max).contains(v) => {
            Ok(ParamValue::Int(*v))
        }
        (ParamKind::Choice { options }, ParamValue::Text(v))
            if options.iter().any(|o| &o.value == v) =>
        {
            Ok(ParamValue::Text(v.clone()))
        }
        (ParamKind::Bool, ParamValue::Bool(v)) => Ok(ParamValue::Bool(*v)),
        _ => Err(bad()),
    }
}

/// Lower-case hex SHA-256 identifying the audio a request will produce with this adapter.
pub fn cache_key(
    descriptor: &EngineDescriptor,
    model: Option<&str>,
    request: &SynthesisRequest,
) -> String {
    use sha2::{Digest, Sha256};
    // Canonical JSON: struct field order is fixed and params are a sorted BTreeMap.
    let canonical = serde_json::json!({
        "v": 1,
        "kind": descriptor.kind,
        "adapter": descriptor.adapter_version,
        "model": model,
        "text": request.text,
        "voice": request.voice_id,
        "language": request.language,
        "params": request.params,
    });
    format!("{:x}", Sha256::digest(canonical.to_string().as_bytes()))
}

/// Whether `language` (any BCP-47 tag) is supported according to the descriptor.
pub fn supports_language(descriptor: &EngineDescriptor, language: &str) -> bool {
    let primary = language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    descriptor
        .languages
        .as_ref()
        .is_none_or(|languages| languages.iter().any(|l| l.eq_ignore_ascii_case(&primary)))
}

/// Encodes mono audio as a 16-bit PCM WAV file (samples outside `[-1, 1]` are clipped), for
/// playing a short clip such as a voice preview in the UI.
pub fn encode_wav(audio: &Synthesis) -> Result<Vec<u8>, EngineError> {
    let invalid = |e: hound::Error| EngineError::InvalidAudio(e.to_string());
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: audio.sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = std::io::Cursor::new(Vec::with_capacity(44 + audio.samples.len() * 2));
    let mut writer = hound::WavWriter::new(&mut bytes, spec).map_err(invalid)?;
    for sample in &audio.samples {
        let clipped = if sample.is_finite() {
            sample.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        writer
            .write_sample((clipped * f32::from(i16::MAX)).round() as i16)
            .map_err(invalid)?;
    }
    writer.finalize().map_err(invalid)?;
    Ok(bytes.into_inner())
}

/// Decodes a complete WAV file (PCM 8–32 bit or 32-bit float, any channel count) to mono f32.
pub fn decode_wav(bytes: &[u8]) -> Result<Synthesis, EngineError> {
    let invalid = |e: hound::Error| EngineError::InvalidAudio(e.to_string());
    let mut reader = hound::WavReader::new(bytes).map_err(invalid)?;
    let spec = reader.spec();
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err(EngineError::InvalidAudio(
            "sample rate and channel count must be positive".into(),
        ));
    }
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|s| s as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(invalid)?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(invalid)?,
    };
    let channels = usize::from(spec.channels);
    if interleaved.is_empty() || !interleaved.len().is_multiple_of(channels) {
        return Err(EngineError::InvalidAudio(
            "WAV must contain complete, nonempty sample frames".into(),
        ));
    }
    if interleaved.iter().any(|s| !s.is_finite()) {
        return Err(EngineError::InvalidAudio("non-finite sample".into()));
    }
    let samples = interleaved
        .chunks_exact(channels)
        .map(|frame| (frame.iter().sum::<f32>() / channels as f32).clamp(-1.0, 1.0))
        .collect();
    Ok(Synthesis {
        sample_rate: spec.sample_rate,
        samples,
        word_timings: None,
    })
}

/// A saved, user-editable engine configuration.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct EngineProfile {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    /// Model name for engines that serve several (sent as-is); `None` uses the server default.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub network: NetworkScope,
    /// Default voice for books that do not override it.
    pub voice_id: Option<String>,
    #[serde(default)]
    pub params: Params,
    /// Optional command that starts the engine server.
    pub launch: Option<LaunchCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LaunchCommand {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub working_dir: Option<String>,
}

/// Builds an adapter from a profile. A plain `fn` works; tests can pass a closure that hands
/// out a shared fake.
pub type EngineFactory =
    Arc<dyn Fn(&EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError> + Send + Sync>;

/// Adapters by kind.
#[derive(Default)]
pub struct Registry {
    adapters: BTreeMap<String, (EngineDescriptor, EngineFactory)>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an adapter; a later registration of the same kind replaces the earlier one.
    pub fn register(
        &mut self,
        descriptor: EngineDescriptor,
        factory: impl Fn(&EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError>
            + Send
            + Sync
            + 'static,
    ) {
        self.adapters
            .insert(descriptor.kind.clone(), (descriptor, Arc::new(factory)));
    }

    /// Descriptors of all registered adapters, sorted by kind.
    pub fn descriptors(&self) -> Vec<EngineDescriptor> {
        self.adapters.values().map(|(d, _)| d.clone()).collect()
    }

    pub fn descriptor(&self, kind: &str) -> Option<&EngineDescriptor> {
        self.adapters.get(kind).map(|(d, _)| d)
    }

    /// Validates the profile endpoint for its scope and builds the engine.
    pub fn create(&self, profile: &EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError> {
        let (_, factory) = self.adapters.get(&profile.kind).ok_or_else(|| {
            EngineError::InvalidConfig(format!("no adapter for engine kind `{}`", profile.kind))
        })?;
        validate_endpoint(&profile.base_url, profile.network)?;
        factory(profile)
    }
}
