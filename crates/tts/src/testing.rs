//! Conformance checks every adapter's tests should run against a fake server.

use crate::{check_param, EngineDescriptor, ParamKind, Synthesis, SynthesisRequest};

/// Panics with a message when the descriptor is internally inconsistent: empty kind/name,
/// zero `max_chars`, duplicate or empty param keys, defaults that fail their own spec,
/// empty choice lists, or inverted numeric ranges.
pub fn assert_valid_descriptor(descriptor: &EngineDescriptor) {
    assert!(
        !descriptor.kind.trim().is_empty(),
        "descriptor kind is empty"
    );
    assert!(
        !descriptor.name.trim().is_empty(),
        "descriptor name is empty"
    );
    assert!(descriptor.max_chars > 0, "max_chars must be positive");
    if let Some(languages) = &descriptor.languages {
        assert!(
            !languages.is_empty(),
            "use None for any language, not an empty list"
        );
    }
    let mut keys = std::collections::BTreeSet::new();
    for spec in &descriptor.params {
        assert!(!spec.key.trim().is_empty(), "param key is empty");
        assert!(keys.insert(&spec.key), "duplicate param key `{}`", spec.key);
        match &spec.kind {
            ParamKind::Float { min, max, step } => {
                assert!(
                    min <= max && *step > 0.0,
                    "bad float range for `{}`",
                    spec.key
                )
            }
            ParamKind::Int { min, max } => assert!(min <= max, "bad int range for `{}`", spec.key),
            ParamKind::Choice { options } => {
                assert!(!options.is_empty(), "choice `{}` has no options", spec.key)
            }
            ParamKind::Bool => {}
        }
        assert!(
            check_param(spec, &spec.default).is_ok(),
            "default for `{}` fails its own spec",
            spec.key
        );
    }
}

/// Panics when a synthesis result is not usable by Aoide: zero sample rate, no samples,
/// non-finite or out-of-range samples, or word timings that are not one per word, ordered,
/// non-overlapping and within the audio duration.
pub fn assert_valid_synthesis(request: &SynthesisRequest, synthesis: &Synthesis) {
    assert!(synthesis.sample_rate > 0, "sample rate is zero");
    assert!(!synthesis.samples.is_empty(), "no samples");
    assert!(
        synthesis
            .samples
            .iter()
            .all(|s| s.is_finite() && (-1.0..=1.0).contains(s)),
        "samples must be finite and within [-1, 1]"
    );
    if let Some(timings) = &synthesis.word_timings {
        let words = request.text.split_whitespace().count();
        assert_eq!(timings.len(), words, "one word timing per request word");
        let duration = synthesis.duration_ms() as u32;
        let mut previous_end = 0;
        for &(start, end) in timings {
            assert!(
                start >= previous_end && start <= end,
                "word timings out of order"
            );
            assert!(end <= duration + 1, "word timing beyond audio duration");
            previous_end = end;
        }
    }
}

/// An engine that answers at once with a short silence and remembers what it was asked to say.
/// `register` puts it in a registry as kind `"fake"`; every profile of that kind shares it.
pub struct FakeEngine {
    descriptor: EngineDescriptor,
    calls: std::sync::Mutex<Vec<SynthesisRequest>>,
    voices: Vec<String>,
}

impl FakeEngine {
    /// The voice of `new()`.
    pub const VOICE: &'static str = "Test.wav";

    pub fn new() -> std::sync::Arc<Self> {
        Self::with_voices(&[Self::VOICE])
    }

    /// An engine offering these voices, in this order.
    pub fn with_voices(voices: &[&str]) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            voices: voices.iter().map(|v| v.to_string()).collect(),
            descriptor: EngineDescriptor {
                kind: "fake".into(),
                name: "Fake".into(),
                adapter_version: 1,
                languages: None,
                word_timings: false,
                max_chars: 400,
                params: vec![],
            },
            calls: std::sync::Mutex::new(vec![]),
        })
    }

    /// Texts synthesised so far, in order.
    pub fn calls(&self) -> Vec<String> {
        self.requests().into_iter().map(|r| r.text).collect()
    }

    /// Requests synthesised so far, in order.
    pub fn requests(&self) -> Vec<SynthesisRequest> {
        self.calls.lock().expect("calls lock").clone()
    }

    pub fn register(self: &std::sync::Arc<Self>, registry: &mut crate::Registry) {
        let engine = std::sync::Arc::clone(self);
        registry.register(self.descriptor.clone(), move |_| {
            Ok(std::sync::Arc::clone(&engine) as std::sync::Arc<dyn crate::SpeechEngine>)
        });
    }
}

impl crate::SpeechEngine for FakeEngine {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.descriptor
    }
    fn status(&self) -> crate::BoxFuture<'_, crate::EngineStatus> {
        Box::pin(async { crate::EngineStatus::Ready(None) })
    }
    fn voices(&self) -> crate::BoxFuture<'_, Result<Vec<crate::Voice>, crate::EngineError>> {
        let voices = self
            .voices
            .iter()
            .map(|id| crate::Voice {
                id: id.clone(),
                display_name: id.trim_end_matches(".wav").into(),
            })
            .collect();
        Box::pin(async { Ok(voices) })
    }
    fn synthesize<'a>(
        &'a self,
        request: &'a SynthesisRequest,
    ) -> crate::BoxFuture<'a, Result<Synthesis, crate::EngineError>> {
        self.calls.lock().expect("calls lock").push(request.clone());
        Box::pin(async {
            Ok(Synthesis {
                sample_rate: 24_000,
                samples: vec![0.0; 7_200],
                word_timings: None,
            })
        })
    }
}
