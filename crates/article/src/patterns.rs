//! What marks a page that did not give its article. Each entry has a page in
//! `tests/completeness.rs` that it catches and a near miss it must not.

/// Words in the whole page under which a page counts as short.
pub(crate) const SHORT_PAGE_WORDS: usize = 150;
/// Words captured under which nothing readable was found.
pub(crate) const EMPTY_WORDS: usize = 40;
/// Declared words from which a shortfall is judged.
pub(crate) const CUT_SHORT_MIN_DECLARED: usize = 300;
/// The share of declared words a whole capture reaches, in percent.
pub(crate) const CUT_SHORT_PERCENT: usize = 60;
/// Words captured under which a page that is not free is taken to be paywalled.
pub(crate) const PAYWALL_SHORT_WORDS: usize = 300;

/// Phrases of challenge pages ("checking your browser"), matched in lower case against a short
/// page's title and text.
pub(crate) const CHECK_PHRASES: &[&str] = &[
    "checking your browser",
    "checking if the site connection is secure",
    "just a moment...",
    "attention required!",
    "verify you are human",
    "verifying you are human",
    "enable javascript and cookies to continue",
    "ddos protection by",
    "please complete the security check",
    "are you a robot",
    "unusual traffic from your computer network",
    "press & hold to confirm you are",
];

/// Class or id fragments of the containers paywalls put on a page, matched in lower case.
pub(crate) const PAYWALL_MARKERS: &[&str] = &[
    "paywall",
    "regwall",
    "subscriber-only",
    "subscription-wall",
    "metered-content",
    "premium-wall",
    "tp-modal",
];
