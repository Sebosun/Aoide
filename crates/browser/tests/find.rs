use std::path::{Path, PathBuf};

use aoide_browser::find::{find_binary, FoundBinary};
use aoide_browser::patterns::matches;

/// A fake executable (or, with `run: false`, a plain file) at `dir/name`.
fn fake(dir: &Path, name: &str, run: bool) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\necho Chromium 1.0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if run { 0o755 } else { 0o644 };
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    }
    path
}

fn found(path: &Path, downloaded: bool) -> Option<FoundBinary> {
    Some(FoundBinary {
        path: path.to_owned(),
        downloaded,
    })
}

#[test]
fn the_configured_browser_then_chrome_on_the_path_then_the_downloaded_one() {
    let root = tempfile::tempdir().unwrap();
    let (bin, usr) = (root.path().join("bin"), root.path().join("usr"));
    let downloaded = root.path().join("data/browser/chrome/chrome");
    let dirs = [bin.clone(), usr.clone()];

    assert_eq!(find_binary(None, &dirs, &downloaded), None);
    let ours = fake(downloaded.parent().unwrap(), "chrome", true);
    assert_eq!(find_binary(None, &dirs, &downloaded), found(&ours, true));
    let chromium = fake(&usr, "chromium", true);
    assert_eq!(
        find_binary(None, &dirs, &downloaded),
        found(&chromium, false)
    );
    // Google Chrome first, and the earlier folder on the path first.
    let stable = fake(&usr, "google-chrome-stable", true);
    assert_eq!(find_binary(None, &dirs, &downloaded), found(&stable, false));
    let chrome = fake(&bin, "chromium-browser", true);
    assert_eq!(find_binary(None, &dirs, &downloaded), found(&chrome, false));
    // Configured wins, whatever its name.
    let custom = fake(&root.path().join("opt"), "my-chrome", true);
    assert_eq!(
        find_binary(Some(&custom), &dirs, &downloaded),
        found(&custom, false)
    );
    // A configured path that does not run is not used, and nothing else is either.
    let broken = fake(&root.path().join("opt"), "broken", false);
    assert_eq!(find_binary(Some(&broken), &dirs, &downloaded), None);
    assert_eq!(
        find_binary(Some(&root.path().join("gone")), &dirs, &downloaded),
        None
    );
}

#[test]
fn chromium_derivatives_are_used_only_when_configured() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let downloaded = root.path().join("none/chrome");
    let brave = fake(&bin, "brave-browser", true);
    fake(&bin, "microsoft-edge", true);
    fake(&bin, "google-chrome", false);
    assert_eq!(
        find_binary(None, std::slice::from_ref(&bin), &downloaded),
        None
    );
    assert_eq!(
        find_binary(Some(&brave), &[bin], &downloaded),
        found(&brave, false)
    );
}

#[test]
fn block_patterns_match_whole_addresses_with_wildcards() {
    assert!(matches("*/ads/*", "https://news.example/ads/banner.js"));
    assert!(matches(
        "*.doubleclick.net/*",
        "https://ad.doubleclick.net/x"
    ));
    assert!(!matches(
        "*.doubleclick.net/*",
        "https://example.com/doubleclick.net"
    ));
    assert!(matches(
        "https://cdn.example/*.js",
        "https://cdn.example/a/b.js"
    ));
    assert!(!matches(
        "https://cdn.example/*.js",
        "https://cdn.example/a/b.css"
    ));
    assert!(matches("*", "https://anything/"));
    assert!(!matches("", "https://anything/"));
}
