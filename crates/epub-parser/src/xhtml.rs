//! EPUB content documents are XHTML, but are read with an HTML5 parser for its error recovery.
//! The two disagree on self-closing tags: in XML `<title/>` and `<a id="p1"/>` are empty
//! elements, while HTML ignores the slash and leaves them open, so a `<title/>` swallows the
//! whole body as title text. Expanding them first makes the HTML parser read what XML means.

use std::borrow::Cow;

/// HTML elements that never have content; the HTML parser already treats these as empty.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "keygen", "link", "meta", "param",
    "source", "track", "wbr",
];

/// Rewrites `<name .../>` as `<name ...></name>` for non-void elements, outside comments,
/// CDATA sections, declarations and processing instructions.
pub(crate) fn expand_self_closing(source: &str) -> Cow<'_, str> {
    if !source.contains("/>") {
        return Cow::Borrowed(source);
    }
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len() + 64);
    let mut copied = 0;
    let mut at = 0;
    while let Some(offset) = source[at..].find('<') {
        let start = at + offset;
        let rest = &source[start..];
        let skip_to = |end: &str| {
            rest.find(end)
                .map_or(source.len(), |i| start + i + end.len())
        };
        if rest.starts_with("<!--") {
            at = skip_to("-->");
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            at = skip_to("]]>");
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            at = skip_to(">");
            continue;
        }
        if !bytes.get(start + 1).is_some_and(u8::is_ascii_alphabetic) {
            at = start + 1;
            continue;
        }
        let Some(end) = tag_end(bytes, start + 1) else {
            break; // Unterminated tag: leave the rest to the HTML parser.
        };
        let inner = source[start + 1..end].trim_end();
        if let Some(open) = inner.strip_suffix('/') {
            let name_len = open
                .find(|c: char| c.is_ascii_whitespace() || c == '/')
                .unwrap_or(open.len());
            let name = &open[..name_len];
            if !VOID.contains(&name.to_ascii_lowercase().as_str()) {
                out.push_str(&source[copied..start]);
                out.push('<');
                out.push_str(open.trim_end());
                out.push_str("></");
                out.push_str(name);
                out.push('>');
                copied = end + 1;
            }
        }
        at = end + 1;
    }
    if copied == 0 {
        return Cow::Borrowed(source);
    }
    out.push_str(&source[copied..]);
    Cow::Owned(out)
}

/// Index of the `>` closing a start tag that begins at `from`, skipping quoted attribute values.
fn tag_end(bytes: &[u8], from: usize) -> Option<usize> {
    let mut quote = None;
    for (i, &b) in bytes.iter().enumerate().skip(from) {
        match (quote, b) {
            (Some(q), _) if b == q => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(b),
            (None, b'>') => return Some(i),
            (None, b'<') => return None, // A stray `<`: not a tag after all.
            (None, _) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::expand_self_closing;

    #[test]
    fn expands_only_non_void_elements_outside_comments_and_quotes() {
        let source = r#"<head><title/><meta charset="utf-8"/></head><a id="x" title="a>b"/><br/><!-- <p/> --><svg:rect/><p>1 < 2</p>"#;
        assert_eq!(
            expand_self_closing(source),
            r#"<head><title></title><meta charset="utf-8"/></head><a id="x" title="a>b"></a><br/><!-- <p/> --><svg:rect></svg:rect><p>1 < 2</p>"#
        );
        assert!(matches!(
            expand_self_closing("<p>plain</p>"),
            std::borrow::Cow::Borrowed(_)
        ));
    }
}
