//! `aoide --install-desktop-file`: for a build from source, makes Aoide an application the
//! desktop knows, so "Open with → Aoide" works for EPUBs and PDFs and `aoide:` addresses reach
//! it. The binary writes it, because it knows its own path.

use std::path::{Path, PathBuf};

/// The one argument that asks for it; `main` checks for it before anything else runs.
pub const FLAG: &str = "--install-desktop-file";

/// What the desktop file is called, in `applications`.
const NAME: &str = "aoide.desktop";
/// Aoide's icon, installed for the desktop file's `Icon=aoide`.
const ICON: &[u8] = include_bytes!("../icons/icon.png");
/// Where the icon goes, under the data folder: its size is that of `icons/icon.png`.
const ICON_PATH: &str = "icons/hicolor/32x32/apps/aoide.png";

/// Runs a desktop tool with its arguments; an error says why it could not (not installed).
pub type Run<'a> = &'a mut dyn FnMut(&str, &[&str]) -> Result<(), String>;

/// What installing did.
#[derive(Debug, PartialEq, Eq)]
pub struct Installed {
    pub desktop_file: PathBuf,
    pub icon: PathBuf,
    /// The desktop tools that could not be run, each with why.
    pub tools_failed: Vec<(String, String)>,
}

/// The desktop file for the program at `exe`.
pub fn entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Aoide\n\
         GenericName=Audiobook player\n\
         Comment=Read and listen to EPUBs, PDFs and articles\n\
         Exec={} %U\n\
         Icon=aoide\n\
         Terminal=false\n\
         Categories=AudioVideo;Audio;Player;\n\
         MimeType=x-scheme-handler/aoide;application/epub+zip;application/pdf;\n",
        quoted(&exe.to_string_lossy())
    )
}

/// `arg` as one argument of an `Exec` line: quoted, with what the quotes and the file's own
/// escapes would read otherwise escaped (the desktop entry specification, "The Exec key").
fn quoted(arg: &str) -> String {
    let mut out = String::from('"');
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' => {
                out.push_str("\\\\");
                out.push(c);
            }
            '\\' => out.push_str("\\\\\\\\"),
            '%' => out.push_str("%%"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Writes the desktop file and the icon under `data_home` (`~/.local/share`), then has the
/// desktop read them again and makes Aoide the handler of `aoide:` addresses. A tool that is
/// missing is reported, not fatal; a file that cannot be written is.
pub fn install(exe: &Path, data_home: &Path, run: Run<'_>) -> Result<Installed, String> {
    let applications = data_home.join("applications");
    let desktop_file = applications.join(NAME);
    let icon = data_home.join(ICON_PATH);
    write(&desktop_file, entry(exe).as_bytes(), "desktop_file")?;
    write(&icon, ICON, "icon")?;
    let folder = applications.to_string_lossy();
    let mut tools_failed = Vec::new();
    for (tool, args) in [
        ("update-desktop-database", vec![folder.as_ref()]),
        ("xdg-mime", vec!["default", NAME, "x-scheme-handler/aoide"]),
    ] {
        match run(tool, &args) {
            Ok(()) => tracing::info!(
                event = "launch.desktop_file",
                step = tool,
                outcome = "ok",
                "Ran {tool}"
            ),
            Err(error) => {
                tracing::warn!(event = "launch.desktop_file", step = tool, outcome = "failed", %error, "Could not run {tool}");
                tools_failed.push((tool.to_owned(), error));
            }
        }
    }
    Ok(Installed {
        desktop_file,
        icon,
        tools_failed,
    })
}

fn write(path: &Path, bytes: &[u8], step: &str) -> Result<(), String> {
    let file = crate::logging::file(path);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, bytes));
    match written {
        Ok(()) => {
            tracing::info!(event = "launch.desktop_file", step, outcome = "ok", %file, "Wrote {file}");
            Ok(())
        }
        Err(error) => {
            tracing::warn!(event = "launch.desktop_file", step, outcome = "failed", %file, %error, "Could not write {file}");
            Err(format!("{}: {error}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_runs_this_program_with_the_files_or_addresses_given() {
        let text = entry(Path::new("/home/mira/Aoide build/target/release/aoide"));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "[Desktop Entry]");
        for line in [
            "Type=Application",
            "Name=Aoide",
            "Icon=aoide",
            "Terminal=false",
            "Exec=\"/home/mira/Aoide build/target/release/aoide\" %U",
            "MimeType=x-scheme-handler/aoide;application/epub+zip;application/pdf;",
        ] {
            assert!(lines.contains(&line), "{line} missing from:\n{text}");
        }
    }

    #[test]
    fn a_path_with_quotes_dollars_backslashes_or_percents_is_escaped() {
        let text = entry(Path::new(r#"/opt/a"b$c\d%e/aoide"#));
        assert!(
            text.contains(r#"Exec="/opt/a\\"b\\$c\\\\d%%e/aoide" %U"#),
            "{text}"
        );
    }

    #[test]
    fn installing_writes_the_entry_and_icon_and_registers_them() {
        let dir = tempfile::tempdir().unwrap();
        let exe = Path::new("/opt/aoide/aoide");
        let mut ran = Vec::new();
        let mut run = |tool: &str, args: &[&str]| {
            ran.push(format!("{tool} {}", args.join(" ")));
            Ok(())
        };

        let installed = install(exe, dir.path(), &mut run).unwrap();

        let applications = dir.path().join("applications");
        assert_eq!(installed.desktop_file, applications.join("aoide.desktop"));
        assert_eq!(
            std::fs::read_to_string(&installed.desktop_file).unwrap(),
            entry(exe)
        );
        assert_eq!(
            installed.icon,
            dir.path().join("icons/hicolor/32x32/apps/aoide.png")
        );
        assert_eq!(std::fs::read(&installed.icon).unwrap(), ICON);
        assert_eq!(installed.tools_failed, []);
        assert_eq!(
            ran,
            [
                format!("update-desktop-database {}", applications.display()),
                "xdg-mime default aoide.desktop x-scheme-handler/aoide".to_string(),
            ]
        );
    }

    #[test]
    fn a_missing_desktop_tool_is_reported_and_the_files_stay() {
        let dir = tempfile::tempdir().unwrap();
        let mut run = |tool: &str, _: &[&str]| -> Result<(), String> {
            if tool == "xdg-mime" {
                Err("not installed".into())
            } else {
                Ok(())
            }
        };

        let installed = install(Path::new("/opt/aoide/aoide"), dir.path(), &mut run).unwrap();

        assert!(installed.desktop_file.is_file());
        assert_eq!(
            installed.tools_failed,
            [("xdg-mime".to_string(), "not installed".to_string())]
        );
    }

    #[test]
    fn a_folder_that_cannot_be_written_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("applications"), b"a file in the way").unwrap();
        let mut run = |_: &str, _: &[&str]| Ok(());
        assert!(install(Path::new("/opt/aoide/aoide"), dir.path(), &mut run).is_err());
    }
}
