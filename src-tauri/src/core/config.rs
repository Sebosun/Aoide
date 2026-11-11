//! User settings in `~/.config/aoide/settings.toml` and colour themes in
//! `~/.config/aoide/themes/`. Book data (per-book voice overrides, progress, queues) stays in
//! the library database.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aoide_config::{Appearance, SettingsFile, Theme, SETTINGS_FILE, THEMES_DIR};
use aoide_library::Library;
use aoide_tts::EngineProfile;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::extras::AppSettings;
use super::{lock, Core, CoreEvent};

pub(super) const APPEARANCE: &str = "appearance";
pub(super) const PLAYER: &str = "player";
pub(super) const APP: &str = "app";
pub(super) const ENGINE: &str = "engine";

/// Keys under which Aoide kept these settings in the library database before they moved to
/// the config folder. Read once, when `settings.toml` does not exist yet.
const OLD_PROFILE_KEY: &str = "engine.profile";
const OLD_SPEED_KEY: &str = "player.speed";
const OLD_APP_KEY: &str = "app.settings";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlayerSettings {
    pub speed: f32,
    /// Output gain, 0.0..=1.0; files written before volume existed play at full volume.
    #[serde(default = "full_volume")]
    pub volume: f32,
}

fn full_volume() -> f32 {
    1.0
}

/// What the UI needs to apply appearance settings and show config problems.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "Preferences"))]
pub struct PreferencesView {
    pub appearance: Appearance,
    pub themes: Vec<Theme>,
    /// The config folder, for display.
    pub config_dir: String,
    /// Problems with settings.toml (parse or section errors); theme problems are on each theme.
    pub problems: Vec<String>,
}

pub(super) struct Config {
    dir: PathBuf,
    file: SettingsFile,
    /// Set while settings.toml cannot be parsed: values are defaults and nothing is written.
    parse_error: Option<String>,
    /// Text Aoide last wrote or read, to tell its own writes from the user's edits.
    text: Option<String>,
    /// Why a hand-edited `[engine]` section was not applied.
    rejected_engine: Option<String>,
}

impl Config {
    /// Reads `settings.toml` in `dir`. When it does not exist, creates it from the values
    /// Aoide used to keep in `library`; the old database rows are left in place.
    pub(super) fn open(dir: &Path, library: &Library) -> Self {
        let path = dir.join(SETTINGS_FILE);
        match SettingsFile::load(&path) {
            Ok(Some(file)) => Self {
                dir: dir.to_path_buf(),
                text: std::fs::read_to_string(&path).ok(),
                file,
                parse_error: None,
                rejected_engine: None,
            },
            Ok(None) => {
                let mut config = Self {
                    dir: dir.to_path_buf(),
                    file: SettingsFile::empty(&path),
                    parse_error: None,
                    text: None,
                    rejected_engine: None,
                };
                config.migrate(library);
                config
            }
            Err(error) => Self {
                dir: dir.to_path_buf(),
                file: SettingsFile::empty(&path),
                parse_error: Some(error.to_string()),
                text: std::fs::read_to_string(&path).ok(),
                rejected_engine: None,
            },
        }
    }

    fn migrate(&mut self, library: &Library) {
        let old = |key: &str| library.setting(key).ok().flatten();
        let profile = old(OLD_PROFILE_KEY)
            .and_then(|json| serde_json::from_str::<EngineProfile>(&json).ok())
            .unwrap_or_else(super::default_profile);
        let speed = old(OLD_SPEED_KEY)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(1.0);
        let app = old(OLD_APP_KEY)
            .and_then(|json| serde_json::from_str::<AppSettings>(&json).ok())
            .unwrap_or_default();
        let result = self.update(|file| {
            file.set_section(APPEARANCE, &Appearance::default())?;
            file.set_section(PLAYER, &PlayerSettings { speed, volume: 1.0 })?;
            file.set_section(APP, &app)?;
            file.set_section(ENGINE, &profile)
        });
        if let Err(error) = result {
            eprintln!("aoide: cannot write settings: {error}");
        }
    }

    pub(super) fn dir(&self) -> &Path {
        &self.dir
    }

    /// A section, or `None` when it is missing or malformed (see [`Config::problems`]).
    pub(super) fn get<T: DeserializeOwned>(&self, section: &str) -> Option<T> {
        self.file.section(section).ok().flatten()
    }

