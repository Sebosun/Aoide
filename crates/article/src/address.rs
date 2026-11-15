//! One address per article: the same page reached through a share link, a feed or a pasted
//! URL is the same item.

use sha2::{Digest, Sha256};
use url::Url;

/// Query parameters that only say where a click came from.
const TRACKING: &[&str] = &[
    "fbclid",
    "gclid",
    "dclid",
    "gbraid",
    "wbraid",
    "msclkid",
    "yclid",
    "twclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_ga",
    "_gl",
    "mkt_tok",
    "oly_anon_id",
    "oly_enc_id",
    "vero_id",
    "vero_conv",
    "__s",
    "ref_src",
    "ref_url",
    "s_cid",
    "spm",
];

fn tracking(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_") || TRACKING.contains(&key.as_str())
}

/// `url` with a lower-case scheme and host, no fragment and no tracking parameters; the other
/// parameters keep their order. An empty path is `/`, so a trailing slash is dropped only there.
pub fn normalise(url: &Url) -> Url {
    // `Url` already lower-cases the scheme and a special scheme's host, and drops default ports.
    let mut url = url.clone();
    url.set_fragment(None);
    if let Some(query) = url.query() {
        // Kept parameters stay exactly as written, so their encoding does not change.
        let kept: Vec<&str> = query
            .split('&')
            .filter(|pair| {
                let key = pair.split('=').next().unwrap_or_default();
                !pair.is_empty() && !tracking(key)
            })
            .collect();
        let kept = kept.join("&");
        url.set_query((!kept.is_empty()).then_some(kept.as_str()));
    }
    url
}

/// The article's id: the SHA-256 of its normalised address, in lower-case hex.
pub fn canonical_id(url: &Url) -> String {
    Sha256::digest(normalise(url).as_str().as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
