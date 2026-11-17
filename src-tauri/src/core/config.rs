//! User settings in `~/.config/aoide/settings.toml` and colour themes in
//! `~/.config/aoide/themes/`. Book data (per-book voice overrides, progress, queues) stays in
//! the library database.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use aoide_config::{Appearance, SettingsFile, Theme, SETTINGS_FILE, THEMES_DIR};
use aoide_library::{GlobalKey, Library};
use aoide_tts::EngineProfile;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::settings::{changed_keys, AppSettings, AppSettingsPatch};
use super::{lock, Core, CoreError, CoreEvent};

pub(super) const APPEARANCE: &str = "appearance";
pub(super) const PLAYER: &str = "player";
pub(super) const APP: &str = "app";
pub(super) const ENGINE: &str = "engine";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PlayerSettings {
    #[serde(serialize_with = "two_decimals")]
    pub speed: f32,
    /// Output gain, 0.0..=1.0; files written before volume existed play at full volume.
    #[serde(default = "full_volume")]
    pub volume: f32,
}

fn full_volume() -> f32 {
    1.0
}

/// A speed written as it is kept, to two decimals (`1.1`, not `1.100000023841858`).
fn two_decimals<S: serde::Serializer>(speed: &f32, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64((f64::from(*speed) * 100.0).round() / 100.0)
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
    /// The library folder, which cannot be the inbox.
    library_dir: PathBuf,
    file: SettingsFile,
    /// Set while settings.toml cannot be parsed: values are defaults and nothing is written.
    parse_error: Option<String>,
    /// Text Aoide last wrote or read, to tell its own writes from the user's edits.
    text: Option<String>,
    /// Why a hand-edited `[engine]` section was not applied.
    rejected_engine: Option<String>,
    /// The app settings in use: the `[app]` section as last checked. A hand edit that fails
    /// the checks leaves them as they were (and says why in `rejected_app`).
    app: AppSettings,
    /// Why a hand-edited `[app]` section was not applied.
    rejected_app: Option<String>,
}

impl Config {
    /// Reads `settings.toml` in `dir`. When it does not exist, creates it from the values
    /// Aoide used to keep in `library`; the old database rows are left in place.
    pub(super) fn open(dir: &Path, library_dir: &Path, library: &Library) -> Self {
        let path = dir.join(SETTINGS_FILE);
        match SettingsFile::load(&path) {
            Ok(Some(file)) => {
                let mut config = Self {
                    dir: dir.to_path_buf(),
                    library_dir: library_dir.to_path_buf(),
                    text: std::fs::read_to_string(&path).ok(),
                    file,
                    parse_error: None,
                    rejected_engine: None,
                    app: AppSettings::default(),
                    rejected_app: None,
                };
                config.adopt_app();
                config
            }
            Ok(None) => {
                let mut config = Self {
                    dir: dir.to_path_buf(),
                    library_dir: library_dir.to_path_buf(),
                    file: SettingsFile::empty(&path),
                    parse_error: None,
                    text: None,
                    rejected_engine: None,
                    app: AppSettings::default(),
                    rejected_app: None,
                };
                config.migrate(library);
                config.adopt_app();
                config
            }
            Err(error) => Self {
                dir: dir.to_path_buf(),
                library_dir: library_dir.to_path_buf(),
                file: SettingsFile::empty(&path),
                parse_error: Some(error.to_string()),
                text: std::fs::read_to_string(&path).ok(),
                rejected_engine: None,
                app: AppSettings::default(),
                rejected_app: None,
            },
        }
    }

    /// Takes the `[app]` section as the settings in use when it passes [`check_app`]. Returns
    /// the settings before and after when they changed; a section that fails the checks is
    /// not taken, and the reason is listed among the problems.
    fn adopt_app(&mut self) -> Option<(AppSettings, AppSettings)> {
        let found = self.get::<AppSettings>(APP).unwrap_or_default();
        match check_app(&found, &self.library_dir) {
            Ok(()) => {
                self.rejected_app = None;
                (found != self.app)
                    .then(|| (std::mem::replace(&mut self.app, found.clone()), found))
            }
            Err(error) => {
                tracing::warn!(event = "config.rejected", section = "app", %error, "Hand-edited app settings not applied");
                self.rejected_app =
                    Some(format!("[app] in settings.toml was not applied: {error}"));
                None
            }
        }
    }

