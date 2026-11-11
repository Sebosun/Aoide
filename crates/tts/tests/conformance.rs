use aoide_tts::testing::{assert_valid_descriptor, assert_valid_synthesis};
use aoide_tts::{
    EngineDescriptor, ParamKind, ParamSpec, ParamValue, Params, Synthesis, SynthesisRequest,
};

fn descriptor() -> EngineDescriptor {
    EngineDescriptor {
        kind: "fake".into(),
        name: "Fake".into(),
        adapter_version: 1,
        languages: None,
        word_timings: true,
        max_chars: 100,
        params: vec![ParamSpec {
            key: "speed".into(),
            label: "Speed".into(),
            kind: ParamKind::Float {
                min: 0.5,
                max: 2.0,
                step: 0.1,
            },
            default: ParamValue::Float(1.0),
        }],
    }
}

fn request() -> SynthesisRequest {
    SynthesisRequest {
        text: "two words".into(),
        voice_id: "v".into(),
        language: None,
        params: Params::new(),
    }
}

#[test]
fn valid_descriptor_and_synthesis_pass() {
    assert_valid_descriptor(&descriptor());
    let synthesis = Synthesis {
        sample_rate: 1000,
        samples: vec![0.0; 1000],
        word_timings: Some(vec![(0, 400), (400, 1000)]),
    };
    assert_valid_synthesis(&request(), &synthesis);
}

#[test]
#[should_panic(expected = "fails its own spec")]
fn out_of_range_default_is_rejected() {
    let mut d = descriptor();
    d.params[0].default = ParamValue::Float(3.0);
    assert_valid_descriptor(&d);
}

#[test]
#[should_panic(expected = "duplicate param key")]
fn duplicate_keys_are_rejected() {
    let mut d = descriptor();
    d.params.push(d.params[0].clone());
    assert_valid_descriptor(&d);
}

#[test]
#[should_panic(expected = "one word timing per request word")]
fn word_timings_must_match_words() {
    let synthesis = Synthesis {
        sample_rate: 1000,
        samples: vec![0.0; 1000],
        word_timings: Some(vec![(0, 400)]),
    };
    assert_valid_synthesis(&request(), &synthesis);
}

#[test]
#[should_panic(expected = "within [-1, 1]")]
fn clipped_samples_are_rejected() {
    let synthesis = Synthesis {
        sample_rate: 1000,
        samples: vec![1.5],
        word_timings: None,
    };
    assert_valid_synthesis(&request(), &synthesis);
}
