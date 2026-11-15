//! Per-site rules for fetching articles, kept in `site-rules.toml` next to `settings.toml`:
//! one table per host. Aoide ships none; the user writes them, or Settings sets a site's
//! `fetch` and `site_data` (every other key, known or not, is kept when it writes the file).
//!
//! ```toml
//! ["example.com"]               # also covers its subdomains, unless one has its own rule
//! fetch = "browser"             # or "http"; left out: the global choice
//! site_data = "clear_before_fetch"  # or "keep" (the default)
//! block = ["*/ads/*"]           # URL patterns the browser does not load
//! wait_for = "article .body"    # CSS selector that marks the article as present
//! user_agent = "Mozilla/5.0 …"
//! fallbacks = ["archive"]       # also try an archived copy ("json_ld" is always tried)
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ConfigError;

/// File name of the site rules inside the config folder.
pub const SITE_RULES_FILE: &str = "site-rules.toml";

/// How a site's articles are fetched, overriding the global choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SiteFetch {
    /// Plain HTTP only.
    Http,
    /// The browser.
    Browser,
}

/// What happens to a site's cookies and storage in the browser.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SiteData {
    /// Kept between fetches (a sign-in stays).
    #[default]
    Keep,
    /// Cleared before each fetch.
    ClearBeforeFetch,
}

/// Where else an article's text may come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fallback {
    /// The page's JSON-LD (extraction always reads it).
    JsonLd,
    /// An archived copy of the page.
    Archive,
}

/// One site's rule.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteRule {
    pub fetch: Option<SiteFetch>,
    pub site_data: SiteData,
    pub block: Vec<String>,
    pub wait_for: Option<String>,
    pub user_agent: Option<String>,
    pub fallbacks: Vec<Fallback>,
}

/// A host as rules name it: lower case, without a final dot or a leading `www.`.
pub fn site_host(host: &str) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    match host.strip_prefix("www.") {
        Some(rest) if !rest.is_empty() => rest.to_owned(),
        _ => host,
    }
}

/// The rules file as read: each host's table as written, and the rules it gives.
#[derive(Debug, Clone, Default)]
pub struct SiteRules {
    table: toml::Table,
    /// The text last read, to tell an edit from the same file read again.
    text: Option<String>,
}

impl SiteRules {
    /// Reads rules from TOML text. Every host's table must be a valid rule.
    pub fn parse(text: &str) -> Result<Self, String> {
        let table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.to_string().trim().to_owned())?;
        for (host, value) in &table {
            SiteRule::deserialize(value.clone())
                .map_err(|e| format!("[\"{host}\"]: {}", e.to_string().trim()))?;
        }
        Ok(Self {
            table,
            text: Some(text.to_owned()),
        })
    }

    /// Reads the file at `path`; no rules when it does not exist.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match read(path)? {
            Some(text) => Self::parse(&text).map_err(|message| ConfigError::Parse {
                path: path.to_owned(),
                message,
            }),
            None => Ok(Self::default()),
        }
    }

    /// Reads the file again; whether the rules changed. A file that does not parse is an
    /// error and leaves the rules as they were.
    pub fn reload(&mut self, path: &Path) -> Result<bool, ConfigError> {
        let text = read(path)?;
        if text == self.text {
            return Ok(false);
        }
        *self = match &text {
            Some(_) => Self::load(path)?,
            None => Self::default(),
        };
        Ok(true)
    }

    /// The hosts with a rule, in order.
    pub fn hosts(&self) -> Vec<String> {
        let mut hosts: Vec<String> = self.table.keys().cloned().collect();
        hosts.sort();
        hosts
    }

    /// The rule for `host`: its own, else its closest parent domain's. Returns the rule's
    /// host with it.
    pub fn rule_for(&self, host: &str) -> Option<(&str, SiteRule)> {
        let host = site_host(host);
        let mut name = host.as_str();
        loop {
            if let Some((key, value)) = self.table.get_key_value(name) {
                let rule = SiteRule::deserialize(value.clone()).unwrap_or_default();
                return Some((key.as_str(), rule));
            }
            let (_, parent) = name.split_once('.')?;
            // A rule is for a site, never for a bare top-level domain.
            if !parent.contains('.') {
                return None;
            }
            name = parent;
        }
    }

    /// Sets the rule for `host`'s fetch route and site data; its other keys are kept.
    pub fn set(&mut self, host: &str, fetch: Option<SiteFetch>, site_data: SiteData) {
        let entry = self
            .table
            .entry(site_host(host))
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        let Some(rule) = entry.as_table_mut() else {
            return;
        };
        let mut put = |key: &str, value: Option<toml::Value>| match value {
            Some(value) => rule.insert(key.into(), value),
            None => rule.remove(key),
        };
        put("fetch", fetch.and_then(|f| toml::Value::try_from(f).ok()));
        put(
            "site_data",
            (site_data != SiteData::Keep)
                .then(|| toml::Value::try_from(site_data).ok())
                .flatten(),
        );
    }

    /// Drops `host`'s rule.
    pub fn remove(&mut self, host: &str) {
        self.table.remove(&site_host(host));
    }

    /// Writes the rules to `path` atomically, creating its folder.
    pub fn save(&mut self, path: &Path) -> Result<(), ConfigError> {
        const HEADER: &str = "# Aoide's rules for fetching articles, one table per site.\n\
             # Settings › Sites rewrites this file: values and unknown keys are kept, comments\n\
             # are not.\n\n";
        let text = format!("{HEADER}{}", self.table);
        write_atomically(path, &text)?;
        self.text = Some(text);
        Ok(())
    }
}

fn read(path: &Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigError::Io {
            path: path.to_owned(),
            source,
        }),
    }
}

fn write_atomically(path: &Path, text: &str) -> Result<(), ConfigError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| ConfigError::Io { path, source }
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| SITE_RULES_FILE.into());
    let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    std::fs::write(&temp, text).map_err(io(&temp))?;
    std::fs::rename(&temp, path).map_err(|source| {
        let _ = std::fs::remove_file(&temp);
        ConfigError::Io {
            path: path.to_owned(),
            source,
        }
    })
}
