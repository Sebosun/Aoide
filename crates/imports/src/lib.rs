//! Read-later exports and lists of links, read into saved links: Pocket (CSV, and the older
//! HTML), Instapaper (CSV), Omnivore (its zip), Wallabag (JSON), Readwise Reader (CSV), browser
//! bookmarks (Netscape HTML) and plain lists. Pure: no network, no clock. Only `http` and
//! `https` addresses are kept; duplicates are the caller's to drop (by its own rule for when two
//! addresses are one page).

mod csv;

use std::io::Read as _;

use url::Url;

/// One link from an export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedLink {
    pub url: Url,
    pub title: Option<String>,
    /// When it was saved in the old app, Unix milliseconds.
    pub saved_at: Option<i64>,
    /// Its tags, labels or folder there.
    pub labels: Vec<String>,
    /// Archived (read) there.
    pub archived: bool,
    /// The page's text as the app saved it (Omnivore, Wallabag), when it has it.
    pub content_html: Option<String>,
}

/// Where a file came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Pocket,
    Instapaper,
    Omnivore,
    Wallabag,
    Readwise,
    Bookmarks,
    PlainList,
}

impl Source {
    /// A stable name, for the log and the library.
    pub fn name(self) -> &'static str {
        match self {
            Source::Pocket => "pocket",
            Source::Instapaper => "instapaper",
            Source::Omnivore => "omnivore",
            Source::Wallabag => "wallabag",
            Source::Readwise => "readwise",
            Source::Bookmarks => "bookmarks",
            Source::PlainList => "list",
        }
    }
}

/// The links read from a file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Read {
    pub links: Vec<SavedLink>,
    /// Entries whose address is not a web page (`mailto:`, `javascript:`, …).
    pub not_web: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    /// Not the file this source exports.
    NotThisExport(&'static str),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotThisExport(what) => write!(f, "This is not {what}."),
        }
    }
}

impl std::error::Error for ImportError {}

/// Which export `name` (its file name) with these first bytes is; `None` when it is none of
/// them.
pub fn detect(name: &str, head: &[u8]) -> Option<Source> {
    let name = name.to_ascii_lowercase();
    if head.starts_with(b"PK\x03\x04") {
        return omnivore_metadata(head).is_ok().then_some(Source::Omnivore);
    }
    let text = String::from_utf8_lossy(&head[..head.len().min(64 * 1024)]);
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        return trimmed
            .contains("\"is_archived\"")
            .then_some(Source::Wallabag);
    }
    if text.contains("NETSCAPE-Bookmark-file") {
        return Some(Source::Bookmarks);
    }
    if trimmed.starts_with('<') {
        return (name.starts_with("ril_export") || text.contains("Pocket Export"))
            .then_some(Source::Pocket);
    }
    let header = trimmed.lines().next().unwrap_or("").to_ascii_lowercase();
    if header.starts_with("title,url,time_added") {
        return Some(Source::Pocket);
    }
    if header.starts_with("url,title,selection,folder") {
        return Some(Source::Instapaper);
    }
    if header.starts_with("title,url,id,document tags") {
        return Some(Source::Readwise);
    }
    let first = trimmed.lines().map(str::trim).find(|l| !l.is_empty());
    if first.is_some_and(|l| l.starts_with("http://") || l.starts_with("https://"))
        || name.ends_with(".txt")
    {
        return Some(Source::PlainList);
    }
    None
}

/// The links in `bytes`, read as `source` exports them.
pub fn read(source: Source, bytes: &[u8]) -> Result<Read, ImportError> {
    let text = || {
        String::from_utf8_lossy(bytes)
            .trim_start_matches('\u{feff}')
            .to_owned()
    };
    let mut read = Read::default();
    match source {
        Source::Pocket if text().trim_start().starts_with('<') => pocket_html(&text(), &mut read),
        Source::Pocket => pocket_csv(&text(), &mut read)?,
        Source::Instapaper => instapaper(&text(), &mut read)?,
        Source::Readwise => readwise(&text(), &mut read)?,
        Source::Wallabag => wallabag(&text(), &mut read)?,
        Source::Omnivore => omnivore(bytes, &mut read)?,
        Source::Bookmarks => bookmarks(&text(), &mut read),
        Source::PlainList => plain(&text(), &mut read),
    }
    Ok(read)
}

impl Read {
    /// Keeps the link if its address is a web page; counts it otherwise.
    fn push(&mut self, url: &str, mut link: impl FnMut(Url) -> SavedLink) {
        match Url::parse(url.trim()) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => self.links.push(link(url)),
            _ => self.not_web += 1,
        }
    }
}

