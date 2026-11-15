use aoide_browser::profile::clear_stale_lock;

#[cfg(unix)]
#[test]
fn a_lock_left_by_a_browser_that_is_gone_is_cleared_and_a_live_one_kept() {
    let dir = tempfile::tempdir().unwrap();
    let lock = dir.path().join("SingletonLock");
    let link = |target: &str| {
        let _ = std::fs::remove_file(&lock);
        std::os::unix::fs::symlink(target, &lock).unwrap();
    };
    std::fs::write(dir.path().join("SingletonCookie"), "").unwrap();

    // No process has this id (ids are far smaller).
    link("somehost-2147483000");
    assert!(clear_stale_lock(dir.path()));
    assert!(std::fs::symlink_metadata(&lock).is_err());
    assert!(!dir.path().join("SingletonCookie").exists());

    // This test's own process is alive.
    link(&format!("somehost-{}", std::process::id()));
    assert!(!clear_stale_lock(dir.path()));
    assert!(std::fs::symlink_metadata(&lock).is_ok());

    // No lock at all: nothing to do.
    let empty = tempfile::tempdir().unwrap();
    assert!(!clear_stale_lock(empty.path()));
}
