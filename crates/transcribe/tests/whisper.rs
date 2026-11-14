//! Against a real model, opt-in: `AOIDE_WHISPER_MODEL=/path/ggml-base.en.bin cargo test -p
//! aoide-transcribe --features whisper --test whisper -- --ignored`.
#![cfg(feature = "whisper")]

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use aoide_transcribe::{Error, Request, Transcriber, WhisperCpp, WhisperOptions, SAMPLE_RATE};

fn whisper() -> WhisperCpp {
    let model = std::env::var_os("AOIDE_WHISPER_MODEL").expect("set AOIDE_WHISPER_MODEL");
    WhisperCpp::new(WhisperOptions {
        model: model.into(),
        threads: 0,
        use_gpu: false,
        vad_model: None,
    })
    .unwrap()
}

#[test]
#[ignore = "needs a whisper model"]
fn an_abort_flag_stops_recognition_only_once_it_is_set() {
    let whisper = whisper();
    let silence = vec![0.0f32; SAMPLE_RATE as usize * 3];
    let hear = |stop: bool| {
        whisper.transcribe(&Request {
            samples: &silence,
            language: Some("en"),
            abort: Some(Arc::new(AtomicBool::new(stop))),
        })
    };
    for _ in 0..3 {
        assert!(hear(false).is_ok(), "an unset flag lets it finish");
    }
    assert!(matches!(hear(true), Err(Error::Aborted)));
}
