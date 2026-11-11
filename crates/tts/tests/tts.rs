use std::sync::Arc;

use aoide_tts::{
    cache_key, decode_wav, encode_wav, resolve_params, supports_language, validate_endpoint,
    BoxFuture, ChoiceOption, EngineDescriptor, EngineError, EngineProfile, EngineStatus,
    NetworkScope, ParamKind, ParamSpec, ParamValue, Params, Registry, SpeechEngine, Synthesis,
    SynthesisRequest, Voice,
};

fn descriptor() -> EngineDescriptor {
    EngineDescriptor {
        kind: "fake".into(),
        name: "Fake".into(),
        adapter_version: 1,
        languages: Some(vec!["en".into(), "pl".into()]),
        word_timings: false,
        max_chars: 300,
        params: vec![
            ParamSpec {
                key: "temperature".into(),
                label: "Temperature".into(),
                kind: ParamKind::Float {
                    min: 0.0,
                    max: 2.0,
                    step: 0.05,
                },
                default: ParamValue::Float(0.8),
            },
            ParamSpec {
                key: "seed".into(),
                label: "Seed".into(),
                kind: ParamKind::Int { min: 0, max: 100 },
                default: ParamValue::Int(0),
            },
            ParamSpec {
                key: "style".into(),
                label: "Style".into(),
                kind: ParamKind::Choice {
                    options: vec![
                        ChoiceOption {
                            value: "calm".into(),
                            label: "Calm".into(),
                        },
                        ChoiceOption {
                            value: "lively".into(),
                            label: "Lively".into(),
                        },
                    ],
                },
                default: ParamValue::Text("calm".into()),
            },
            ParamSpec {
                key: "trim".into(),
                label: "Trim silence".into(),
                kind: ParamKind::Bool,
                default: ParamValue::Bool(true),
            },
        ],
    }
}

fn request(text: &str) -> SynthesisRequest {
    SynthesisRequest {
        text: text.into(),
        voice_id: "Emily.wav".into(),
        language: Some("en".into()),
        params: resolve_params(&descriptor(), &Params::new()).unwrap(),
    }
}

fn wav(channels: u16, samples: &[i16]) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate: 24_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
    for s in samples {
        writer.write_sample(*s).unwrap();
    }
    writer.finalize().unwrap();
    cursor.into_inner()
}

#[test]
fn loopback_scope_accepts_only_loopback_origins() {
    let ok = |v| validate_endpoint(v, NetworkScope::Loopback).map(|u| u.to_string());
    assert_eq!(
        ok("http://localhost:8004").unwrap(),
        "http://127.0.0.1:8004/"
    );
    assert_eq!(ok("http://127.0.0.2:1").unwrap(), "http://127.0.0.2:1/");
    assert_eq!(ok("https://[::1]:9/").unwrap(), "https://[::1]:9/");
    for bad in [
        "http://192.168.1.5:8004",
        "http://example.com",
        "ftp://127.0.0.1",
        "http://u:p@127.0.0.1",
        "http://127.0.0.1/tts",
        "http://127.0.0.1/?q=1",
        "http://127.0.0.1/#x",
        "not a url",
    ] {
        assert!(
            matches!(
                validate_endpoint(bad, NetworkScope::Loopback),
                Err(EngineError::InvalidConfig(_))
            ),
            "{bad}"
        );
    }
}

#[test]
fn private_network_scope_adds_lan_addresses_but_not_public_or_names() {
    let scope = NetworkScope::PrivateNetwork;
    for good in [
        "http://192.168.1.5:8004",
        "http://10.0.0.2",
        "http://172.16.4.4",
        "http://169.254.1.1",
        "http://[fd00::1]:80",
        "http://[fe80::1]",
        "http://localhost:1",
    ] {
        assert!(validate_endpoint(good, scope).is_ok(), "{good}");
    }
    for bad in [
        "http://8.8.8.8",
        "http://172.32.0.1",
        "http://gpu-box.local",
        "http://[2001:db8::1]",
    ] {
        assert!(validate_endpoint(bad, scope).is_err(), "{bad}");
    }
}

