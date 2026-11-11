//! Aligner client behaviour over loopback HTTP with synthetic responses.
use aoide_aligner::{Aligner, Error, Options};
use aoide_tts::NetworkScope;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Serves one response per connection; returns (url, received requests as (head, body)).
async fn serve(
    status: u16,
    body: &'static str,
) -> (
    String,
    tokio::sync::mpsc::UnboundedReceiver<(String, Vec<u8>)>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut data = Vec::new();
            let mut buffer = vec![0; 65536];
            let head_end = loop {
                let n = stream.read(&mut buffer).await.unwrap();
                data.extend_from_slice(&buffer[..n]);
                if let Some(i) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let head = String::from_utf8_lossy(&data[..head_end]).to_string();
            let length: usize = head
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse().unwrap())
                })
                .unwrap_or(0);
            while data.len() < head_end + length {
                let n = stream.read(&mut buffer).await.unwrap();
                data.extend_from_slice(&buffer[..n]);
            }
            let _ = tx.send((head, data[head_end..head_end + length].to_vec()));
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes()).await;
        }
    });
    (url, rx)
}

fn aligner(url: &str) -> Aligner {
    Aligner::new(Options {
        base_url: url.into(),
        ..Default::default()
    })
    .unwrap()
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| w.to_string()).collect()
}

#[tokio::test]
async fn align_sends_words_and_pcm16_and_reads_spans() {
    let (url, mut requests) = serve(200, r#"{"words":[[100,400],null,[700,900]]}"#).await;
    let samples = [0.0f32, 0.5, -0.5, 1.5];
    let spans = aligner(&url)
        .align(&words(&["One,", "1999", "three."]), &samples, 24_000)
        .await
        .unwrap();
    assert_eq!(spans, vec![Some((100, 400)), None, Some((700, 900))]);
    let (head, body) = requests.recv().await.unwrap();
    assert!(head.starts_with("POST /align "), "{head}");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["words"], serde_json::json!(["One,", "1999", "three."]));
    assert_eq!(json["sampleRate"], 24_000);
    let pcm = base64_decode(json["audio"].as_str().unwrap());
    let values: Vec<i16> = pcm
        .chunks(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(values, vec![0, 16384, -16384, 32767]);
}

fn base64_decode(text: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .unwrap()
}

#[tokio::test]
async fn mismatched_or_inverted_spans_are_invalid() {
    for body in [
        r#"{"words":[[1,2]]}"#,
        r#"{"words":[[5,2],[6,7]]}"#,
        r#"{"spans":[]}"#,
    ] {
        let (url, _requests) = serve(200, body).await;
        let error = aligner(&url)
            .align(&words(&["a", "b"]), &[0.1; 10], 16_000)
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::InvalidResponse(_)),
            "{body}: {error:?}"
        );
    }
}

#[tokio::test]
async fn http_errors_carry_the_detail() {
    let (url, _requests) = serve(422, r#"{"detail":"audio too short"}"#).await;
    let error = aligner(&url)
        .align(&words(&["a"]), &[0.1; 10], 16_000)
        .await
        .unwrap_err();
    match error {
        Error::Http { status, message } => {
            assert_eq!(status, 422);
            assert_eq!(message, "audio too short");
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn health_reports_readiness_and_closed_ports_are_unavailable() {
    let (url, mut requests) = serve(200, r#"{"ready":true,"model":"MMS_FA"}"#).await;
    assert!(aligner(&url).ready().await.unwrap());
    assert!(requests.recv().await.unwrap().0.starts_with("GET /health "));
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let error = aligner(&format!("http://127.0.0.1:{port}"))
        .ready()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Unavailable(_)), "{error:?}");
}

#[tokio::test]
async fn empty_input_needs_no_request() {
    let aligner = aligner("http://127.0.0.1:9");
    assert_eq!(
        aligner.align(&[], &[0.1; 10], 16_000).await.unwrap(),
        vec![]
    );
    assert!(matches!(
        aligner.align(&words(&["a"]), &[], 16_000).await,
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn addresses_follow_the_network_scope() {
    let lan = |network| {
        Aligner::new(Options {
            base_url: "http://192.168.1.9:8890".into(),
            network,
            ..Default::default()
        })
    };
    assert!(matches!(
        lan(NetworkScope::Loopback),
        Err(Error::InvalidConfig(_))
    ));
    assert!(lan(NetworkScope::PrivateNetwork).is_ok());
}
