use crate::Error;
use reqwest::Url;

pub(crate) fn parse(value: &str, scope: aoide_tts::NetworkScope) -> Result<Url, Error> {
    let url = aoide_tts::validate_endpoint(value, scope).map_err(|_| {
        Error::InvalidEndpoint(match scope {
            aoide_tts::NetworkScope::Loopback => {
                "use a loopback HTTP(S) origin (localhost or a loopback IP) without credentials, path, query, or fragment"
            }
            aoide_tts::NetworkScope::PrivateNetwork => {
                "use a loopback or private-network HTTP(S) IP origin without credentials, path, query, or fragment"
            }
        })
    })?;
    Url::parse(url.as_str()).map_err(|_| Error::InvalidEndpoint("expected an HTTP(S) URL"))
}