#[test]
fn params_are_resolved_with_defaults_and_validated() {
    let d = descriptor();
    let resolved = resolve_params(&d, &Params::new()).unwrap();
    assert_eq!(resolved.len(), 4);
    assert_eq!(resolved["temperature"], ParamValue::Float(0.8));

    let mut overrides = Params::new();
    overrides.insert("temperature".into(), ParamValue::Int(1));
    overrides.insert("style".into(), ParamValue::Text("lively".into()));
    let resolved = resolve_params(&d, &overrides).unwrap();
    assert_eq!(
        resolved["temperature"],
        ParamValue::Float(1.0),
        "ints widen to floats"
    );
    assert_eq!(resolved["style"], ParamValue::Text("lively".into()));

    let reject = |key: &str, value: ParamValue| {
        let mut o = Params::new();
        o.insert(key.into(), value);
        assert!(
            matches!(resolve_params(&d, &o), Err(EngineError::InvalidInput(_))),
            "{key}"
        );
    };
    reject("temperature", ParamValue::Float(2.5));
    reject("temperature", ParamValue::Float(f64::NAN));
    reject("seed", ParamValue::Float(1.5));
    reject("seed", ParamValue::Int(101));
    reject("style", ParamValue::Text("angry".into()));
    reject("trim", ParamValue::Int(1));
    reject("unknown", ParamValue::Bool(true));
}

#[test]
fn cache_keys_are_stable_and_sensitive_to_every_input() {
    let d = descriptor();
    let base = cache_key(&d, Some("turbo"), &request("Hello there."));
    assert_eq!(base.len(), 64);
    assert!(base
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!(base, cache_key(&d, Some("turbo"), &request("Hello there.")));

    let mut different = vec![
        cache_key(&d, Some("turbo"), &request("Hello there!")),
        cache_key(&d, None, &request("Hello there.")),
    ];
    let mut r = request("Hello there.");
    r.voice_id = "Adrian.wav".into();
    different.push(cache_key(&d, Some("turbo"), &r));
    let mut r = request("Hello there.");
    r.language = None;
    different.push(cache_key(&d, Some("turbo"), &r));
    let mut r = request("Hello there.");
    r.params.insert("seed".into(), ParamValue::Int(5));
    different.push(cache_key(&d, Some("turbo"), &r));
    let mut d2 = d.clone();
    d2.adapter_version = 2;
    different.push(cache_key(&d2, Some("turbo"), &request("Hello there.")));
    let mut d3 = d.clone();
    d3.kind = "other".into();
    different.push(cache_key(&d3, Some("turbo"), &request("Hello there.")));
    for key in different {
        assert_ne!(key, base);
    }
}

#[test]
fn language_support_matches_primary_subtag() {
    let d = descriptor();
    assert!(supports_language(&d, "en"));
    assert!(supports_language(&d, "en-GB"));
    assert!(supports_language(&d, "PL"));
    assert!(!supports_language(&d, "la"));
    let mut any = d.clone();
    any.languages = None;
    assert!(supports_language(&any, "la"));
}

#[test]
fn synthesis_is_encoded_as_a_16_bit_mono_wav_that_decodes_back() {
    let audio = Synthesis {
        sample_rate: 22_050,
        samples: vec![0.0, 0.5, -1.0, 1.5],
        word_timings: None,
    };
    let bytes = encode_wav(&audio).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    assert_eq!(bytes.len(), 44 + 4 * 2);
    let decoded = decode_wav(&bytes).unwrap();
    assert_eq!(decoded.sample_rate, 22_050);
    assert_eq!(decoded.samples.len(), 4);
    for (got, want) in decoded.samples.iter().zip([0.0, 0.5, -1.0, 1.0]) {
        assert!((got - want).abs() < 1e-3, "{got} vs {want}");
    }
}

#[test]
fn wav_is_decoded_to_mono_floats_and_stereo_is_averaged() {
    let mono = decode_wav(&wav(1, &[0, 16384, -32768])).unwrap();
    assert_eq!(mono.sample_rate, 24_000);
    assert_eq!(mono.samples, vec![0.0, 0.5, -1.0]);
    assert_eq!(mono.word_timings, None);

    let stereo = decode_wav(&wav(2, &[16384, 0, -16384, -16384])).unwrap();
    assert_eq!(stereo.samples, vec![0.25, -0.5]);
    assert!(matches!(
        decode_wav(b"RIFFjunk"),
        Err(EngineError::InvalidAudio(_))
    ));
    assert!(matches!(
        decode_wav(&wav(1, &[])),
        Err(EngineError::InvalidAudio(_))
    ));
}

#[test]
fn synthesis_duration_rounds_down_to_whole_milliseconds() {
    let s = Synthesis {
        sample_rate: 24_000,
        samples: vec![0.0; 36_000],
        word_timings: None,
    };
    assert_eq!(s.duration_ms(), 1500);
}

struct Fake(EngineDescriptor);

impl SpeechEngine for Fake {
    fn descriptor(&self) -> &EngineDescriptor {
        &self.0
    }
    fn status(&self) -> BoxFuture<'_, EngineStatus> {
        Box::pin(async { EngineStatus::Ready(None) })
    }
    fn voices(&self) -> BoxFuture<'_, Result<Vec<Voice>, EngineError>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn synthesize<'a>(
        &'a self,
        _: &'a SynthesisRequest,
    ) -> BoxFuture<'a, Result<Synthesis, EngineError>> {
        Box::pin(async { Err(EngineError::Timeout) })
    }
}

