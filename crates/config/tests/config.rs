use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;

use aoide_config::{
    config_dir, is_colour, load_themes, Appearance, ConfigError, Mode, ReaderMode, SettingsFile,
    SETTINGS_FILE,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Engine {
    name: String,
    base_url: String,
    model: Option<String>,
    #[serde(default)]
    params: BTreeMap<String, Param>,
    launch: Option<Launch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Param {
    Bool(bool),
    Number(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Launch {
    program: String,
    args: Vec<String>,
    working_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct App {
    metadata_lookup: bool,
    cache_limit_bytes: Option<u64>,
    library_sort: String,
}

impl Default for App {
    fn default() -> Self {
        Self {
            metadata_lookup: true,
            cache_limit_bytes: None,
            library_sort: "recent".into(),
        }
    }
}

fn engine() -> Engine {
    Engine {
        name: "Local Chatterbox".into(),
        base_url: "http://127.0.0.1:8004".into(),
        model: None,
        params: BTreeMap::from([
            ("temperature".into(), Param::Number(0.8)),
            ("cfg_weight".into(), Param::Number(0.5)),
            ("split".into(), Param::Bool(true)),
            ("format".into(), Param::Text("wav".into())),
        ]),
        launch: Some(Launch {
            program: "/opt/venv/bin/python".into(),
            args: vec!["server.py".into()],
            working_dir: None,
        }),
    }
}

#[test]
fn config_dir_prefers_absolute_xdg_config_home() {
    let home = Path::new("/home/reader");
    assert_eq!(
        config_dir(Some(OsStr::new("/srv/conf")), home),
        Path::new("/srv/conf/aoide")
    );
    assert_eq!(
        config_dir(None, home),
        Path::new("/home/reader/.config/aoide")
    );
    assert_eq!(
        config_dir(Some(OsStr::new("")), home),
        Path::new("/home/reader/.config/aoide")
    );
    assert_eq!(
        config_dir(Some(OsStr::new("relative/conf")), home),
        Path::new("/home/reader/.config/aoide")
    );
}

#[test]
fn missing_file_loads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    assert!(SettingsFile::load(dir.path().join(SETTINGS_FILE))
        .unwrap()
        .is_none());
}

#[test]
fn sections_round_trip_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join(SETTINGS_FILE);
    let mut file = SettingsFile::empty(&path);
    let appearance = Appearance {
        mode: Mode::Dark,
        theme: "nord".into(),
        text_size: 3,
        reader_mode: ReaderMode::Lines,
        zoom: 1.1,
    };
    file.set_section("appearance", &appearance).unwrap();
    file.set_section("engine", &engine()).unwrap();
    file.set_section(
        "app",
        &App {
            cache_limit_bytes: Some(5 << 30),
            ..App::default()
        },
    )
    .unwrap();
    file.save().unwrap();

    let loaded = SettingsFile::load(&path).unwrap().unwrap();
    assert_eq!(
        loaded.section::<Appearance>("appearance").unwrap(),
        Some(appearance)
    );
    assert_eq!(loaded.section::<Engine>("engine").unwrap(), Some(engine()));
    assert_eq!(
        loaded
            .section::<App>("app")
            .unwrap()
            .unwrap()
            .cache_limit_bytes,
        Some(5 << 30)
    );
    assert!(loaded.has("engine"));
    assert!(!loaded.has("player"));
    assert_eq!(loaded.section::<App>("player").unwrap(), None);
}

#[test]
fn written_file_is_readable_toml_with_a_header_and_no_nulls() {
    let mut file = SettingsFile::empty("/unused/settings.toml");
    file.set_section("engine", &engine()).unwrap();
    let text = file.to_text();
    assert!(
        text.starts_with('#'),
        "starts with a comment header:\n{text}"
    );
    assert!(text.contains("[engine]"));
    assert!(text.contains("baseUrl = \"http://127.0.0.1:8004\""));
    assert!(
        text.contains("cfg_weight = 0.5"),
        "param keys are kept as written:\n{text}"
    );
    assert!(!text.contains("model"), "None fields are left out:\n{text}");
    assert!(!text.contains("workingDir"));
}

#[test]
fn hand_written_file_with_missing_keys_uses_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    std::fs::write(
        &path,
        "# my settings\n[appearance]\nmode = \"light\"\n\n[app]\nlibrarySort = \"title\"\n",
    )
    .unwrap();
    let file = SettingsFile::load(&path).unwrap().unwrap();
    assert_eq!(
        file.section::<Appearance>("appearance").unwrap(),
        Some(Appearance {
            mode: Mode::Light,
            ..Appearance::default()
        })
    );
    assert_eq!(
        file.section::<App>("app").unwrap(),
        Some(App {
            library_sort: "title".into(),
            ..App::default()
        })
    );
}

#[test]
fn interface_zoom_is_read_from_appearance_and_defaults_to_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    std::fs::write(&path, "[appearance]\nzoom = 1.25\n").unwrap();
    let file = SettingsFile::load(&path).unwrap().unwrap();
    let appearance = file.section::<Appearance>("appearance").unwrap().unwrap();
    assert_eq!(appearance.zoom, 1.25);
    assert_eq!(Appearance::default().zoom, 1.0);
}

#[test]
fn saving_keeps_sections_and_keys_aoide_does_not_know() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    std::fs::write(
        &path,
        "[future]\nflag = true\n\n[appearance]\nmode = \"dark\"\nsparkles = 3\n",
    )
    .unwrap();
    let mut file = SettingsFile::load(&path).unwrap().unwrap();
    file.set_section("player", &BTreeMap::from([("speed", 1.25)]))
        .unwrap();
    file.save().unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("[future]") && text.contains("flag = true"),
        "{text}"
    );
    assert!(text.contains("sparkles = 3"), "{text}");
    assert!(text.contains("speed = 1.25"), "{text}");
}

