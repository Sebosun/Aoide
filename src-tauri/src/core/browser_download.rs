//! Downloading a browser for the user who has none: a pinned Chrome for Testing build.

/// The program inside the downloaded folder.
#[cfg(target_os = "linux")]
pub const PROGRAM: &str = "chrome-linux64/chrome";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub const PROGRAM: &str =
    "chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
pub const PROGRAM: &str =
    "chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing";
#[cfg(target_os = "windows")]
pub const PROGRAM: &str = "chrome-win64/chrome.exe";
