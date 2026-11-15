//! The browser's profile folder, kept between runs (cookies and passed checks live there).

use std::path::Path;

/// The files a running Chrome holds in its profile to keep a second one out.
const SINGLETON: [&str; 3] = ["SingletonLock", "SingletonSocket", "SingletonCookie"];

/// Clears the lock a crashed browser left in `profile`, when no process owns it any more;
/// whether it did. The lock is a link naming `<host>-<process id>`.
pub fn clear_stale_lock(profile: &Path) -> bool {
    let Ok(target) = std::fs::read_link(profile.join(SINGLETON[0])) else {
        return false;
    };
    let pid = target
        .to_string_lossy()
        .rsplit_once('-')
        .and_then(|(_, pid)| pid.parse::<u32>().ok());
    if pid.is_some_and(alive) {
        return false;
    }
    for name in SINGLETON {
        let _ = std::fs::remove_file(profile.join(name));
    }
    true
}

/// Whether process `pid` exists.
fn alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        Path::new("/proc").join(pid.to_string()).exists()
    } else {
        // Elsewhere a lock is assumed live; Chrome itself takes over a stale one there.
        true
    }
}
