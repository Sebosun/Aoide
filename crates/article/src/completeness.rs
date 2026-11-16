//! Whether the text captured is the article: whole, cut short, behind a wall, or nothing.

use scraper::{Html, Selector};

use crate::patterns::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Completeness {
    Complete,
    /// Far fewer words than the page declares.
    CutShort {
        captured: usize,
        declared: usize,
    },
    Wall(WallKind),
    /// Nothing readable was found.
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallKind {
    SignIn,
    Paywall,
    Check,
}

/// What the whole page shows, beside the text taken from it.
#[derive(Debug, Default)]
pub(crate) struct Signs {
    /// The title and visible text, lower case.
    pub text: String,
    pub words: usize,
    pub password_field: bool,
    pub paywall_container: bool,
}

impl Signs {
    pub fn of(page: &Html) -> Self {
        let mut text = String::new();
        if let Some(title) = page.select(&selector("title")).next() {
            text.extend(title.text());
            text.push(' ');
        }
        if let Some(body) = page.select(&selector("body")).next() {
            let hidden = ["script", "style", "template", "noscript"];
            for node in body.descendants() {
                if let Some(t) = node.value().as_text() {
                    let shown = !node.ancestors().any(
                        |a| matches!(a.value().as_element(), Some(e) if hidden.contains(&e.name())),
                    );
                    if shown {
                        text.push_str(t);
                        text.push(' ');
                    }
                }
            }
        }
        let paywall_container = page.select(&selector("[class], [id]")).any(|e| {
            let e = e.value();
            [e.attr("class"), e.attr("id")]
                .into_iter()
                .flatten()
                .any(|value| {
                    let value = value.to_ascii_lowercase();
                    PAYWALL_MARKERS.iter().any(|m| value.contains(m))
                })
        });
        let words = crate::words(&text);
        Signs {
            text: text.to_lowercase(),
            words,
            password_field: page.select(&selector("input[type]")).any(|i| {
                i.value()
                    .attr("type")
                    .is_some_and(|t| t.eq_ignore_ascii_case("password"))
            }),
            paywall_container,
        }
    }
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("a valid selector")
}

/// Judges `captured` words against what the page declares and shows.
pub(crate) fn judge(
    captured: usize,
    declared: Option<usize>,
    free: Option<bool>,
    signs: &Signs,
) -> Completeness {
    let short_page = signs.words < SHORT_PAGE_WORDS;
    let falls_short = |declared: usize| captured * 100 < declared * CUT_SHORT_PERCENT;
    if short_page && CHECK_PHRASES.iter().any(|p| signs.text.contains(p)) {
        return Completeness::Wall(WallKind::Check);
    }
    if signs.password_field && captured < SHORT_PAGE_WORDS {
        return Completeness::Wall(WallKind::SignIn);
    }
    let short_body = captured < PAYWALL_SHORT_WORDS || declared.is_some_and(falls_short);
    // A paywall's marks (a page that is not free, a paywall's container) count only when the
    // text that came through is short: sites mark whole articles they let through too.
    if (free == Some(false) || signs.paywall_container) && short_body {
        return Completeness::Wall(WallKind::Paywall);
    }
    if captured < EMPTY_WORDS {
        return Completeness::Empty;
    }
    match declared {
        Some(declared) if declared >= CUT_SHORT_MIN_DECLARED && falls_short(declared) => {
            Completeness::CutShort { captured, declared }
        }
        _ => Completeness::Complete,
    }
}
