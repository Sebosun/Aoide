mod support;

#[path = "cases/audio.rs"]
mod audio_cases;
#[path = "cases/http.rs"]
mod http_cases;

use aoide_chatterbox::{
    AudioMetadata, ChatterboxClient, ClientOptions, Error, SampleEncoding, SpeechRequest, Voice,
};
use support::{Response, Server};

#[tokio::test]
async fn sends_explicit_seed_language_and_chunk_size_and_accepts_complete_chunked_wav() {
    let mut response = Response::wav(support::wav());
    response.chunked = true;
    response.content_type = "Audio/X-Wav; charset=binary".into();
    let mut server = Server::start(response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.replace("127.0.0.1", "localhost"),
        max_audio_bytes: 172,
        ..Default::default()
    })
    .unwrap();
    let mut speech = SpeechRequest::new("A sentence.", "Narrator_żółć.MP3");
    speech.settings.seed = Some(0);
    speech.settings.language = Some("pl".into());
    speech.settings.chunk_size = 500;
    assert_eq!(
        client.generate(&speech).await.unwrap().metadata().frames,
        64
    );
    let request = server.request().await;
    let json = serde_json::from_slice::<serde_json::Value>(&request.body).unwrap();
    assert_eq!(json["seed"], 0);
    assert_eq!(json["language"], "pl");
    assert_eq!(json["chunk_size"], 500);
    assert_eq!(json["predefined_voice_id"], "Narrator_żółć.MP3");
}

#[test]
fn rejects_zero_deadlines_and_response_or_input_budgets() {
    use std::time::Duration;
    for options in [
        ClientOptions {
            connect_timeout: Duration::ZERO,
            ..Default::default()
        },
        ClientOptions {
            discovery_timeout: Duration::ZERO,
            ..Default::default()
        },
        ClientOptions {
            generation_timeout: Duration::ZERO,
            ..Default::default()
        },
        ClientOptions {
            max_audio_bytes: 0,
            ..Default::default()
        },
        ClientOptions {
            max_metadata_bytes: 0,
            ..Default::default()
        },
        ClientOptions {
            max_error_bytes: 0,
            ..Default::default()
        },
        ClientOptions {
            max_text_bytes: 0,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            ChatterboxClient::new(options),
            Err(Error::InvalidOptions(_))
        ));
    }
}

#[tokio::test]
async fn rejects_malformed_or_unsafe_voice_lists_but_accepts_an_empty_library() {
    for body in [
        "not JSON",
        r#"{"voices":[]}"#,
        r#"[{"filename":"Emily.wav"}]"#,
        r#"[{"filename":"../Emily.wav","display_name":"Emily"}]"#,
        r#"[{"filename":"Emily.wav","display_name":" "}]"#,
        r#"[{"filename":"Emily.wav","display_name":"A"},{"filename":"Emily.wav","display_name":"B"}]"#,
    ] {
        let server = Server::start(Response::json(body)).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            ..Default::default()
        })
        .unwrap();
        assert!(
            matches!(client.voices().await, Err(Error::InvalidResponse(_))),
            "accepted {body}"
        );
    }
    let server = Server::start(Response::json("[]")).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(client.voices().await.unwrap().is_empty());
}

#[tokio::test]
async fn times_out_stalled_discovery_and_generation_including_response_bodies() {
    for stall in [support::Stall::BeforeHeaders, support::Stall::AfterHeaders] {
        let mut response = Response::wav(support::wav());
        response.stall = Some(stall);
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            generation_timeout: std::time::Duration::from_millis(200),
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            client
                .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
                .await,
            Err(Error::Timeout)
        ));
        assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 1);

        let mut response = Response::json("[]");
        response.stall = Some(stall);
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            discovery_timeout: std::time::Duration::from_millis(200),
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(client.voices().await, Err(Error::Timeout)));
    }
}

#[tokio::test]
async fn rejects_empty_truncated_and_structurally_inconsistent_wav_audio() {
    let mut empty = support::wav();
    empty.truncate(44);
    empty[4..8].copy_from_slice(&36u32.to_le_bytes());
    empty[40..44].copy_from_slice(&0u32.to_le_bytes());
    let mut truncated = support::wav();
    truncated.truncate(163);
    let mut wrong_riff_size = support::wav();
    wrong_riff_size[4..8].copy_from_slice(&1u32.to_le_bytes());
    let mut wrong_data_size = support::wav();
    wrong_data_size[40..44].copy_from_slice(&130u32.to_le_bytes());
    let mut zero_rate = support::wav();
    zero_rate[24..28].copy_from_slice(&0u32.to_le_bytes());
    for bytes in [
        b"not a WAV".to_vec(),
        empty,
        truncated,
        wrong_riff_size,
        wrong_data_size,
        zero_rate,
    ] {
        let server = Server::start(Response::wav(bytes)).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            client
                .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
                .await,
            Err(Error::InvalidAudio(_))
        ));
    }
}

#[tokio::test]
async fn bounds_audio_and_voice_responses_with_or_without_content_length() {
    for chunked in [false, true] {
        let mut response = Response::wav(support::wav());
        response.chunked = chunked;
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            max_audio_bytes: 100,
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            client
                .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
                .await,
            Err(Error::ResponseTooLarge {
                kind: "audio",
                limit: 100
            })
        ));

        let mut response = Response::json(r#"[{"filename":"Emily.wav","display_name":"Emily"}]"#);
        response.chunked = chunked;
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            max_metadata_bytes: 16,
            ..Default::default()
        })
        .unwrap();
        assert!(matches!(
            client.voices().await,
            Err(Error::ResponseTooLarge {
                kind: "voices",
                limit: 16
            })
        ));
    }
}