fn fake_factory(_: &EngineProfile) -> Result<Arc<dyn SpeechEngine>, EngineError> {
    Ok(Arc::new(Fake(descriptor())))
}

fn profile(kind: &str, url: &str, network: NetworkScope) -> EngineProfile {
    EngineProfile {
        id: "p1".into(),
        name: "Mine".into(),
        kind: kind.into(),
        base_url: url.into(),
        model: None,
        network,
        voice_id: None,
        params: Params::new(),
        launch: None,
    }
}

#[test]
fn registry_creates_engines_for_known_kinds_within_scope() {
    let mut registry = Registry::new();
    registry.register(descriptor(), fake_factory);
    assert_eq!(registry.descriptors().len(), 1);
    let engine = registry
        .create(&profile(
            "fake",
            "http://127.0.0.1:1",
            NetworkScope::Loopback,
        ))
        .unwrap();
    assert_eq!(engine.descriptor().kind, "fake");
    assert!(matches!(
        registry.create(&profile(
            "nope",
            "http://127.0.0.1:1",
            NetworkScope::Loopback
        )),
        Err(EngineError::InvalidConfig(_))
    ));
    assert!(registry
        .create(&profile(
            "fake",
            "http://192.168.0.2:1",
            NetworkScope::Loopback
        ))
        .is_err());
    assert!(registry
        .create(&profile(
            "fake",
            "http://192.168.0.2:1",
            NetworkScope::PrivateNetwork
        ))
        .is_ok());
}

#[test]
fn profiles_and_descriptors_round_trip_through_json() {
    let p = profile("fake", "http://127.0.0.1:1", NetworkScope::PrivateNetwork);
    let json = serde_json::to_string(&p).unwrap();
    assert!(json.contains("\"network\":\"privateNetwork\""));
    assert_eq!(serde_json::from_str::<EngineProfile>(&json).unwrap(), p);
    let d = descriptor();
    assert_eq!(
        serde_json::from_str::<EngineDescriptor>(&serde_json::to_string(&d).unwrap()).unwrap(),
        d
    );
}
