//! URL patterns a site's rule blocks: `*` stands for any run of characters, everything else
//! is matched as written against the whole address.

/// Whether `url` matches `pattern`.
pub fn matches(pattern: &str, url: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return url == pattern;
    }
    if !url.starts_with(first) || url.len() < first.len() + last.len() || !url.ends_with(last) {
        return false;
    }
    let mut rest = &url[first.len()..url.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}
