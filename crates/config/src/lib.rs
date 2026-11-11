//! Aoide's user settings and colour themes, kept as hand-editable TOML files in
//! `$XDG_CONFIG_HOME/aoide` (falling back to `~/.config/aoide`).
//!
//! * `settings.toml` holds named sections (`[appearance]`, `[player]`, `[app]`, `[engine]`).
//!   Section types live in other crates, so access is generic over serde. Writing a section
//!   replaces that section; every other section, including ones Aoide does not know, is kept.
//! * `themes/<name>.toml` holds `[light]` and `[dark]` tables of colour-token overrides.
//!   Aoide only reads theme files; it never writes them.
//!
//! Independent of Tauri, the UI and the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// File name of the settings file inside the config folder.
pub const SETTINGS_FILE: &str = "settings.toml";
/// Folder of theme files inside the config folder.
pub const THEMES_DIR: &str = "themes";
/// Name of the built-in palette; a theme file with this name is ignored.
pub const DEFAULT_THEME: &str = "default";

/// Colour tokens a theme may override, as CSS custom-property names without the `--`.
pub const TOKENS: &[&str] = &[
    "ground",
    "cover-tint",
    "page",
    "ink",
    "reading",
    "dim",
    "faint",
    "line",
    "glass",
    "hero",
    "wash",
    "accent",
    "on-accent",
    "ok",
    "warn",
    "shadow",
];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid TOML. `message` is the parser's description, including the line.
    #[error("{path} is not valid TOML: {message}")]
    Parse { path: PathBuf, message: String },
    /// A section exists but does not match the expected shape.
    #[error("[{section}] in {path}: {message}")]
    Section {
        path: PathBuf,
        section: String,
        message: String,
    },
    /// A value could not be represented in TOML.
    #[error("cannot write [{section}]: {message}")]
    Serialize { section: String, message: String },
}

/// The config folder: `$XDG_CONFIG_HOME/aoide` when that variable is set to an absolute
/// path, otherwise `<home>/.config/aoide`.
pub fn config_dir(xdg_config_home: Option<&std::ffi::OsStr>, home: &Path) -> PathBuf {
    match xdg_config_home.map(Path::new) {
        Some(base) if base.is_absolute() => base.join("aoide"),
        _ => home.join(".config").join("aoide"),
    }
}

/// `settings.toml`, loaded as a table of sections.
#[derive(Debug, Clone)]
pub struct SettingsFile {
    path: PathBuf,
    table: toml::Table,
}

impl SettingsFile {
    /// An empty settings file at `path` (nothing is written until [`SettingsFile::save`]).
    pub fn empty(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            table: toml::Table::new(),
        }
    }

    /// Reads the file. `Ok(None)` when it does not exist; `Err(Parse)` when it is not TOML.
    pub fn load(path: impl Into<PathBuf>) -> Result<Option<Self>, ConfigError> {
        let path = path.into();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(ConfigError::Io { path, source }),
        };
        match text.parse::<toml::Table>() {
            Ok(table) => Ok(Some(Self { path, table })),
            Err(e) => Err(ConfigError::Parse {
                path,
                message: e.to_string().trim().to_string(),
            }),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the section exists.
    pub fn has(&self, section: &str) -> bool {
        self.table.contains_key(section)
    }

    /// Decodes a section. `Ok(None)` when it is absent; `Err(Section)` when it has the wrong
    /// shape. Missing keys inside a section fall back to the type's serde defaults.
    pub fn section<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, ConfigError> {
        let Some(value) = self.table.get(name) else {
            return Ok(None);
        };
        T::deserialize(value.clone())
            .map(Some)
            .map_err(|e| ConfigError::Section {
                path: self.path.clone(),
                section: name.to_string(),
                message: e.to_string().trim().to_string(),
            })
    }

    /// Replaces one section entirely, leaving every other section untouched. `None` fields
    /// are left out of the file.
    pub fn set_section<T: Serialize>(&mut self, name: &str, value: &T) -> Result<(), ConfigError> {
        let value = toml::Value::try_from(value).map_err(|e| ConfigError::Serialize {
            section: name.to_string(),
            message: e.to_string(),
        })?;
        self.table.insert(name.to_string(), value);
        Ok(())
    }

    /// The TOML text [`SettingsFile::save`] writes: a short comment header, then the sections.
    pub fn to_text(&self) -> String {
        const HEADER: &str =
            "# Aoide settings. Aoide rewrites this file when you change settings in the app:\n\
                              # values and unknown sections are kept, comments are not.\n\
                              # Colour themes go in themes/<name>.toml next to this file.\n\n";
        format!("{HEADER}{}", self.table)
    }

    /// Writes the file atomically (temporary file in the same folder, then rename), creating
    /// the folder when needed.
    pub fn save(&self) -> Result<(), ConfigError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| ConfigError::Io { path, source }
        };
        let dir = self.path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir).map_err(io(dir))?;
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| SETTINGS_FILE.into());
        let temp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
        std::fs::write(&temp, self.to_text()).map_err(io(&temp))?;
        std::fs::rename(&temp, &self.path).map_err(|source| {
            let _ = std::fs::remove_file(&temp);
            ConfigError::Io {
                path: self.path.clone(),
                source,
            }
        })
    }
}