#[test]
fn invalid_toml_is_a_parse_error_naming_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    std::fs::write(&path, "[appearance\nmode = ").unwrap();
    match SettingsFile::load(&path) {
        Err(error @ ConfigError::Parse { .. }) => {
            assert!(error.to_string().contains("settings.toml"), "{error}")
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[test]
fn wrongly_shaped_section_is_a_section_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    std::fs::write(&path, "[appearance]\nmode = \"purple\"\n").unwrap();
    let file = SettingsFile::load(&path).unwrap().unwrap();
    match file.section::<Appearance>("appearance") {
        Err(error @ ConfigError::Section { .. }) => {
            assert!(error.to_string().contains("[appearance]"), "{error}")
        }
        other => panic!("expected a section error, got {other:?}"),
    }
}

#[test]
fn save_leaves_no_temporary_files_behind() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(SETTINGS_FILE);
    let mut file = SettingsFile::empty(&path);
    file.set_section("appearance", &Appearance::default())
        .unwrap();
    file.save().unwrap();
    file.save().unwrap();
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, vec![SETTINGS_FILE.to_string()]);
}

#[test]
fn colours_are_validated_strictly() {
    for ok in [
        "#fff",
        "#FFFA",
        "#E0502F",
        "#e0502f80",
        "rgba(255,255,255,.72)",
        "rgb(10 20 30 / 50%)",
        "hsl(210deg 40% 20%)",
        "oklch(0.7 0.1 30)",
    ] {
        assert!(is_colour(ok), "{ok} should be accepted");
    }
    for bad in [
        "",
        "red",
        "#ggg",
        "#12345",
        "var(--accent)",
        "#fff; background: url(x)",
        "rgb(1,2,3)}",
        "rgb(1,2,3",
        "url(https://example.com)",
        "expression(1)",
    ] {
        assert!(!is_colour(bad), "{bad:?} should be rejected");
    }
}

#[test]
fn themes_load_valid_tokens_and_report_problems() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("nord.toml"),
        "[light]\naccent = \"#5E81AC\"\nground = \"#ECEFF4\"\nsparkle = \"#fff\"\n\n[dark]\naccent = \"red\"\nground = \"#2E3440\"\nline = 3\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("broken.toml"), "[light\n").unwrap();
    std::fs::write(
        dir.path().join("default.toml"),
        "[light]\naccent = \"#000\"\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not a theme").unwrap();

    let themes = load_themes(dir.path());
    let names: Vec<_> = themes.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, vec!["broken", "nord"]);

    let broken = &themes[0];
    assert!(broken.light.is_empty() && broken.dark.is_empty());
    assert_eq!(broken.problems.len(), 1);

    let nord = &themes[1];
    assert_eq!(
        nord.light,
        BTreeMap::from([
            ("accent".to_string(), "#5E81AC".to_string()),
            ("ground".to_string(), "#ECEFF4".to_string()),
        ])
    );
    assert_eq!(
        nord.dark,
        BTreeMap::from([("ground".to_string(), "#2E3440".to_string())])
    );
    assert_eq!(nord.problems.len(), 3, "{:?}", nord.problems);
    let problems = nord.problems.join("\n");
    assert!(problems.contains("sparkle"), "{problems}");
    assert!(problems.contains("red"), "{problems}");
    assert!(problems.contains("line"), "{problems}");
}

#[test]
fn missing_theme_folder_has_no_themes() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load_themes(&dir.path().join("themes")).is_empty());
}
