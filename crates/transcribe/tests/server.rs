//! The OpenAI-style transcription client against a fake server on loopback.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use aoide_transcribe::{
    Error, NetworkScope, Request, ServerOptions, ServerTranscriber, Transcriber, SAMPLE_RATE,
};

/// Answers each request with `responses` in turn and keeps what it was sent.
fn server(responses: Vec<String>) -> (String, Arc<Mutex<Vec<Vec<u8>>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    std::thread::spawn(move || {
        for response in responses {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = Vec::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
                head.extend_from_slice(line.as_bytes());
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            head.extend(body);
            kept.lock().unwrap().push(head);
            let mut stream = stream;
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    (url, seen)
}

fn ok(json: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{json}",
        json.len()
    )
}

fn client(url: &str) -> ServerTranscriber {
    ServerTranscriber::new(ServerOptions {
        base_url: url.into(),
        model: Some("base.en".into()),
        ..Default::default()
    })
    .unwrap()
}

fn second() -> Vec<f32> {
    vec![0.25; SAMPLE_RATE as usize]
}

#[test]
fn sends_the_block_as_wav_and_reads_back_word_times() {
    let (url, seen) = server(vec![ok(
        r#"{"text":"Lanterns burn.","words":[{"word":" Lanterns","start":0.12,"end":0.58},{"word":"burn.","start":0.6,"end":0.9}]}"#,
    )]);
    let samples = second();
    let words = client(&url)
        .transcribe(&Request {
            samples: &samples,
            language: Some("en"),
            abort: None,
        })
        .unwrap();
    assert_eq!(
        words
            .iter()
            .map(|w| (w.text.as_str(), w.start_ms, w.end_ms))
            .collect::<Vec<_>>(),
        [("Lanterns", 120, 580), ("burn.", 600, 900)]
    );
    let sent = String::from_utf8_lossy(&seen.lock().unwrap()[0]).into_owned();
    assert!(
        sent.starts_with("POST /v1/audio/transcriptions "),
        "{sent:.80}"
    );
    for field in [
        "name=\"model\"\r\n\r\nbase.en",
        "name=\"language\"\r\n\r\nen",
        "name=\"response_format\"\r\n\r\nverbose_json",
        "name=\"timestamp_granularities[]\"\r\n\r\nword",
        "filename=\"block.wav\"",
        "RIFF",
        "WAVEfmt ",
    ] {
        assert!(sent.contains(field), "missing {field:?}");
    }
    // 16-bit samples: one second at 16 kHz plus the header.
    assert!(seen.lock().unwrap()[0].len() > 2 * SAMPLE_RATE as usize);
}

#[test]
fn reaches_only_this_computer_unless_the_home_network_is_allowed() {
    let at = |network| {
        ServerTranscriber::new(ServerOptions {
            base_url: "http://192.168.1.20:8000".into(),
            network,
            ..Default::default()
        })
    };
    assert!(at(NetworkScope::Loopback).is_err());
    assert!(at(NetworkScope::PrivateNetwork).is_ok());
    assert!(ServerTranscriber::new(ServerOptions {
        base_url: "https://api.example.com".into(),
        network: NetworkScope::PrivateNetwork,
        ..Default::default()
    })
    .is_err());
}

#[test]
fn does_not_follow_a_redirect() {
    let (url, seen) = server(vec![
        "HTTP/1.1 307 Temporary Redirect\r\nlocation: http://192.168.1.20/v1/audio/transcriptions\r\ncontent-length: 0\r\nconnection: close\r\n\r\n".into(),
    ]);
    let samples = second();
    let error = client(&url)
        .transcribe(&Request {
            samples: &samples,
            language: None,
            abort: None,
        })
        .unwrap_err();
    assert!(matches!(error, Error::Failed(_)), "{error:?}");
    assert_eq!(seen.lock().unwrap().len(), 1);
}

#[test]
fn says_what_the_server_answered_when_it_fails() {
    let body = r#"{"error":{"message":"model not found"}}"#;
    let (url, _) = server(vec![format!(
        "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    )]);
    let samples = second();
    let error = client(&url)
        .transcribe(&Request {
            samples: &samples,
            language: None,
            abort: None,
        })
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("404") && error.contains("model not found"),
        "{error}"
    );
}

#[test]
fn needs_word_times_from_the_server() {
    let (url, _) = server(vec![ok(r#"{"text":"Lanterns burn."}"#)]);
    let samples = second();
    let error = client(&url)
        .transcribe(&Request {
            samples: &samples,
            language: None,
            abort: None,
        })
        .unwrap_err()
        .to_string();
    assert!(error.contains("word times"), "{error}");
}

#[test]
fn a_stopped_request_is_not_sent() {
    let samples = second();
    let error = client("http://127.0.0.1:9")
        .transcribe(&Request {
            samples: &samples,
            language: None,
            abort: Some(Arc::new(AtomicBool::new(true))),
        })
        .unwrap_err();
    assert!(matches!(error, Error::Aborted));
}