fn some(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn seconds(text: &str) -> Option<i64> {
    text.trim().parse::<i64>().ok().map(|s| s * 1000)
}

/// A CSV export's rows by its header's names (compared ignoring case); `None` when a needed
/// column is missing.
fn rows(text: &str, needed: &[&str]) -> Option<Vec<std::collections::HashMap<String, String>>> {
    let mut records = csv::records(text).into_iter();
    let header: Vec<String> = records
        .next()?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    if !needed.iter().all(|n| header.iter().any(|h| h == n)) {
        return None;
    }
    Some(
        records
            .filter(|r| r.iter().any(|f| !f.trim().is_empty()))
            .map(|r| header.iter().cloned().zip(r).collect())
            .collect(),
    )
}

fn field<'a>(row: &'a std::collections::HashMap<String, String>, name: &str) -> &'a str {
    row.get(name).map(String::as_str).unwrap_or("")
}

fn pocket_csv(text: &str, read: &mut Read) -> Result<(), ImportError> {
    let rows = rows(text, &["title", "url", "time_added"])
        .ok_or(ImportError::NotThisExport("a Pocket export"))?;
    for row in rows {
        read.push(field(&row, "url"), |url| SavedLink {
            url,
            title: some(field(&row, "title")),
            saved_at: seconds(field(&row, "time_added")),
            labels: split(field(&row, "tags"), '|'),
            archived: field(&row, "status").eq_ignore_ascii_case("archive"),
            content_html: None,
        });
    }
    Ok(())
}

fn split(text: &str, by: char) -> Vec<String> {
    text.split(by).filter_map(some).collect()
}

/// The older `ril_export.html`: lists under "Unread" and "Read Archive" headings.
fn pocket_html(text: &str, read: &mut Read) {
    let page = scraper::Html::parse_document(text);
    let selector = scraper::Selector::parse("h1, a").expect("a valid selector");
    let mut archived = false;
    for element in page.select(&selector) {
        let value = element.value();
        if value.name() == "h1" {
            archived = element
                .text()
                .collect::<String>()
                .to_lowercase()
                .contains("archive");
            continue;
        }
        let Some(href) = value.attr("href") else {
            continue;
        };
        read.push(href, |url| SavedLink {
            url,
            title: some(&element.text().collect::<String>()),
            saved_at: value.attr("time_added").and_then(seconds),
            labels: split(value.attr("tags").unwrap_or(""), ','),
            archived,
            content_html: None,
        });
    }
}

fn instapaper(text: &str, read: &mut Read) -> Result<(), ImportError> {
    let rows = rows(text, &["url", "title", "folder"])
        .ok_or(ImportError::NotThisExport("an Instapaper export"))?;
    for row in rows {
        let folder = field(&row, "folder").trim();
        let archived = folder.eq_ignore_ascii_case("archive");
        let labels = match folder.to_ascii_lowercase().as_str() {
            "" | "unread" | "archive" => Vec::new(),
            _ => vec![folder.to_owned()],
        };
        read.push(field(&row, "url"), |url| SavedLink {
            url,
            title: some(field(&row, "title")),
            saved_at: seconds(field(&row, "timestamp")),
            labels: labels.clone(),
            archived,
            content_html: None,
        });
    }
    Ok(())
}

/// Readwise Reader's CSV: tags written as a Python list (`['a', 'b']`), dates with an offset.
fn readwise(text: &str, read: &mut Read) -> Result<(), ImportError> {
    let rows = rows(text, &["title", "url", "document tags"])
        .ok_or(ImportError::NotThisExport("a Readwise Reader export"))?;
    for row in rows {
        let labels = field(&row, "document tags")
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .filter_map(|t| some(t.trim().trim_matches(|c| c == '\'' || c == '"')))
            .collect::<Vec<_>>();
        let saved_at = chrono::DateTime::parse_from_str(
            field(&row, "saved date").trim(),
            "%Y-%m-%d %H:%M:%S%:z",
        )
        .ok()
        .map(|d| d.timestamp_millis());
        read.push(field(&row, "url"), |url| SavedLink {
            url,
            title: some(field(&row, "title")),
            saved_at,
            labels: labels.clone(),
            archived: field(&row, "location").eq_ignore_ascii_case("archive"),
            content_html: None,
        });
    }
    Ok(())
}

fn string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned()
}