#[tokio::test]
async fn rejects_success_responses_with_the_wrong_content_type() {
    let mut audio_response = Response::wav(support::wav());
    audio_response.content_type = "application/json".into();
    let server = Server::start(audio_response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        client
            .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
            .await,
        Err(Error::InvalidResponse(_))
    ));

    let mut voice_response = Response::json("[]");
    voice_response.content_type = "text/html".into();
    let server = Server::start(voice_response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        client.voices().await,
        Err(Error::InvalidResponse(_))
    ));
}

#[tokio::test]
async fn reports_an_unloaded_model_without_retrying_a_generation_request() {
    let mut response =
        Response::json(r#"{"detail":"TTS engine model is not currently loaded or available."}"#);
    response.status = 503;
    let server = Server::start(response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(
        matches!(client.generate(&SpeechRequest::new("A short chapter.", "Emily.wav")).await,
        Err(Error::Http { status: 503, message }) if message == "TTS engine model is not currently loaded or available.")
    );
    assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn rejects_invalid_speech_requests_without_contacting_the_server() {
    let server = Server::start(Response::wav(support::wav())).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        max_text_bytes: 24,
        ..Default::default()
    })
    .unwrap();
    let mut requests = vec![
        SpeechRequest::new("", "Emily.wav"),
        SpeechRequest::new(" \n\t", "Emily.wav"),
        SpeechRequest::new("x".repeat(25), "Emily.wav"),
        SpeechRequest::new("海".repeat(9), "Emily.wav"),
    ];
    for voice in [
        "",
        " ",
        "../Emily.wav",
        "voices/Emily.wav",
        "C:\\Emily.wav",
        "Emily\n.wav",
        "Emily\0.wav",
        "Emily.txt",
    ] {
        requests.push(SpeechRequest::new("A sentence.", voice));
    }
    for size in [0, 49, 501] {
        let mut request = SpeechRequest::new("A sentence.", "Emily.wav");
        request.settings.chunk_size = size;
        requests.push(request);
    }
    let mut request = SpeechRequest::new("A sentence.", "Emily.wav");
    request.settings.language = Some(" ".into());
    requests.push(request);
    for request in requests {
        assert!(matches!(
            client.generate(&request).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn rejects_nonlocal_or_ambiguous_endpoints_before_any_book_text_can_be_sent() {
    for base_url in [
        "https://example.com",
        "http://192.168.1.10:8004",
        "http://0.0.0.0:8004",
        "http://localhost.example.com",
        "file:///tmp/tts",
        "http://user:secret@127.0.0.1:8004",
        "http://127.0.0.1:8004/proxy",
        "http://127.0.0.1:8004?key=secret",
        "http://127.0.0.1:8004#fragment",
        "not a URL",
    ] {
        assert!(
            matches!(
                ChatterboxClient::new(ClientOptions {
                    base_url: base_url.into(),
                    ..Default::default()
                }),
                Err(Error::InvalidEndpoint(_))
            ),
            "accepted {base_url}"
        );
    }
    for base_url in [
        "http://localhost:8004/",
        "http://127.0.0.1:8004",
        "http://[::1]:8004",
    ] {
        assert!(ChatterboxClient::new(ClientOptions {
            base_url: base_url.into(),
            ..Default::default()
        })
        .is_ok());
    }
}

#[tokio::test]
async fn sends_chapter_text_verbatim_and_returns_wav_bytes_with_metadata() {
    let wav = support::wav();
    let mut server = Server::start(Response::wav(wav.clone())).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();

    let audio = client
        .generate(&SpeechRequest::new(
            "Chapter 1\n\nCafé & 海. No rewriting.",
            "Emily.wav",
        ))
        .await
        .unwrap();

    assert_eq!(audio.as_bytes(), wav);
    assert_eq!(
        audio.metadata(),
        &AudioMetadata {
            sample_rate: 24000,
            channels: 1,
            bits_per_sample: 16,
            frames: 64,
            encoding: SampleEncoding::Pcm
        }
    );
    let request = server.request().await;
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/tts");
    assert_eq!(
        request.headers.get("content-type").map(String::as_str),
        Some("application/json")
    );
    assert_eq!(
        request.headers.get("accept").map(String::as_str),
        Some("audio/wav")
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
        serde_json::json!({
            "text": "Chapter 1\n\nCafé & 海. No rewriting.", "voice_mode": "predefined", "predefined_voice_id": "Emily.wav",
            "output_format": "wav", "split_text": true, "chunk_size": 240, "stream": false,
        })
    );
}

#[tokio::test]
async fn lists_predefined_voice_filenames_and_display_names_using_the_server_contract() {
    let mut server = Server::start(Response::json(r#"[{"display_name":"Robert","filename":"Robert.wav"},{"display_name":"Emily","filename":"Emily.wav"}]"#)).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();

    assert_eq!(
        client.voices().await.unwrap(),
        [
            Voice {
                id: "Robert.wav".into(),
                display_name: "Robert".into()
            },
            Voice {
                id: "Emily.wav".into(),
                display_name: "Emily".into()
            },
        ]
    );
    let request = server.request().await;
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/get_predefined_voices");
    assert_eq!(
        request.headers.get("accept").map(String::as_str),
        Some("application/json")
    );
    assert!(request.body.is_empty());
    assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 1);
}
