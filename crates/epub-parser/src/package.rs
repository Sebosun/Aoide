use crate::navigation::{Entry, Navigation};
use crate::{archive::Archive, location::resolve, ParseError};
use roxmltree::Document;

pub(crate) struct Package {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub identifiers: Vec<String>,
    pub cover: Option<crate::Cover>,
    pub spine: Vec<String>,
    pub navigation: Option<Navigation>,
    pub guide: Vec<Entry>,
}

pub(crate) fn read(archive: &mut Archive<'_>) -> Result<Package, ParseError> {
    let container = archive.read("META-INF/container.xml")?;
    let container = xml(&container)?;
    let path = container
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or_else(|| ParseError::Invalid("missing package location".into()))?;
    let path = resolve("", path)?.path;
    let source = archive.read(&path)?;
    let doc = xml(&source)?;
    let spine: Vec<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .map(|n| {
            let item = doc
                .descendants()
                .find(|item| {
                    item.has_tag_name("item") && item.attribute("id") == n.attribute("idref")
                })
                .ok_or_else(|| ParseError::Invalid("spine references an unknown item".into()))?;
            let href = item
                .attribute("href")
                .ok_or_else(|| ParseError::Invalid("item has no href".into()))?;
            Ok(resolve(&path, href)?.path)
        })
        .collect::<Result<_, _>>()?;
    let all = |name| -> Vec<String> {
        doc.descendants()
            .filter(|n| n.has_tag_name(name))
            .filter_map(|n| n.text())
            .map(collapse)
            .filter(|t| !t.is_empty())
            .collect()
    };
    let metadata = |name| all(name).into_iter().next();
    let cover = cover_path(&doc, &path)?
        .and_then(|(href, media_type)| {
            archive.contains(&href).then(|| {
                archive
                    .read_bytes(&href)
                    .map(|bytes| crate::Cover { media_type, bytes })
            })
        })
        .transpose()?;
    let navigation = doc
        .descendants()
        .find(|n| {
            n.has_tag_name("item")
                && n.attribute("properties")
                    .is_some_and(|p| p.split_whitespace().any(|v| v == "nav"))
        })
        .or_else(|| {
            doc.descendants().find(|n| {
                n.has_tag_name("item")
                    && n.attribute("media-type") == Some("application/x-dtbncx+xml")
            })
        })
        .map(|n| {
            let href = n
                .attribute("href")
                .ok_or_else(|| ParseError::Invalid("navigation has no href".into()))?;
            Ok(Navigation {
                path: resolve(&path, href)?.path,
                is_ncx: n.attribute("media-type") == Some("application/x-dtbncx+xml"),
            })
        })
        .transpose()?;
    check_encryption(
        archive,
        &spine,
        navigation.as_ref().map(|n| n.path.as_str()),
    )?;
    let mut guide = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("reference")) {
        let token = node.attribute("type").unwrap_or("");
        let kind = crate::ExclusionReason::from_token(token);
        if kind.is_none() && token != "text" {
            continue;
        }
        let href = node
            .attribute("href")
            .ok_or_else(|| ParseError::Invalid("guide reference has no href".into()))?;
        let target = resolve(&path, href)?;
        guide.push(Entry {
            path: target.path,
            fragment: target.fragment,
            title: node.attribute("title").unwrap_or(token).into(),
            depth: 0,
            kind,
            is_landmark: true,
        });
    }
    Ok(Package {
        title: metadata("title"),
        authors: all("creator"),
        language: metadata("language"),
        publisher: metadata("publisher"),
        description: metadata("description")
            .map(|d| strip_markup(&d))
            .filter(|d| !d.is_empty()),
        identifiers: all("identifier"),
        cover,
        spine,
        navigation,
        guide,
    })
}

fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_markup(text: &str) -> String {
    let fragment = scraper::Html::parse_fragment(text);
    collapse(&fragment.root_element().text().collect::<String>())
}

/// Archive path and media type of the declared cover image, if any.
fn cover_path(doc: &Document<'_>, package: &str) -> Result<Option<(String, String)>, ParseError> {
    let items = || doc.descendants().filter(|n| n.has_tag_name("item"));
    let epub3 = items().find(|n| {
        n.attribute("properties")
            .is_some_and(|p| p.split_whitespace().any(|v| v == "cover-image"))
    });
    let epub2 = || {
        let id = doc
            .descendants()
            .find(|n| n.has_tag_name("meta") && n.attribute("name") == Some("cover"))
            .and_then(|n| n.attribute("content"))?;
        items().find(|n| n.attribute("id") == Some(id))
    };
    let Some(item) = epub3.or_else(epub2) else {
        return Ok(None);
    };
    let media_type = item.attribute("media-type").unwrap_or("");
    let Some(href) = item.attribute("href") else {
        return Ok(None);
    };
    if !media_type.starts_with("image/") {
        return Ok(None);
    }
    Ok(Some((resolve(package, href)?.path, media_type.to_owned())))
}

/// Digits-only ISBN (with `X` check digit) when `identifier` is a checksum-valid ISBN-10/13.
pub(crate) fn isbn(identifier: &str) -> Option<String> {
    let lower = identifier.trim().to_ascii_lowercase();
    let value = lower.strip_prefix("urn:isbn:").unwrap_or(&lower);
    let cleaned: String = value
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let valid = match cleaned.len() {
        10 => {
            let mut sum = 0;
            for (i, c) in cleaned.chars().enumerate() {
                let digit = match c {
                    'X' if i == 9 => 10,
                    c => c.to_digit(10)?,
                };
                sum += digit * (10 - i as u32);
            }
            sum % 11 == 0
        }
        13 => {
            let mut sum = 0;
            for (i, c) in cleaned.chars().enumerate() {
                sum += c.to_digit(10)? * if i % 2 == 0 { 1 } else { 3 };
            }
            sum % 10 == 0
        }
        _ => false,
    };
    valid.then_some(cleaned)
}

fn check_encryption(
    archive: &mut Archive<'_>,
    spine: &[String],
    navigation: Option<&str>,
) -> Result<(), ParseError> {
    if !archive.contains("META-INF/encryption.xml") {
        return Ok(());
    }
    let source = archive.read("META-INF/encryption.xml")?;
    let doc = xml(&source)?;
    for reference in doc
        .descendants()
        .filter(|n| n.has_tag_name("CipherReference"))
    {
        let uri = reference
            .attribute("URI")
            .ok_or_else(|| ParseError::Invalid("encrypted resource has no URI".into()))?;
        let path = resolve("", uri)?.path;
        if spine.contains(&path) || navigation == Some(path.as_str()) {
            return Err(ParseError::UnsupportedEncryption(path));
        }
    }
    Ok(())
}

pub(crate) fn xml(source: &str) -> Result<Document<'_>, ParseError> {
    // External DOCTYPE declarations are common in EPUB 2. roxmltree never fetches
    // them. Reject custom entities rather than expanding an untrusted DTD.
    if source.contains("<!ENTITY") {
        return Err(ParseError::Invalid(
            "custom XML entities are unsupported".into(),
        ));
    }
    Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 200_000,
            ..Default::default()
        },
    )
    .map_err(|error| ParseError::Invalid(error.to_string()))
}
