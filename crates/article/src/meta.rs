//! What the page says about itself. Three levels, each field taken from the first that has it:
//! JSON-LD `Article` (or a kind of it), then Open Graph and `article:*` meta (with Dublin Core
//! `dc:*` and Parse.ly `parsely-*` where those say nothing), then the plain page (`<title>`,
//! `<meta name="author">`, `<html lang>`, `<link rel="canonical">`).

use scraper::{Html, Selector};
use serde_json::Value;
use url::Url;

use crate::blocks::collapse;
use crate::{ArticleMeta, Published};

/// schema.org types that are articles.
const ARTICLE_TYPES: &[&str] = &[
    "Article",
    "NewsArticle",
    "AnalysisNewsArticle",
    "BackgroundNewsArticle",
    "OpinionNewsArticle",
    "ReportageNewsArticle",
    "ReviewNewsArticle",
    "BlogPosting",
    "LiveBlogPosting",
    "Report",
    "ScholarlyArticle",
    "TechArticle",
    "SocialMediaPosting",
];

/// One level's answers.
#[derive(Default)]
struct Level {
    title: Option<String>,
    /// JSON-LD's `name`, the title when its `headline` is not.
    other_title: Option<String>,
    byline: Option<String>,
    site_name: Option<String>,
    published: Option<String>,
    language: Option<String>,
    canonical: Option<String>,
    image: Option<String>,
    description: Option<String>,
}

/// The JSON-LD article node's own fields that are not metadata.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    pub body: Option<String>,
    pub word_count: Option<usize>,
    pub free: Option<bool>,
}

pub(crate) struct Read {
    pub meta: ArticleMeta,
    pub description: Option<String>,
    pub declared: Declared,
}

/// Reads the page's metadata. `found_title` is the content finder's title, which has the
/// site's name taken off; it stands in for `<title>` when there is one. `found_byline` is the
/// content finder's byline, the last one to fall back on.
pub(crate) fn read(
    page: &Html,
    url: &Url,
    found_title: Option<String>,
    found_byline: Option<String>,
) -> Read {
    let (json, declared) = json_ld(page);
    let levels = [
        json,
        open_graph(page),
        plain(page, found_title, found_byline),
    ];
    let pick = |field: fn(&Level) -> &Option<String>| levels.iter().find_map(|l| field(l).clone());
    let link = |field: fn(&Level) -> &Option<String>| {
        levels
            .iter()
            .filter_map(|l| field(l).as_deref())
            .find_map(|href| resolve(url, href))
    };
    let published = pick(|l| &l.published).map(|text| Published {
        at_ms: parse_time(&text),
        text,
    });
    Read {
        meta: ArticleMeta {
            title: title(&levels, &headings(page)),
            byline: pick(|l| &l.byline),
            site_name: pick(|l| &l.site_name),
            published,
            language: pick(|l| &l.language),
            canonical_url: link(|l| &l.canonical),
            lead_image: link(|l| &l.image),
            word_count: 0,
            declared_words: None,
        },
        description: pick(|l| &l.description),
        declared,
    }
}

/// The title one of the page's `headings` shows, from any level (social titles are often
/// rewritten for sharing); else JSON-LD's `headline`, else its `name`, when it agrees with the
/// page's own titles (some sites put a description of the subject in `headline`); else the
/// first page title. A site's
/// name after it ("Title | Site") is taken off when another level's title or one of the page's
/// `headings` is the same without it.
fn title(levels: &[Level; 3], headings: &[String]) -> Option<String> {
    let [json, rest @ ..] = levels;
    let page: Vec<&str> = rest.iter().filter_map(|l| l.title.as_deref()).collect();
    let agrees = |title: &str| page.is_empty() || page.iter().any(|p| alike(title, p));
    let shown = [&json.title, &json.other_title]
        .into_iter()
        .chain(rest.iter().map(|l| &l.title))
        .flatten()
        .find(|t| {
            headings
                .iter()
                .any(|h| h.to_lowercase() == t.to_lowercase())
        });
    let chosen = shown
        .or_else(|| {
            [&json.title, &json.other_title]
                .into_iter()
                .flatten()
                .find(|t| agrees(t))
        })
        .cloned()
        .or_else(|| rest.iter().find_map(|l| l.title.clone()))?;
    let shorter = levels
        .iter()
        .filter_map(|l| l.title.as_deref())
        .chain(headings.iter().map(String::as_str))
        .find(|other| {
            chosen.len() > other.len()
                && chosen.starts_with(*other)
                && chosen[other.len()..]
                    .trim_start()
                    .starts_with(['|', '-', '–', '—', '·', ':'])
        });
    Some(shorter.map_or(chosen.clone(), str::to_owned))
}

