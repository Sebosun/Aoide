//! Explicit opt-in contract smoke tests; never run by the default test command.
use aoide_chatterbox::{ChatterboxClient, ClientOptions, SpeechRequest};

fn client() -> ChatterboxClient {
    ChatterboxClient::new(ClientOptions {
        base_url: std::env::var("AOIDE_CHATTERBOX_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8004".into()),
        ..Default::default()
    })
    .unwrap()
}

#[tokio::test]
#[ignore = "requires a separately running local Chatterbox TTS Server"]
async fn lists_real_server_voices() {
    client()
        .voices()
        .await
        .expect("local voice-list request failed");
}

#[tokio::test]
#[ignore = "uses the local model/GPU; set AOIDE_CHATTERBOX_VOICE explicitly"]
async fn generates_a_short_original_sentence_on_the_real_server() {
    let voice = std::env::var("AOIDE_CHATTERBOX_VOICE")
        .expect("set AOIDE_CHATTERBOX_VOICE to an installed predefined voice filename");
    let mut request = SpeechRequest::new("Aoide local speech integration test.", voice);
    request.settings.seed = Some(0);
    let audio = client()
        .generate(&request)
        .await
        .expect("local speech generation failed");
    assert!(audio.metadata().frames > 0);
    assert!(audio.metadata().sample_rate > 0);
    assert!(audio.as_bytes().starts_with(b"RIFF"));
}

#[tokio::test]
#[ignore = "uses the local model/GPU; set AOIDE_CHATTERBOX_VOICE explicitly"]
async fn engine_adapter_reports_status_and_synthesizes_one_sentence() {
    use aoide_chatterbox::engine::{descriptor, ChatterboxEngine};
    use aoide_tts::{resolve_params, EngineStatus, Params, SpeechEngine, SynthesisRequest};
    let voice = std::env::var("AOIDE_CHATTERBOX_VOICE")
        .expect("set AOIDE_CHATTERBOX_VOICE to an installed predefined voice filename");
    let engine = ChatterboxEngine::new(ClientOptions {
        base_url: std::env::var("AOIDE_CHATTERBOX_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8004".into()),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(engine.status().await, EngineStatus::Ready(_)));
    let request = SynthesisRequest {
        text: "The boat reached the grey shore before the storm arrived.".into(),
        voice_id: voice,
        language: Some("en".into()),
        params: resolve_params(&descriptor(), &Params::new()).unwrap(),
    };
    let started = std::time::Instant::now();
    let synthesis = engine.synthesize(&request).await.unwrap();
    aoide_tts::testing::assert_valid_synthesis(&request, &synthesis);
    eprintln!(
        "audio {} ms generated in {} ms",
        synthesis.duration_ms(),
        started.elapsed().as_millis()
    );
    assert!(synthesis.duration_ms() > 1000);
}
