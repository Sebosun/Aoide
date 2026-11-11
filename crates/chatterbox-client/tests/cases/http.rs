use super::*;

#[tokio::test]
async fn rejects_accepted_or_partial_responses_instead_of_returning_them_as_complete_chapters() {
    for status in [202, 206] {
        let mut response = Response::wav(support::wav());
        response.status = status;
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            ..Default::default()
        })
        .unwrap();
        assert!(
            matches!(client.generate(&SpeechRequest::new("A sentence.", "Emily.wav")).await, Err(Error::Http { status: actual, .. }) if actual == status)
        );
    }
}

#[tokio::test]
async fn does_not_follow_redirects_even_to_another_loopback_server() {
    let destination = Server::start(Response::wav(support::wav())).await;
    let mut response = Response::json("");
    response.status = 302;
    response
        .extra_headers
        .push(("Location".into(), format!("{}/tts", destination.url)));
    let source = Server::start(response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: source.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        client
            .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
            .await,
        Err(Error::Http { status: 302, .. })
    ));
    assert_eq!(source.count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(
        destination.count.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[tokio::test]
async fn reports_auth_validation_rate_limit_and_server_errors_without_retries() {
    for status in [400, 401, 403, 404, 422, 429, 500] {
        let mut response = Response::json(r#"{"detail":"Request could not be completed."}"#);
        response.status = status;
        response
            .extra_headers
            .push(("Retry-After".into(), "60".into()));
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            ..Default::default()
        })
        .unwrap();
        assert!(
            matches!(client.voices().await, Err(Error::Http { status: actual, message }) if actual == status && message == "Request could not be completed.")
        );
        assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn bounds_error_details_and_does_not_echo_pydantic_input_fields_or_html() {
    let mut response = Response::json(
        r#"{"detail":[{"msg":"Invalid parameter","input":"PRIVATE CHAPTER TEXT"}]}"#,
    );
    response.status = 422;
    let server = Server::start(response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(
        matches!(client.voices().await, Err(Error::Http { status: 422, message }) if message == "Invalid parameter")
    );
    for body in [
        "<html>PRIVATE CHAPTER TEXT</html>".into(),
        format!(r#"{{"detail":"{}"}}"#, "x".repeat(100)),
    ] {
        let mut response = Response::json(&body);
        response.status = 500;
        response.chunked = true;
        let server = Server::start(response).await;
        let client = ChatterboxClient::new(ClientOptions {
            base_url: server.url.clone(),
            max_error_bytes: 16,
            ..Default::default()
        })
        .unwrap();
        assert!(
            matches!(client.voices().await, Err(Error::Http { status: 500, message }) if message == "Internal Server Error")
        );
    }
}

#[tokio::test]
async fn reports_connections_closed_mid_body_as_transport_errors_without_retries() {
    let mut response = Response::wav(support::wav());
    response.declared_length = Some(1024);
    let server = Server::start(response).await;
    let client = ChatterboxClient::new(ClientOptions {
        base_url: server.url.clone(),
        ..Default::default()
    })
    .unwrap();
    assert!(matches!(
        client
            .generate(&SpeechRequest::new("A sentence.", "Emily.wav"))
            .await,
        Err(Error::Transport(_))
    ));
    assert_eq!(server.count.load(std::sync::atomic::Ordering::SeqCst), 1);
}