/// The texts of the page's first top-level headings.
fn headings(page: &Html) -> Vec<String> {
    page.select(&selector("h1, h2"))
        .take(8)
        .filter_map(|h| clean(&h.text().collect::<String>()))
        .collect()
}

/// Whether two titles share at least half the words of the shorter.
fn alike(a: &str, b: &str) -> bool {
    let words = |text: &str| -> Vec<String> {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let (a, b) = (words(a), words(b));
    let shared = a.iter().filter(|w| b.contains(w)).count();
    shared > 0 && shared * 2 >= a.len().min(b.len())
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

fn clean(text: &str) -> Option<String> {
    let text = collapse(&html_escape::decode_html_entities(text));
    (!text.is_empty()).then_some(text)
}

/// `href` made absolute against the page; only web addresses count.
fn resolve(url: &Url, href: &str) -> Option<Url> {
    let resolved = url.join(href.trim()).ok()?;
    matches!(resolved.scheme(), "http" | "https").then_some(resolved)
}

// --- JSON-LD ---------------------------------------------------------------------------------

fn json_ld(page: &Html) -> (Level, Declared) {
    let scripts = selector("script[type]");
    let node = page
        .select(&scripts)
        .filter(|s| {
            s.value()
                .attr("type")
                .is_some_and(|t| t.trim().eq_ignore_ascii_case("application/ld+json"))
        })
        .filter_map(|s| {
            serde_json::from_str::<Value>(without_cdata(&s.text().collect::<String>())).ok()
        })
        .find_map(|value| article_node(&value).cloned());
    let Some(node) = node else {
        return Default::default();
    };
    let string = |key: &str| node.get(key).and_then(Value::as_str).and_then(clean);
    let level = Level {
        title: string("headline").or_else(|| string("name")),
        other_title: string("headline").and(string("name")),
        byline: node.get("author").and_then(names),
        site_name: node
            .get("publisher")
            .and_then(|p| p.as_str().map(str::to_owned).or_else(|| first_name(p)))
            .and_then(|n| clean(&n)),
        published: string("datePublished"),
        language: node
            .get("inLanguage")
            .and_then(Value::as_str)
            .and_then(clean),
        canonical: string("url").or_else(|| {
            node.get("mainEntityOfPage").and_then(|m| {
                m.as_str()
                    .or_else(|| m.get("@id").and_then(Value::as_str))
                    .map(str::to_owned)
            })
        }),
        image: node.get("image").and_then(image),
        description: string("description"),
    };
    let declared = Declared {
        body: node
            .get("articleBody")
            .and_then(Value::as_str)
            .map(|b| b.trim().to_owned())
            .filter(|b| !b.is_empty()),
        word_count: node.get("wordCount").and_then(|w| {
            w.as_u64()
                .map(|n| n as usize)
                .or_else(|| w.as_str().and_then(|s| s.trim().parse().ok()))
        }),
        free: node.get("isAccessibleForFree").and_then(|f| {
            f.as_bool().or_else(|| match f.as_str()?.trim() {
                s if s.eq_ignore_ascii_case("true") => Some(true),
                s if s.eq_ignore_ascii_case("false") => Some(false),
                _ => None,
            })
        }),
    };
    (level, declared)
}

/// A script's text without the CDATA section some pages wrap it in (`//<![CDATA[ … //]]>`).
fn without_cdata(text: &str) -> &str {
    let text = text.trim();
    let Some(inner) = text
        .strip_prefix("//")
        .map(str::trim_start)
        .unwrap_or(text)
        .strip_prefix("<![CDATA[")
    else {
        return text;
    };
    let inner = inner.strip_suffix("]]>").unwrap_or(inner).trim_end();
    inner.strip_suffix("//").unwrap_or(inner).trim()
}

/// The first node in `value` (an object, an array, or a `@graph`) typed as an article.
fn article_node(value: &Value) -> Option<&Value> {
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            Value::Array(items) => stack.extend(items.iter().rev()),
            Value::Object(map) => {
                let typed = match map.get("@type") {
                    Some(Value::String(t)) => ARTICLE_TYPES.contains(&t.as_str()),
                    Some(Value::Array(ts)) => ts
                        .iter()
                        .any(|t| t.as_str().is_some_and(|t| ARTICLE_TYPES.contains(&t))),
                    _ => false,
                };
                if typed {
                    return Some(value);
                }
                if let Some(graph) = map.get("@graph") {
                    stack.push(graph);
                }
            }
            _ => {}
        }
    }
    None
}

