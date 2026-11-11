use crate::Error;
use reqwest::Response;

pub(crate) async fn limited(
    mut response: Response,
    limit: usize,
    kind: &'static str,
) -> Result<Vec<u8>, Error> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(Error::ResponseTooLarge { kind, limit });
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit - bytes.len() {
            return Err(Error::ResponseTooLarge { kind, limit });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(crate) fn expect_type(response: &Response, allowed: &[&str]) -> Result<(), Error> {
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(';').next())
        .map(str::trim);
    if !media_type.is_some_and(|media_type| {
        allowed
            .iter()
            .any(|allowed| media_type.eq_ignore_ascii_case(allowed))
    }) {
        return Err(Error::InvalidResponse(format!(
            "expected response content type {}",
            allowed.join(" or ")
        )));
    }
    Ok(())
}

pub(crate) async fn checked(
    mut response: Response,
    max_error_bytes: usize,
) -> Result<Response, Error> {
    let status = response.status();
    if status == reqwest::StatusCode::OK {
        return Ok(response);
    }
    let mut bytes = Vec::new();
    while bytes.len() < max_error_bytes {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                bytes.extend_from_slice(&chunk[..chunk.len().min(max_error_bytes - bytes.len())])
            }
            _ => break,
        }
    }
    let detail = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .and_then(|value| {
            let detail = value.get("detail")?;
            if let Some(message) = detail.as_str() {
                return Some(message.to_owned());
            }
            // Pydantic validation details can echo the input text. Keep only messages.
            let messages = detail
                .as_array()?
                .iter()
                .filter_map(|item| item.get("msg")?.as_str())
                .collect::<Vec<_>>();
            (!messages.is_empty()).then(|| messages.join("; "))
        });
    Err(Error::Http {
        status: status.as_u16(),
        message: detail.unwrap_or_else(|| {
            status
                .canonical_reason()
                .unwrap_or("HTTP request failed")
                .into()
        }),
    })
}