    fn migrate(&mut self, library: &Library) {
        let profile = library
            .value::<EngineProfile>(GlobalKey::OldEngineProfile)
            .ok()
            .flatten()
            .unwrap_or_else(super::default_profile);
        let speed = library
            .value::<f32>(GlobalKey::OldPlayerSpeed)
            .ok()
            .flatten()
            .unwrap_or(1.0);
        let app = library
            .value::<AppSettings>(GlobalKey::OldAppSettings)
            .ok()
            .flatten()
            .unwrap_or_default();
        let result = self.update(|file| {
            file.set_section(APPEARANCE, &Appearance::default())?;
            file.set_section(PLAYER, &PlayerSettings { speed, volume: 1.0 })?;
            file.set_section(APP, &app)?;
            file.set_section(ENGINE, &profile)
        });
        if let Err(error) = result {
            tracing::error!(event = "config.save", section = "engine", outcome = "failed", %error, "Cannot save the engine settings");
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
    ) -> Result<(), super::CoreError> {
        if let Some(error) = &self.parse_error {
            return Err(super::CoreError::invalid(
                "settings",
                format!("Settings were not saved: {error}"),
            ));
        }
        let mut next = self.file.clone();
        change(&mut next)?;
        next.save()?;
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
        problems.extend(self.rejected_app.clone());
        problems
    }

    pub(super) fn preferences(&self) -> PreferencesView {
        PreferencesView {
            appearance: self.get(APPEARANCE).unwrap_or_default(),
            themes: aoide_config::themes(&self.dir.join(THEMES_DIR)),
            config_dir: self.dir.to_string_lossy().into_owned(),
            problems: self.problems(),
        }
    }
}

/// Checks app settings before they are used, wherever they came from (`library_dir` is the
/// library folder).
fn check_app(settings: &AppSettings, library_dir: &Path) -> Result<(), String> {
    if let Some(inbox) = &settings.inbox {
        super::inbox::check_folder(inbox, library_dir)?;
    }
    settings.recognition.validate()?;
    if !super::settings::READING_WPM.contains(&settings.reading_wpm) {
        return Err(format!(
            "Reading speed must be {} to {} words a minute",
            super::settings::READING_WPM.start(),
            super::settings::READING_WPM.end()
        ));
    }
    if !super::feeds::CHECK_MINUTES.contains(&settings.feeds.check_minutes) {
        return Err(format!(
            "Feeds are checked every {} to {} minutes",
            super::feeds::CHECK_MINUTES.start(),
            super::feeds::CHECK_MINUTES.end()
        ));
    }
    if !super::browsing::IDLE_SECONDS.contains(&settings.browser.idle_seconds) {
        return Err(format!(
            "The browser is shut down after {} to {} idle seconds",
            super::browsing::IDLE_SECONDS.start(),
            super::browsing::IDLE_SECONDS.end()
        ));
    }
    let pauses = &settings.pauses;
    let range = super::settings::PAUSE_MS;
    if [pauses.paragraph_ms, pauses.break_ms, pauses.heading_ms]
        .iter()
        .any(|ms| !range.contains(ms))
    {
        return Err(format!(
            "Pauses are {} to {} milliseconds long",
            range.start(),
            range.end()
        ));
    }
    if let Some(aligner) = &settings.aligner {
        aoide_aligner::Aligner::new(aoide_aligner::Options {
            base_url: aligner.base_url.clone(),
            network: aligner.network,
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// What the settings ask of the rest of the app.
pub(super) trait ConfigHost {
    /// settings.toml was edited by hand: play at `player`'s speed and volume and switch to
    /// `engine`. An `Err` says why the engine was not switched.
    fn settings_edited(
        &self,
        player: Option<PlayerSettings>,
        engine: Option<EngineProfile>,
        runtime: &tokio::runtime::Handle,
    ) -> Result<(), String>;
    /// The app settings changed from `before` to `after`, from the Settings window or a hand
    /// edit that passed the checks: do what the change asks for.
    fn app_changed(&self, before: &AppSettings, after: &AppSettings);
    /// The window shows `preferences` from now on.
    fn preferences_changed(&self, preferences: PreferencesView);
    /// Something in the config folder changed (the site rules may have).
    fn folder_changed(&self);
}

/// The settings file, shared by everything that reads or saves a section.
pub(super) struct Settings {
    config: Mutex<Config>,
}

impl Settings {
    pub fn new(config: Config) -> Self {
        Self {
            config: Mutex::new(config),
        }
    }

    /// A section, or `None` when it is missing or malformed.
    pub fn get<T: DeserializeOwned>(&self, section: &str) -> Option<T> {
        lock(&self.config).get(section)
    }

    /// Writes one section.
    pub fn save<T: Serialize>(&self, section: &str, value: &T) -> Result<(), CoreError> {
        lock(&self.config).update(|file| file.set_section(section, value))
    }

    /// The app settings in use (as last saved or hand-edited, once they passed the checks).
    pub fn app(&self) -> AppSettings {
        lock(&self.config).app.clone()
    }

    /// Merges `patch` into the app settings, checks and saves them, and tells `host` what
    /// changed. Returns the settings now in use; an invalid change saves nothing.
    pub fn update_app(
        &self,
        patch: AppSettingsPatch,
        host: &dyn ConfigHost,
    ) -> Result<AppSettings, String> {
        // Read, merge and write under one lock, so concurrent patches both land.
        let mut config = lock(&self.config);
        let before = config.app.clone();
        let settings = patch.apply(before.clone());
        check_app(&settings, &config.library_dir)?;
        config.update(|file| file.set_section(APP, &settings))?;
        config.app = settings.clone();
        config.rejected_app = None;
        drop(config);
        tracing::info!(event = "config.saved", section = "app", changed = %changed_keys(&before, &settings), "Saved the app settings");
        if before != settings {
            host.app_changed(&before, &settings);
        }
        Ok(settings)
    }

    /// Applies settings.toml as it is now on disk, after a change there: the player and
    /// engine sections, and the app settings when they pass the checks (each change told to
    /// `host`); then sends the new [`PreferencesView`].
    pub fn apply_edits(&self, host: &dyn ConfigHost, runtime: &tokio::runtime::Handle) {
        let (changed, app) = {
            let mut config = lock(&self.config);
            let changed = config.reload();
            (changed, if changed { config.adopt_app() } else { None })
        };
        if changed {
            let (player, engine) = (self.get(PLAYER), self.get(ENGINE));
            let applied = host.settings_edited(player, engine, runtime);
            lock(&self.config).rejected_engine = applied
                .err()
                .map(|e| format!("[engine] in settings.toml was not applied: {e}"));
            if let Some((before, after)) = app {
                tracing::info!(event = "config.edited", section = "app", changed = %changed_keys(&before, &after), "Applied hand-edited app settings");
                host.app_changed(&before, &after);
            }
        }
        host.preferences_changed(self.preferences());
        host.folder_changed();
    }

    pub fn preferences(&self) -> PreferencesView {
        lock(&self.config).preferences()
    }

    /// Watches the config folder and applies each change through [`Settings::apply_edits`]
    /// (reading the settings is not a change). Returns only if the folder cannot be watched.
    pub fn watch(&self, host: &dyn ConfigHost, runtime: &tokio::runtime::Handle) {
        let dir = lock(&self.config).dir().to_path_buf();
        if let Err(error) = std::fs::create_dir_all(dir.join(aoide_config::THEMES_DIR)) {
            tracing::warn!(event = "config.watch", %error, "Cannot create the themes folder");
            return;
        }
        // Editors save in several steps (temporary file, rename, chmod).
        let watched = super::watching::watch_folder(&dir, Duration::from_millis(250), || {
            self.apply_edits(host, runtime);
            std::ops::ControlFlow::Continue(())
        });
        if let Err(error) = watched {
            tracing::warn!(event = "config.watch", %error, "Cannot watch the settings folder");
        }
    }
}

impl Core {
    pub fn preferences(&self) -> PreferencesView {
        self.settings.preferences()
    }

    pub fn save_appearance(&self, appearance: Appearance) -> Result<(), CoreError> {
        self.settings.save(APPEARANCE, &appearance)
    }
}

impl ConfigHost for Core {
    fn settings_edited(
        &self,
        player: Option<PlayerSettings>,
        engine: Option<EngineProfile>,
        runtime: &tokio::runtime::Handle,
    ) -> Result<(), String> {
        if let Some(player) = player {
            self.playback.set_usual_speed(player.speed);
            self.playback.set_volume(player.volume);
        }
        match engine {
            Some(profile) if profile != self.engine.profile() => {
                let engine = self.engine.create(&profile)?;
                runtime.block_on(self.activate_profile(profile, engine))
            }
            _ => Ok(()),
        }
    }

    fn app_changed(&self, before: &AppSettings, after: &AppSettings) {
        if before.recognition != after.recognition {
            // Another model size or a recognition server may be what a waiting sync needs.
            self.models_changed();
        }
        if after.verify_audiobooks && !before.verify_audiobooks {
            self.verify_found_audiobooks();
        }
        if before.cache_limit_bytes != after.cache_limit_bytes {
            self.storage.limit_changed();
            self.enforce_cache_limit();
        }
        if before.feeds != after.feeds {
            self.feeds.settings_changed();
        }
        if before.pauses != after.pauses {
            self.text_laid_out_again();
        }
        if before.inbox != after.inbox {
            self.inbox.set_folder(after.inbox.clone());
        }
        if before.search_text && !after.search_text {
            if let Err(error) = self.indexing.turned_off() {
                tracing::warn!(event = "index.cleared", outcome = "failed", %error, "Could not drop the library's text index");
            }
        } else if after.search_text && !before.search_text {
            self.indexing.poke();
        }
    }

    fn preferences_changed(&self, preferences: PreferencesView) {
        self.emit(CoreEvent::ConfigChanged(preferences));
    }

    fn folder_changed(&self) {
        self.browsing.reload_rules();
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
            .set_value(GlobalKey::OldEngineProfile, &profile)
            .unwrap();
        library
            .set_value(GlobalKey::OldPlayerSpeed, &1.5f32)
            .unwrap();
        let app = AppSettings {
            cache_limit_bytes: Some(1 << 30),
            library_sort: crate::core::settings::LibrarySort::Title,
            ..AppSettings::default()
        };
        library.set_value(GlobalKey::OldAppSettings, &app).unwrap();
        library
    }

    #[test]
    fn first_start_moves_settings_from_the_database_to_the_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &library_with_old_settings(),
        );

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
        assert_eq!(app.library_sort, crate::core::settings::LibrarySort::Title);
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
        let config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &library_with_old_settings(),
        );
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
        let mut config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &library_with_old_settings(),
        );

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
        let mut config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        );
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
        let config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        );
        let problems = config.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("[appearance]"));
        assert_eq!(config.preferences().appearance, Appearance::default());
    }

    #[test]
    fn preferences_list_themes_from_the_themes_folder_after_those_that_come_with_aoide() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(THEMES_DIR)).unwrap();
        std::fs::write(
            dir.path().join(THEMES_DIR).join("dusk.toml"),
            "[dark]\naccent = \"#FFB000\"\n",
        )
        .unwrap();
        let config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        );
        let themes = config.preferences().themes;
        let dusk = themes.last().unwrap();
        assert_eq!((dusk.name.as_str(), dusk.builtin), ("dusk", false));
        assert_eq!(dusk.dark["accent"], "#FFB000");
        assert!(themes[..themes.len() - 1].iter().all(|t| t.builtin));
    }

    /// Records what the settings told it.
    #[derive(Default)]
    struct Host {
        app_changes: Mutex<Vec<(AppSettings, AppSettings)>>,
        edited: Mutex<Vec<Option<PlayerSettings>>>,
    }

    impl ConfigHost for Host {
        fn settings_edited(
            &self,
            player: Option<PlayerSettings>,
            _engine: Option<EngineProfile>,
            _runtime: &tokio::runtime::Handle,
        ) -> Result<(), String> {
            lock(&self.edited).push(player);
            Ok(())
        }
        fn app_changed(&self, before: &AppSettings, after: &AppSettings) {
            lock(&self.app_changes).push((before.clone(), after.clone()));
        }
        fn preferences_changed(&self, _preferences: PreferencesView) {}
        fn folder_changed(&self) {}
    }

    fn patch(json: &str) -> AppSettingsPatch {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn patches_to_different_app_settings_both_land_and_a_bad_one_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::new(Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        ));
        let host = Host::default();
        settings
            .update_app(patch(r#"{"cacheLimitBytes":1000}"#), &host)
            .unwrap();
        settings
            .update_app(patch(r#"{"verifyAudiobooks":true}"#), &host)
            .unwrap();
        let refused = settings.update_app(patch(r#"{"aligner":{"baseUrl":"not a url"}}"#), &host);

        assert!(refused.is_err());
        let saved = Settings::new(Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        ))
        .app();
        assert_eq!(
            (saved.cache_limit_bytes, saved.verify_audiobooks),
            (Some(1000), true)
        );
        assert_eq!(saved.aligner, None);
        assert_eq!(
            lock(&host.app_changes).len(),
            2,
            "the refused change told nothing"
        );
    }

    fn edit(dir: &Path, text: &str) {
        std::fs::write(dir.join(SETTINGS_FILE), text).unwrap();
    }

    #[test]
    fn a_hand_edit_to_the_app_settings_is_checked_and_told_like_a_change_from_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::new(Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        ));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let host = Host::default();

        edit(dir.path(), "[app]\ncacheLimitBytes = 5000\n");
        settings.apply_edits(&host, runtime.handle());

        assert_eq!(settings.app().cache_limit_bytes, Some(5000));
        let changes = lock(&host.app_changes).clone();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].0.cache_limit_bytes, None);
        assert_eq!(changes[0].1.cache_limit_bytes, Some(5000));
    }

    #[test]
    fn a_hand_edit_that_fails_the_checks_keeps_the_settings_in_use_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings::new(Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        ));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let host = Host::default();
        edit(dir.path(), "[app]\nreadingWpm = 300\n");
        settings.apply_edits(&host, runtime.handle());

        edit(
            dir.path(),
            "[app]\nreadingWpm = 300\n\n[app.feeds]\ncheckMinutes = 0\n",
        );
        settings.apply_edits(&host, runtime.handle());

        assert_eq!(settings.app().reading_wpm, 300);
        assert_eq!(
            settings.app().feeds,
            AppSettings::default().feeds,
            "the refused feed interval is not used"
        );
        assert_eq!(
            lock(&host.app_changes).len(),
            1,
            "only the first edit was told"
        );
        let problems = settings.preferences().problems;
        assert!(
            problems
                .iter()
                .any(|p| p.contains("[app]") && p.contains("Feeds")),
            "{problems:?}"
        );

        edit(dir.path(), "[app]\nreadingWpm = 320\n");
        settings.apply_edits(&host, runtime.handle());
        assert_eq!(settings.app().reading_wpm, 320);
        assert!(settings.preferences().problems.is_empty());
    }

    #[test]
    fn app_settings_that_fail_the_checks_at_start_are_not_used() {
        let dir = tempfile::tempdir().unwrap();
        edit(
            dir.path(),
            "[app]\nreadingWpm = 5\nmetadataLookup = false\n",
        );
        let config = Config::open(
            dir.path(),
            &dir.path().join("Books"),
            &Library::open_in_memory().unwrap(),
        );
        assert_eq!(config.app, AppSettings::default());
        assert_eq!(config.problems().len(), 1, "{:?}", config.problems());
    }

    #[test]
    fn an_inbox_inside_the_library_folder_written_by_hand_is_not_used() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("Books");
        std::fs::create_dir_all(library.join("Incoming")).unwrap();
        edit(
            dir.path(),
            &format!(
                "[app]\ninbox = {:?}\nmetadataLookup = false\n",
                library.join("Incoming").to_string_lossy()
            ),
        );
        let config = Config::open(dir.path(), &library, &Library::open_in_memory().unwrap());
        assert_eq!(config.app, AppSettings::default());
        let problems = config.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("library folder"), "{problems:?}");
    }
}