fn labels(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|tags| {
            tags.iter()
                .filter_map(|t| {
                    t.as_str()
                        .or_else(|| t.get("label").and_then(|l| l.as_str()))
                })
                .filter_map(some)
                .collect()
        })
        .unwrap_or_default()
}

fn wallabag(text: &str, read: &mut Read) -> Result<(), ImportError> {
    let not = ImportError::NotThisExport("a Wallabag export");
    let entries: serde_json::Value = serde_json::from_str(text).map_err(|_| not.clone())?;
    let entries = entries.as_array().ok_or(not)?;
    for entry in entries {
        let archived = match entry.get("is_archived") {
            Some(serde_json::Value::Bool(b)) => *b,
            Some(v) => v.as_i64() == Some(1),
            None => false,
        };
        let saved_at =
            chrono::DateTime::parse_from_str(&string(entry, "created_at"), "%Y-%m-%dT%H:%M:%S%z")
                .ok()
                .map(|d| d.timestamp_millis());
        read.push(&string(entry, "url"), |url| SavedLink {
            url,
            title: some(&string(entry, "title")),
            saved_at,
            labels: labels(entry, "tags"),
            archived,
            content_html: some(&string(entry, "content")),
        });
    }
    Ok(())
}

type Zip<'a> = zip::ZipArchive<std::io::Cursor<&'a [u8]>>;

/// Omnivore's zip: `metadata_*.json` lists, and each saved page as `content/<slug>.html`.
fn omnivore_metadata(bytes: &[u8]) -> Result<(Zip<'_>, Vec<String>), ImportError> {
    let not = ImportError::NotThisExport("an Omnivore export");
    let zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|_| not.clone())?;
    let mut lists: Vec<String> = zip
        .file_names()
        .filter(|n| {
            n.rsplit('/')
                .next()
                .is_some_and(|f| f.starts_with("metadata_") && f.ends_with(".json"))
        })
        .map(str::to_owned)
        .collect();
    if lists.is_empty() {
        return Err(not);
    }
    lists.sort();
    Ok((zip, lists))
}

fn omnivore(bytes: &[u8], read: &mut Read) -> Result<(), ImportError> {
    let (mut zip, lists) = omnivore_metadata(bytes)?;
    let not = || ImportError::NotThisExport("an Omnivore export");
    let mut entries = Vec::new();
    for list in lists {
        let mut json = String::new();
        zip.by_name(&list)
            .map_err(|_| not())?
            .read_to_string(&mut json)
            .map_err(|_| not())?;
        let list: serde_json::Value = serde_json::from_str(&json).map_err(|_| not())?;
        entries.extend(list.as_array().cloned().unwrap_or_default());
    }
    let mut folder = |name: &str| {
        let content = zip
            .file_names()
            .find(|n| n.ends_with(&format!("content/{name}.html")))
            .map(str::to_owned)?;
        let mut html = String::new();
        zip.by_name(&content).ok()?.read_to_string(&mut html).ok()?;
        some(&html)
    };
    let pages: Vec<Option<String>> = entries.iter().map(|e| folder(&string(e, "slug"))).collect();
    for (entry, page) in entries.iter().zip(pages) {
        let saved_at = chrono::DateTime::parse_from_rfc3339(&string(entry, "savedAt"))
            .ok()
            .map(|d| d.timestamp_millis());
        read.push(&string(entry, "url"), |url| SavedLink {
            url,
            title: some(&string(entry, "title")),
            saved_at,
            labels: labels(entry, "labels"),
            archived: string(entry, "state").eq_ignore_ascii_case("archived"),
            content_html: page.clone(),
        });
    }
    Ok(())
}

/// A browser's bookmarks (the Netscape format every browser exports).
fn bookmarks(text: &str, read: &mut Read) {
    let page = scraper::Html::parse_document(text);
    let selector = scraper::Selector::parse("a[href]").expect("a valid selector");
    for element in page.select(&selector) {
        let value = element.value();
        read.push(value.attr("href").unwrap_or(""), |url| SavedLink {
            url,
            title: some(&element.text().collect::<String>()),
            saved_at: value.attr("add_date").and_then(seconds),
            labels: split(value.attr("tags").unwrap_or(""), ','),
            archived: false,
            content_html: None,
        });
    }
}

/// One address a line; blank lines and `#` notes are skipped.
fn plain(text: &str, read: &mut Read) {
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        read.push(line, |url| SavedLink {
            url,
            title: None,
            saved_at: None,
            labels: Vec::new(),
            archived: false,
            content_html: None,
        });
    }
}
