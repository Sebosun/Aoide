//! Adapter behaviour over loopback HTTP with synthetic responses.
mod support;

use aoide_openai_speech::{descriptor, register, OpenAiSpeechEngine, Options};
use aoide_tts::testing::{assert_valid_descriptor, assert_valid_synthesis};
use aoide_tts::{
    resolve_params, EngineError, EngineProfile, EngineStatus, NetworkScope, ParamValue, Params,
    Registry, SpeechEngine, SynthesisRequest, Voice,
};
use support::{Response, Server};

fn engine(url: &str, model: Option<&str>) -> OpenAiSpeechEngine {
    OpenAiSpeechEngine::new(Options {
        base_url: url.into(),
        model: model.map(Into::into),
        ..Default::default()
    })
    .unwrap()
}

fn request(text: &str, language: Option<&str>) -> SynthesisRequest {
    let mut overrides = Params::new();
    overrides.insert("speed".into(), ParamValue::Float(1.5));
    SynthesisRequest {
        text: text.into(),
        voice_id: "af_bella".into(),
        language: language.map(Into::into),
        params: resolve_params(&descriptor(), &overrides).unwrap(),
    }
}

fn json(body: &[u8]) -> serde_json::Value {
    serde_json::from_slice(body).unwrap()
}

fn error_response(status: u16, content_type: &str, body: &str) -> Response {
    let mut response = Response::json(body);
    response.status = status;
    response.content_type = content_type.into();
    response
}

fn profile(url: &str, network: NetworkScope) -> EngineProfile {
    EngineProfile {
        id: "p".into(),
        name: "Kokoro".into(),
        kind: aoide_openai_speech::KIND.into(),
        base_url: url.into(),
        model: Some("kokoro".into()),
        network,
        voice_id: None,
        params: Params::new(),
        launch: None,
    }
}

#[test]
fn descriptor_passes_conformance() {
    let d = descriptor();
    assert_valid_descriptor(&d);
    assert_eq!(d.kind, "openai-speech");
    assert!(d.languages.is_none());
    assert_eq!(d.params.len(), 1);
    assert_eq!(d.params[0].key, "speed");
    assert_eq!(d.params[0].default, ParamValue::Float(1.0));
}

#[tokio::test]
async fn synthesis_posts_openai_body_and_decodes_wav() {
    let mut server = Server::start(Response::wav(support::wav())).await;
    let req = request("An original sentence.", None);
    let synthesis = engine(&server.url, None).synthesize(&req).await.unwrap();
    assert_valid_synthesis(&req, &synthesis);
    assert_eq!(synthesis.sample_rate, 24_000);
    let sent = server.request().await;
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path, "/v1/audio/speech");
    let body = json(&sent.body);
    assert_eq!(body["model"], "tts-1");
    assert_eq!(body["input"], "An original sentence.");
    assert_eq!(body["voice"], "af_bella");
    assert_eq!(body["response_format"], "wav");
    assert_eq!(body["speed"], 1.5);
    assert!(body.get("language").is_none());
}

#[tokio::test]
async fn synthesis_sends_profile_model_and_language() {
    let mut server = Server::start(Response::wav(support::wav())).await;
    let req = request("Another original sentence.", Some("fr"));
    engine(&server.url, Some("kokoro"))
        .synthesize(&req)
        .await
        .unwrap();
    let body = json(&server.request().await.body);
    assert_eq!(body["model"], "kokoro");
    assert_eq!(body["language"], "fr");
}

