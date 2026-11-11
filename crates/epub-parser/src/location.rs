use crate::ParseError;
use percent_encoding::percent_decode_str;

pub(crate) struct Target {
    pub path: String,
    pub fragment: Option<String>,
}

pub(crate) fn resolve(base_file: &str, href: &str) -> Result<Target, ParseError> {
    let (path, fragment) = href
        .split_once('#')
        .map_or((href, None), |(p, f)| (p, Some(f)));
    let path = decode(path)?;
    if path.starts_with('/') || path.contains(['\\', '\0', ':', '?']) {
        return Err(ParseError::UnsafePath(href.into()));
    }
    let base = if path.is_empty() {
        base_file
    } else {
        base_file.rsplit_once('/').map_or("", |(p, _)| p)
    };
    let mut segments = base
        .split('/')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments
                    .pop()
                    .ok_or_else(|| ParseError::UnsafePath(href.into()))?;
            }
            part => segments.push(part),
        }
    }
    Ok(Target {
        path: segments.join("/"),
        fragment: fragment.filter(|f| !f.is_empty()).map(decode).transpose()?,
    })
}

fn decode(value: &str) -> Result<String, ParseError> {
    percent_decode_str(value)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|e| ParseError::Invalid(e.to_string()))
}