fn first_name(value: &Value) -> Option<String> {
    value.get("name").and_then(Value::as_str).map(str::to_owned)
}

/// Authors as one line: a name, a person, or a list of either.
fn names(value: &Value) -> Option<String> {
    let mut all: Vec<String> = Vec::new();
    let items: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        other => vec![other],
    };
    for item in items {
        let name = item
            .as_str()
            .map(str::to_owned)
            .or_else(|| first_name(item));
        if let Some(name) = name.and_then(|n| clean(&n)) {
            if !all.contains(&name) {
                all.push(name);
            }
        }
    }
    (!all.is_empty()).then(|| all.join(", "))
}

fn image(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => items.iter().find_map(image),
        Value::Object(map) => map
            .get("url")
            .or_else(|| map.get("contentUrl"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

// --- Open Graph and article:* -----------------------------------------------------------------

/// The content of the first `<meta>` whose `property` names `key`, else of the first whose
/// `name` does; either may name several keys, separated by spaces. Empty contents do not count.
fn meta_content(page: &Html, key: &str) -> Option<String> {
    let metas = selector("meta[content]");
    let names_key = |keys: Option<&str>| {
        keys.is_some_and(|keys| keys.split_whitespace().any(|k| k.eq_ignore_ascii_case(key)))
    };
    ["property", "name"].into_iter().find_map(|attr| {
        page.select(&metas)
            .filter(|m| names_key(m.value().attr(attr)))
            .find_map(|m| m.value().attr("content").and_then(clean))
    })
}

fn open_graph(page: &Html) -> Level {
    let get = |key: &str| meta_content(page, key);
    let first = |keys: &[&str]| keys.iter().find_map(|k| get(k));
    Level {
        title: first(&["og:title", "dc:title", "parsely-title"]),
        // `article:author` is often a profile link rather than a name.
        byline: get("article:author")
            .filter(|a| !a.starts_with("http"))
            .or_else(|| first(&["dc:creator", "parsely-author"])),
        site_name: get("og:site_name"),
        published: first(&["article:published_time", "parsely-pub-date"]),
        language: get("og:locale").map(|l| l.replace('_', "-")),
        canonical: get("og:url"),
        image: get("og:image").or_else(|| get("og:image:url")),
        description: first(&["og:description", "dc:description"]),
        other_title: None,
    }
}

// --- The plain page ---------------------------------------------------------------------------

fn plain(page: &Html, found_title: Option<String>, found_byline: Option<String>) -> Level {
    let title = found_title.or_else(|| {
        page.select(&selector("title"))
            .next()
            .and_then(|t| clean(&t.text().collect::<String>()))
    });
    let canonical = page
        .select(&selector("link[rel][href]"))
        .find(|l| {
            l.value().attr("rel").is_some_and(|r| {
                r.split_whitespace()
                    .any(|t| t.eq_ignore_ascii_case("canonical"))
            })
        })
        .and_then(|l| l.value().attr("href"))
        .map(str::to_owned);
    Level {
        title,
        byline: meta_content(page, "author")
            .or_else(|| found_byline.as_deref().and_then(byline_shown)),
        site_name: None,
        published: None,
        language: page
            .select(&selector("html[lang]"))
            .next()
            .and_then(|h| h.value().attr("lang"))
            .and_then(clean),
        canonical,
        image: None,
        description: meta_content(page, "description"),
        other_title: None,
    }
}

/// Words that mark a line near the top of an article as something other than its byline.
const NOT_A_BYLINE: &[&str] = &[
    "follow",
    "updated",
    "published",
    "posted",
    "share",
    "subscribe",
    "comments",
];

/// The author's name in a byline taken from the article: "By" (and its kin) taken off, and none
/// when what was taken holds a date, a button's words or several credits ("… Edited by …").
fn byline_shown(line: &str) -> Option<String> {
    let line = clean(line)?;
    let name = ["written by ", "by ", "par ", "por ", "von ", "door ", "di "]
        .iter()
        .find_map(|p| {
            let head = line.get(..p.len())?;
            head.eq_ignore_ascii_case(p)
                .then(|| line[p.len()..].trim_start())
        })
        .unwrap_or(&line);
    let words: Vec<String> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let refused = name.chars().any(|c| c.is_ascii_digit())
        || words
            .iter()
            .any(|w| w == "by" || NOT_A_BYLINE.contains(&w.as_str()));
    (!refused && !name.is_empty()).then(|| name.to_owned())
}

// --- Feeds ----------------------------------------------------------------------------------

/// A feed the page links to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedLink {
    pub url: Url,
    pub title: Option<String>,
}

const FEED_TYPES: &[&str] = &[
    "application/rss+xml",
    "application/atom+xml",
    "application/feed+json",
];

/// The feeds named by `<link rel="alternate">`, in page order, each once.
pub(crate) fn feeds(page: &Html, url: &Url) -> Vec<FeedLink> {
    let mut found: Vec<FeedLink> = Vec::new();
    for link in page.select(&selector("link[rel][type][href]")) {
        let e = link.value();
        let alternate = e.attr("rel").is_some_and(|r| {
            r.split_whitespace()
                .any(|t| t.eq_ignore_ascii_case("alternate"))
        });
        let feed = e
            .attr("type")
            .is_some_and(|t| FEED_TYPES.iter().any(|f| t.trim().eq_ignore_ascii_case(f)));
        let Some(href) = e.attr("href").filter(|_| alternate && feed) else {
            continue;
        };
        if let Some(url) = resolve(url, href) {
            if !found.iter().any(|f| f.url == url) {
                found.push(FeedLink {
                    url,
                    title: e.attr("title").and_then(clean),
                });
            }
        }
    }
    found
}

// --- Dates ------------------------------------------------------------------------------------

/// Milliseconds since the Unix epoch for an ISO 8601 date or date-time (`2026-03-04`,
/// `2026-03-04T05:06`, with seconds, fractions and a `Z` or `±hh:mm` offset; none is UTC).
pub(crate) fn parse_time(text: &str) -> Option<i64> {
    let text = text.trim();
    let number = |s: &str| -> Option<i64> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let (date, time) = match text.find(['T', ' ']) {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text, ""),
    };
    let mut parts = date.split('-');
    let (year, month, day) = (
        number(parts.next()?)?,
        number(parts.next()?)?,
        number(parts.next()?)?,
    );
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (clock, offset_min) = match time.rfind(['Z', 'z', '+', '-']) {
        Some(at) => {
            let zone = &time[at..];
            let offset = if zone.eq_ignore_ascii_case("z") {
                0
            } else {
                let sign = if zone.starts_with('-') { -1 } else { 1 };
                let digits: String = zone[1..].chars().filter(|c| *c != ':').collect();
                if digits.len() != 4 {
                    return None;
                }
                sign * (number(&digits[..2])? * 60 + number(&digits[2..])?)
            };
            (&time[..at], offset)
        }
        None => (time, 0),
    };
    let (mut hour, mut minute, mut second, mut millis) = (0, 0, 0, 0);
    if !clock.is_empty() {
        let (whole, fraction) = clock.split_once(['.', ',']).unwrap_or((clock, ""));
        let mut hms = whole.split(':');
        hour = number(hms.next()?)?;
        minute = number(hms.next()?)?;
        second = hms.next().map(number).unwrap_or(Some(0))?;
        if hms.next().is_some() || hour > 23 || minute > 59 || second > 60 {
            return None;
        }
        if !fraction.is_empty() {
            let padded: String = fraction.chars().chain("000".chars()).take(3).collect();
            millis = number(&padded)?;
        }
    }
    let days = days_from_civil(year, month, day);
    Some(((days * 86_400 + hour * 3_600 + minute * 60 + second) - offset_min * 60) * 1_000 + millis)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
