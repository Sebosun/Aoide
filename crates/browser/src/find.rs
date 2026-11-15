//! Which browser runs: the one configured, else Chrome or Chromium on the `PATH`, else the
//! one Aoide downloaded. Chromium derivatives (Brave, Edge, …) are used only when configured.

use std::path::{Path, PathBuf};

/// The programs looked for on the `PATH`, in order.
pub const NAMES: [&str; 4] = [
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundBinary {
    pub path: PathBuf,
    /// It is the build Aoide downloaded.
    pub downloaded: bool,
}

/// The browser to run. `configured` is the user's choice (only it is tried when set),
/// `path_dirs` the `PATH`'s folders, `downloaded` where a downloaded build's program would be.
pub fn find_binary(
    configured: Option<&Path>,
    path_dirs: &[PathBuf],
    downloaded: &Path,
) -> Option<FoundBinary> {
    let found = |path: &Path, downloaded| {
        runs(path).then(|| FoundBinary {
            path: path.to_owned(),
            downloaded,
        })
    };
    if let Some(configured) = configured {
        return found(configured, false);
    }
    path_dirs
        .iter()
        .flat_map(|dir| NAMES.iter().map(move |name| dir.join(name)))
        .find_map(|path| found(&path, false))
        .or_else(|| found(downloaded, true))
}

/// The `PATH`'s folders.
pub fn path_dirs() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

/// Whether `path` is a file this user may run.
fn runs(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// The browser's version, as it reports it (`Chromium 154.0.8037.97`).
pub fn version(binary: &Path) -> Option<String> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !text.is_empty()).then_some(text)
}