    /// Changes sections and writes the file. Refused while the file has a syntax error, so
    /// a hand edit in progress is never overwritten.
    pub(super) fn update(
        &mut self,
        change: impl FnOnce(&mut SettingsFile) -> Result<(), aoide_config::ConfigError>,
    ) -> Result<(), String> {
        if let Some(error) = &self.parse_error {
            return Err(format!("Settings were not saved: {error}"));
        }
        let mut next = self.file.clone();
        change(&mut next).map_err(|e| e.to_string())?;
        next.save().map_err(|e| e.to_string())?;
        self.text = Some(next.to_text());
        self.file = next;
        Ok(())
    }

    /// Re-reads the file after a change on disk. Returns false when the text is what Aoide
    /// last wrote or read (nothing to apply).
    pub(super) fn reload(&mut self) -> bool {
        let path = self.file.path().to_path_buf();
        let text = std::fs::read_to_string(&path).ok();
        if text == self.text {
            return false;
        }
        self.text = text;
        match SettingsFile::load(&path) {
            Ok(Some(file)) => {
                self.file = file;
                self.parse_error = None;
            }
            Ok(None) => {
                self.file = SettingsFile::empty(&path);
                self.parse_error = None;
            }
            Err(error) => self.parse_error = Some(error.to_string()),
        }
        true
    }

    pub(super) fn problems(&self) -> Vec<String> {
        if let Some(error) = &self.parse_error {
            return vec![error.clone()];
        }
        let mut problems = Vec::new();
        let mut check = |result: Result<(), aoide_config::ConfigError>| {
            if let Err(error) = result {
                problems.push(error.to_string());
            }
        };
        check(self.file.section::<Appearance>(APPEARANCE).map(drop));
        check(self.file.section::<PlayerSettings>(PLAYER).map(drop));
        check(self.file.section::<AppSettings>(APP).map(drop));
        check(self.file.section::<EngineProfile>(ENGINE).map(drop));
        problems.extend(self.rejected_engine.clone());
        problems
    }

    pub(super) fn preferences(&self) -> PreferencesView {
        PreferencesView {
            appearance: self.get(APPEARANCE).unwrap_or_default(),
            themes: aoide_config::load_themes(&self.dir.join(THEMES_DIR)),
            config_dir: self.dir.to_string_lossy().into_owned(),
            problems: self.problems(),
        }
    }
}

impl Core {
    pub fn preferences(&self) -> PreferencesView {
        lock(&self.config).preferences()
    }

    pub fn save_appearance(&self, appearance: Appearance) -> Result<(), String> {
        lock(&self.config).update(|file| file.set_section(APPEARANCE, &appearance))
    }

    /// Watches the config folder. Hand edits to settings.toml are applied (speed, engine;
    /// app settings are read on use) and every change is sent to the UI as
    /// [`CoreEvent::ConfigChanged`] with the new [`PreferencesView`].
    pub(super) fn watch_config(self: Arc<Self>, runtime: tokio::runtime::Handle) {
        use notify::{RecursiveMode, Watcher};
        let dir = lock(&self.config).dir().to_path_buf();
        if let Err(error) = std::fs::create_dir_all(dir.join(aoide_config::THEMES_DIR)) {
            eprintln!("aoide: cannot create {}: {error}", dir.display());
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(error) => {
                eprintln!("aoide: cannot watch the config folder: {error}");
                return;
            }
        };
        if let Err(error) = watcher.watch(&dir, RecursiveMode::Recursive) {
            eprintln!("aoide: cannot watch the config folder: {error}");
            return;
        }
        while rx.recv().is_ok() {
            // Editors save in several steps (temporary file, rename, chmod).
            while rx.recv_timeout(Duration::from_millis(250)).is_ok() {}
            let changed = lock(&self.config).reload();
            if changed {
                self.apply_config(&runtime);
            }
            self.emit(CoreEvent::ConfigChanged(self.preferences()));
        }
        drop(watcher);
    }

