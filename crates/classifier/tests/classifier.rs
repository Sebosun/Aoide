//! Tags from a local model: the prompt and the reading of its answer are pure; the client talks
//! to a scripted server on loopback.
use aoide_classifier::{parse_tags, prompt, Classifier, Item, Options};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn item() -> Item {
    Item {
        title: "The Lantern Keeper".into(),
        authors: vec!["Mira Vale".into()],
        description: Some("A keeper of a harbour light finds a key in a paper boat.".into()),
        excerpt: None,
    }
}

#[test]
fn the_prompt_names_the_item_and_the_tags_in_use() {
    let messages = prompt(&item(), &["Sea Stories".into(), "History".into()]);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "system");
    let asked = &messages[1].content;
    assert!(asked.contains("The Lantern Keeper"), "{asked}");
    assert!(asked.contains("Mira Vale"), "{asked}");
    assert!(asked.contains("paper boat"), "{asked}");
    assert!(asked.contains("Sea Stories, History"), "{asked}");
}

#[test]
fn an_answer_is_read_as_a_few_tags_in_the_casing_already_used() {
    let known = vec!["Sea Stories".to_string()];
    assert_eq!(
        parse_tags(r#"Sure! ["sea stories", "Lighthouses", "Fantasy"]"#, &known),
        ["Sea Stories", "Lighthouses", "Fantasy"]
    );
    // A list, numbered or bulleted, and repeats.
    assert_eq!(
        parse_tags(
            "1. Lighthouses\n2. \"Sea stories\"\n- lighthouses\n* Mystery.",
            &known
        ),
        ["Lighthouses", "Sea Stories", "Mystery"]
    );
    assert_eq!(
        parse_tags(
            "Fantasy, Adventure, Coming of age, Sea, Boats, Keys, Light",
            &[]
        ),
        ["Fantasy", "Adventure", "Coming of age", "Sea", "Boats"],
        "five at most"
    );
    assert!(parse_tags("", &known).is_empty());
}

#[test]
fn only_local_addresses_are_used() {
    let options = |base: &str| Options {
        base_url: base.into(),
        model: "small".into(),
        ..Options::default()
    };
    assert!(Classifier::new(options("http://localhost:11434")).is_ok());
    assert!(Classifier::new(options("http://192.168.1.4:11434")).is_err());
    assert!(Classifier::new(options("https://api.example.com")).is_err());
}

/// Answers one request with `body`, handing back what was asked.
async fn serve_once(body: String) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = socket.read(&mut buffer).await.unwrap();
            request.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&request);
            if let Some(end) = text.find("\r\n\r\n") {
                let length: usize = text
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if request.len() >= end + 4 + length {
                    break;
                }
            }
            if n == 0 {
                break;
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    (address, handle)
}

#[tokio::test]
async fn the_model_is_asked_and_its_tags_come_back() {
    let reply = serde_json::json!({
        "choices": [{ "message": { "role": "assistant", "content": "[\"Sea Stories\", \"Lighthouses\"]" } }]
    })
    .to_string();
    let (address, server) = serve_once(reply).await;
    let classifier = Classifier::new(Options {
        base_url: address,
        model: "small-model".into(),
        ..Options::default()
    })
    .unwrap();
    let tags = classifier
        .suggest(&item(), &["Sea Stories".into()])
        .await
        .unwrap();
    assert_eq!(tags, ["Sea Stories", "Lighthouses"]);
    let asked = server.await.unwrap();
    assert!(asked.starts_with("POST /v1/chat/completions "), "{asked}");
    assert!(asked.contains("\"model\":\"small-model\""), "{asked}");
}

#[tokio::test]
async fn a_server_that_is_not_there_is_an_error_that_says_so() {
    let classifier = Classifier::new(Options {
        base_url: "http://127.0.0.1:9".into(),
        model: "m".into(),
        ..Options::default()
    })
    .unwrap();
    let error = classifier.suggest(&item(), &[]).await.unwrap_err();
    assert!(error.to_string().contains("unavailable"), "{error}");
}