#[tokio::test]
async fn invalid_input_is_rejected_without_a_request() {
    let server = Server::start(Response::wav(support::wav())).await;
    let engine = OpenAiSpeechEngine::new(Options {
        base_url: server.url.clone(),
        max_text_bytes: 16,
        ..Default::default()
    })
    .unwrap();
    for (text, voice) in [
        ("   ", "af_bella"),
        ("Plenty of words beyond sixteen bytes.", "af_bella"),
        ("Short.", " "),
    ] {
        let mut req = request(text, None);
        req.voice_id = voice.into();
        let error = engine.synthesize(&req).await.unwrap_err();
        assert!(matches!(error, EngineError::InvalidInput(_)), "{error:?}");
    }
    assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn oversized_audio_is_rejected() {
    let server = Server::start(Response::wav(support::wav())).await;
    let engine = OpenAiSpeechEngine::new(Options {
        base_url: server.url.clone(),
        max_audio_bytes: 100,
        ..Default::default()
    })
    .unwrap();
    let error = engine
        .synthesize(&request("Original words.", None))
        .await
        .unwrap_err();
    assert!(
        matches!(error, EngineError::InvalidResponse(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn non_audio_success_is_rejected() {
    let server = Server::start(Response::json("{}")).await;
    let error = engine(&server.url, None)
        .synthesize(&request("Original words.", None))
        .await
        .unwrap_err();
    assert!(
        matches!(error, EngineError::InvalidResponse(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn http_errors_carry_detail_but_never_html() {
    for (response, expected) in [
        (
            error_response(400, "application/json", r#"{"detail":"unknown voice"}"#),
            "unknown voice",
        ),
        (
            error_response(
                500,
                "application/json",
                r#"{"error":{"message":"model crashed"}}"#,
            ),
            "model crashed",
        ),
        (
            error_response(503, "text/plain", "warming up"),
            "warming up",
        ),
    ] {
        let server = Server::start(response).await;
        let error = engine(&server.url, None)
            .synthesize(&request("Original words.", None))
            .await
            .unwrap_err();
        match error {
            EngineError::Http { message, .. } => assert_eq!(message, expected),
            other => panic!("{other:?}"),
        }
    }
    let server = Server::start(error_response(
        502,
        "text/html",
        "<html><body>bad gateway</body></html>",
    ))
    .await;
    let error = engine(&server.url, None)
        .synthesize(&request("Original words.", None))
        .await
        .unwrap_err();
    match error {
        EngineError::Http { status, message } => {
            assert_eq!(status, 502);
            assert!(!message.contains('<'), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn voices_accept_every_common_shape() {
    let voice = |id: &str, name: &str| Voice {
        id: id.into(),
        display_name: name.into(),
    };
    for (body, expected) in [
        (
            r#"{"voices":["af_bella","am_adam"]}"#,
            vec![voice("af_bella", "af_bella"), voice("am_adam", "am_adam")],
        ),
        (
            r#"{"voices":[{"id":"a","name":"Alpha"},{"voice_id":"b","display_name":"Beta"},{"id":"c"}]}"#,
            vec![voice("a", "Alpha"), voice("b", "Beta"), voice("c", "c")],
        ),
        (r#"["x"]"#, vec![voice("x", "x")]),
        (r#"[{"id":"y","name":"Why"}]"#, vec![voice("y", "Why")]),
        (r#"{"voices":[]}"#, vec![]),
    ] {
        let mut server = Server::start(Response::json(body)).await;
        let voices = engine(&server.url, None).voices().await.unwrap();
        assert_eq!(voices, expected, "{body}");
        let sent = server.request().await;
        assert_eq!(
            (sent.method.as_str(), sent.path.as_str()),
            ("GET", "/v1/audio/voices")
        );
    }
}

#[tokio::test]
async fn unrecognised_voice_json_is_an_invalid_response() {
    let server = Server::start(Response::json(r#"{"speakers":1}"#)).await;
    let error = engine(&server.url, None).voices().await.unwrap_err();
    assert!(
        matches!(error, EngineError::InvalidResponse(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn status_reflects_voice_endpoint() {
    let server = Server::start(Response::json(r#"{"voices":["a"]}"#)).await;
    assert_eq!(
        engine(&server.url, Some("kokoro")).status().await,
        EngineStatus::Ready(Some("kokoro".into()))
    );
    let server = Server::start(error_response(500, "text/plain", "broken")).await;
    assert!(matches!(
        engine(&server.url, None).status().await,
        EngineStatus::Error(_)
    ));
    // Bind then drop a listener to get a closed loopback port.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert_eq!(
        engine(&format!("http://127.0.0.1:{port}"), None)
            .status()
            .await,
        EngineStatus::Offline
    );
}

#[test]
fn registry_factory_honours_network_scope() {
    let mut registry = Registry::new();
    register(&mut registry);
    assert!(registry.descriptor("openai-speech").is_some());
    assert!(registry
        .create(&profile("http://192.168.1.20:8880", NetworkScope::Loopback))
        .is_err());
    assert!(registry
        .create(&profile(
            "http://192.168.1.20:8880",
            NetworkScope::PrivateNetwork
        ))
        .is_ok());
    assert!(registry
        .create(&profile("http://127.0.0.1:8880", NetworkScope::Loopback))
        .is_ok());
}