    fn apply_config(&self, runtime: &tokio::runtime::Handle) {
        let (profile, player) = {
            let config = lock(&self.config);
            (
                config.get::<EngineProfile>(ENGINE),
                config.get::<PlayerSettings>(PLAYER),
            )
        };
        if let Some(player) = player {
            let mut core = lock(&self.player);
            core.set_speed(player.speed);
            core.set_volume(player.volume);
        }
        let Some(profile) = profile else { return };
        if profile == lock(&self.engine).profile {
            lock(&self.config).rejected_engine = None;
            return;
        }
        let engine = self.registry.create(&profile).map_err(|e| e.to_string());
        let checked = engine.and_then(|engine| {
            aoide_tts::resolve_params(engine.descriptor(), &profile.params)
                .map(|_| engine)
                .map_err(|e| e.to_string())
        });
        let result = match checked {
            Ok(engine) => runtime.block_on(self.activate_profile(profile, engine)),
            Err(error) => Err(error),
        };
        lock(&self.config).rejected_engine = result
            .err()
            .map(|e| format!("[engine] in settings.toml was not applied: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_config::Mode;

    fn library_with_old_settings() -> Library {
        let library = Library::open_in_memory().unwrap();
        let mut profile = crate::core::default_profile();
        profile.name = "Old profile".into();
        library
            .set_setting(OLD_PROFILE_KEY, &serde_json::to_string(&profile).unwrap())
            .unwrap();
        library.set_setting(OLD_SPEED_KEY, "1.5").unwrap();
        let app = AppSettings {
            cache_limit_bytes: Some(1 << 30),
            library_sort: crate::core::extras::LibrarySort::Title,
            ..AppSettings::default()
        };
        library
            .set_setting(OLD_APP_KEY, &serde_json::to_string(&app).unwrap())
            .unwrap();
        library
    }

    #[test]
    fn first_start_moves_settings_from_the_database_to_the_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::open(dir.path(), &library_with_old_settings());

        let on_disk = SettingsFile::load(dir.path().join(SETTINGS_FILE))
            .unwrap()
            .expect("settings.toml written");
        let profile = on_disk.section::<EngineProfile>(ENGINE).unwrap().unwrap();
        assert_eq!(profile.name, "Old profile");
        assert_eq!(
            on_disk.section::<PlayerSettings>(PLAYER).unwrap(),
            Some(PlayerSettings {
                speed: 1.5,
                volume: 1.0
            })
        );
        let app = on_disk.section::<AppSettings>(APP).unwrap().unwrap();
        assert_eq!(app.cache_limit_bytes, Some(1 << 30));
        assert_eq!(app.library_sort, crate::core::extras::LibrarySort::Title);
        assert_eq!(
            on_disk.section::<Appearance>(APPEARANCE).unwrap(),
            Some(Appearance::default())
        );
        assert!(config.problems().is_empty());
    }

    #[test]
    fn an_existing_file_wins_over_the_database() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            "[player]\nspeed = 2.0\n\n[appearance]\nmode = \"dark\"\n",
        )
        .unwrap();
        let config = Config::open(dir.path(), &library_with_old_settings());
        assert_eq!(
            config.get::<PlayerSettings>(PLAYER),
            Some(PlayerSettings {
                speed: 2.0,
                volume: 1.0
            })
        );
        assert_eq!(config.preferences().appearance.mode, Mode::Dark);
        assert_eq!(config.get::<EngineProfile>(ENGINE), None);
    }

    #[test]
    fn a_broken_file_is_reported_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SETTINGS_FILE);
        std::fs::write(&path, "[player\nspeed = ").unwrap();
        let mut config = Config::open(dir.path(), &library_with_old_settings());

        assert_eq!(config.problems().len(), 1);
        assert!(config.problems()[0].contains("settings.toml"));
        let refused = config.update(|file| {
            file.set_section(
                PLAYER,
                &PlayerSettings {
                    speed: 1.0,
                    volume: 1.0,
                },
            )
        });
        assert!(refused.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[player\nspeed = ");

        std::fs::write(&path, "[player]\nspeed = 1.25\n").unwrap();
        assert!(config.reload());
        assert!(config.problems().is_empty());
        assert_eq!(
            config.get::<PlayerSettings>(PLAYER),
            Some(PlayerSettings {
                speed: 1.25,
                volume: 1.0
            })
        );
        assert!(!config.reload(), "unchanged text is not reloaded again");
    }

    #[test]
    fn aoide_writes_are_not_mistaken_for_user_edits() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::open(dir.path(), &Library::open_in_memory().unwrap());
        config
            .update(|file| {
                file.set_section(
                    PLAYER,
                    &PlayerSettings {
                        speed: 0.75,
                        volume: 1.0,
                    },
                )
            })
            .unwrap();
        assert!(!config.reload());
    }

    #[test]
    fn malformed_sections_are_listed_as_problems() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            "[appearance]\nmode = \"sepia\"\n",
        )
        .unwrap();
        let config = Config::open(dir.path(), &Library::open_in_memory().unwrap());
        let problems = config.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("[appearance]"));
        assert_eq!(config.preferences().appearance, Appearance::default());
    }

    #[test]
    fn preferences_list_themes_from_the_themes_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(THEMES_DIR)).unwrap();
        std::fs::write(
            dir.path().join(THEMES_DIR).join("dusk.toml"),
            "[dark]\naccent = \"#FFB000\"\n",
        )
        .unwrap();
        let config = Config::open(dir.path(), &Library::open_in_memory().unwrap());
        let themes = config.preferences().themes;
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "dusk");
        assert_eq!(themes[0].dark["accent"], "#FFB000");
    }
}
