//! `SpeechEngine` adapter behaviour over loopback HTTP with synthetic responses.
mod support;

use aoide_chatterbox::engine::{descriptor, ChatterboxEngine};
use aoide_chatterbox::ClientOptions;
use aoide_tts::testing::{assert_valid_descriptor, assert_valid_synthesis};
use aoide_tts::{
    resolve_params, EngineError, EngineProfile, EngineStatus, NetworkScope, ParamValue, Params,
    SpeechEngine, SynthesisRequest,
};
use support::{Response, Server};

fn engine(url: &str) -> ChatterboxEngine {
    ChatterboxEngine::new(ClientOptions {
        base_url: url.into(),
        ..Default::default()
    })
    .unwrap()
}

fn request(text: &str) -> SynthesisRequest {
    let mut overrides = Params::new();
    overrides.insert("temperature".into(), ParamValue::Float(0.7));
    overrides.insert("exaggeration".into(), ParamValue::Float(1.25));
    overrides.insert("seed".into(), ParamValue::Int(42));
    SynthesisRequest {
        text: text.into(),
        voice_id: "Emily.wav".into(),
        language: Some("en".into()),
        params: resolve_params(&descriptor(), &overrides).unwrap(),
    }
}

#[test]
fn descriptor_passes_conformance_and_declares_chatterbox_settings() {
    let d = descriptor();
    assert_valid_descriptor(&d);
    assert_eq!(d.kind, "chatterbox");
    let keys: Vec<&str> = d.params.iter().map(|p| p.key.as_str()).collect();
    for key in [
        "temperature",
        "exaggeration",
        "cfg_weight",
        "speed_factor",
        "seed",
    ] {
        assert!(keys.contains(&key), "{key}");
    }
    assert!(d.languages.as_ref().unwrap().contains(&"en".to_string()));
    assert!(!d.languages.as_ref().unwrap().contains(&"la".to_string()));
}

#[tokio::test]
async fn short_segments_are_sent_unsplit_with_all_settings() {
    let mut server = Server::start(Response::wav(support::wav())).await;
    let engine = engine(&server.url);
    let req = request("A short original sentence.");
    let synthesis = engine.synthesize(&req).await.unwrap();
    assert_valid_synthesis(&req, &synthesis);
    assert_eq!(synthesis.sample_rate, 24_000);
    assert_eq!(synthesis.samples.len(), 64);

    let sent = server.request().await;
    assert_eq!((sent.method.as_str(), sent.path.as_str()), ("POST", "/tts"));
    let body: serde_json::Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body["text"], "A short original sentence.");
    assert_eq!(body["predefined_voice_id"], "Emily.wav");
    assert_eq!(body["split_text"], false);
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["exaggeration"], 1.25);
    assert_eq!(body["cfg_weight"], 0.5);
    assert_eq!(body["speed_factor"], 1.0);
    assert_eq!(body["seed"], 42);
    assert_eq!(body["language"], "en");
    assert_eq!(body["output_format"], "wav");
    assert_eq!(body["stream"], false);
}

#[tokio::test]
async fn text_longer_than_max_chars_lets_the_server_split() {
    let mut server = Server::start(Response::wav(support::wav())).await;
    let engine = engine(&server.url);
    let long = "word ".repeat(80);
    engine.synthesize(&request(long.trim())).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&server.request().await.body).unwrap();
    assert_eq!(body["split_text"], true);
    assert_eq!(body["chunk_size"], 240);
}

#[tokio::test]
async fn status_reports_loaded_model_type() {
    let server = Server::start(Response::json(
        r#"{"loaded":true,"type":"turbo","sample_rate":24000}"#,
    ))
    .await;
    assert_eq!(
        engine(&server.url).status().await,
        EngineStatus::Ready(Some("turbo".into()))
    );
}

#[tokio::test]
async fn status_reports_loading_when_model_is_not_loaded() {
    let server = Server::start(Response::json(r#"{"loaded":false,"type":null}"#)).await;
    assert_eq!(engine(&server.url).status().await, EngineStatus::Loading);
}

#[tokio::test]
async fn status_reports_server_errors() {
    let mut response = Response::json(r#"{"detail":"Failed to retrieve model information"}"#);
    response.status = 500;
    let server = Server::start(response).await;
    assert!(matches!(
        engine(&server.url).status().await,
        EngineStatus::Error(_)
    ));
}

#[tokio::test]
async fn status_is_offline_when_nothing_listens() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert_eq!(engine(&url).status().await, EngineStatus::Offline);
    assert!(matches!(
        engine(&url).synthesize(&request("Hello.")).await,
        Err(EngineError::Unavailable(_))
    ));
}

#[tokio::test]
async fn voices_are_mapped_in_server_order() {
    let server = Server::start(Response::json(
        r#"[{"display_name":"Emily","filename":"Emily.wav"},{"display_name":"Adrian","filename":"Adrian.wav"}]"#,
    ))
    .await;
    let voices = engine(&server.url).voices().await.unwrap();
    assert_eq!(voices[0].id, "Emily.wav");
    assert_eq!(voices[1].display_name, "Adrian");
}

#[tokio::test]
async fn http_errors_keep_status() {
    let mut response = Response::json(r#"{"detail":"Model not loaded"}"#);
    response.status = 503;
    let server = Server::start(response).await;
    let error = engine(&server.url)
        .synthesize(&request("Hello."))
        .await
        .unwrap_err();
    assert!(
        matches!(error, EngineError::Http { status: 503, .. }),
        "{error:?}"
    );
}

#[test]
fn factory_honours_profile_network_scope() {
    let mut profile = EngineProfile {
        id: "c".into(),
        name: "Chatterbox".into(),
        kind: "chatterbox".into(),
        base_url: "http://192.168.1.20:8004".into(),
        model: None,
        network: NetworkScope::Loopback,
        voice_id: None,
        params: Params::new(),
        launch: None,
    };
    let mut registry = aoide_tts::Registry::new();
    aoide_chatterbox::engine::register(&mut registry);
    assert!(registry.create(&profile).is_err());
    profile.network = NetworkScope::PrivateNetwork;
    assert!(registry.create(&profile).is_ok());
}