/// Light, dark, or follow the system.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(rename = "AppearanceMode"))]
pub enum Mode {
    #[default]
    Auto,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ReaderMode {
    #[default]
    Page,
    Lines,
}

/// The `[appearance]` section.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Appearance {
    pub mode: Mode,
    /// A theme file name without `.toml`, or [`DEFAULT_THEME`].
    pub theme: String,
    /// Reading text size step, 0 (smallest) to 4; 2 is the designed size.
    pub text_size: u8,
    pub reader_mode: ReaderMode,
    /// Interface zoom (Ctrl + / Ctrl −), 1.0 is 100%. The UI keeps it between 0.67 and 1.5.
    pub zoom: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            theme: DEFAULT_THEME.into(),
            text_size: 2,
            reader_mode: ReaderMode::Page,
            zoom: 1.0,
        }
    }
}

/// One theme file. Invalid entries are left out of `light`/`dark` and described in `problems`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Theme {
    /// File name without `.toml`.
    pub name: String,
    /// Token name (see [`TOKENS`]) to CSS colour.
    pub light: BTreeMap<String, String>,
    pub dark: BTreeMap<String, String>,
    pub problems: Vec<String>,
}

/// Reads every `*.toml` file in `dir`, sorted by name. A missing folder gives no themes; a
/// file that cannot be read or parsed gives a theme with no tokens and one problem. A file
/// named [`DEFAULT_THEME`] is skipped.
pub fn load_themes(dir: &Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "toml") && p.is_file())
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().into_owned();
            (name != DEFAULT_THEME).then(|| load_theme(name, path))
        })
        .collect()
}

fn load_theme(name: String, path: &Path) -> Theme {
    let mut theme = Theme {
        name,
        light: BTreeMap::new(),
        dark: BTreeMap::new(),
        problems: Vec::new(),
    };
    let table = match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| text.parse::<toml::Table>().map_err(|e| e.to_string()))
    {
        Ok(table) => table,
        Err(message) => {
            theme.problems.push(format!(
                "{} could not be read: {}",
                path.display(),
                message.trim()
            ));
            return theme;
        }
    };
    for (variant, tokens) in [("light", &mut theme.light), ("dark", &mut theme.dark)] {
        match table.get(variant) {
            None => {}
            Some(toml::Value::Table(entries)) => {
                for (key, value) in entries {
                    match value.as_str() {
                        _ if !TOKENS.contains(&key.as_str()) => theme
                            .problems
                            .push(format!("[{variant}] {key}: not a colour token Aoide knows")),
                        Some(colour) if is_colour(colour) => {
                            tokens.insert(key.clone(), colour.to_string());
                        }
                        Some(colour) => theme.problems.push(format!(
                            "[{variant}] {key}: \"{colour}\" is not a supported colour"
                        )),
                        None => theme
                            .problems
                            .push(format!("[{variant}] {key}: expected a colour string")),
                    }
                }
            }
            Some(_) => theme
                .problems
                .push(format!("[{variant}] should be a table of colours")),
        }
    }
    theme
}

/// Whether `value` is a CSS colour Aoide accepts in a theme: `#rgb`, `#rgba`, `#rrggbb`,
/// `#rrggbbaa`, or `rgb()`, `rgba()`, `hsl()`, `hsla()`, `oklch()` with only numbers,
/// `%`, `deg`, `.`, `,`, `/`, `-` and spaces inside the parentheses. Anything else (named
/// colours, `var()`, `;`, `}`) is rejected, so theme values cannot inject CSS.
pub fn is_colour(value: &str) -> bool {
    if let Some(hex) = value.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit());
    }
    let Some((function, rest)) = value.split_once('(') else {
        return false;
    };
    let Some(inner) = rest.strip_suffix(')') else {
        return false;
    };
    let known = ["rgb", "rgba", "hsl", "hsla", "oklch"];
    if !known.iter().any(|k| k.eq_ignore_ascii_case(function)) {
        return false;
    }
    let without_deg = inner.replace("deg", "");
    inner.bytes().any(|b| b.is_ascii_digit())
        && without_deg
            .bytes()
            .all(|b| b.is_ascii_digit() || b" %.,/-".contains(&b))
}
