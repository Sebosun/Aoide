//! Watching a folder for changes to what it holds. Opening or reading a file is not a change:
//! `notify` reports opens too (inotify's `IN_OPEN`), and a watcher whose own work reads the
//! folder would otherwise wake itself again, forever.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecursiveMode, Watcher};

/// Whether `event` changed what the folder holds: not a file opened, read, or closed unwritten.
/// An error counts, since it may hide a change (a full event queue).
fn changes(event: &notify::Result<Event>) -> bool {
    match event {
        Ok(event) => match event.kind {
            EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
            EventKind::Access(_) => false,
            _ => true,
        },
        Err(_) => true,
    }
}

/// Calls `on_change` once the folder `dir` (and anything under it) changed and then stayed
/// unchanged for `settle`, again after each later change, until `on_change` breaks. Returns an
/// error only if the folder cannot be watched.
pub fn watch_folder(
    dir: &Path,
    settle: Duration,
    mut on_change: impl FnMut() -> ControlFlow<()>,
) -> notify::Result<()> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(dir, RecursiveMode::Recursive)?;
    loop {
        // A change, then quiet: copies and editors' saves come as bursts of events.
        loop {
            match rx.recv() {
                Ok(event) if changes(&event) => break,
                Ok(_) => {}
                Err(_) => return Ok(()),
            }
        }
        let mut quiet_from = Instant::now() + settle;
        loop {
            match rx.recv_timeout(quiet_from.saturating_duration_since(Instant::now())) {
                Ok(event) if changes(&event) => quiet_from = Instant::now() + settle,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
        if on_change().is_break() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Arc;
    use std::time::Duration;

    use super::*;

    /// Wider than a pause between two writes on a busy machine (a parallel build).
    const SETTLE: Duration = Duration::from_millis(250);
    /// Long enough for a change to come through and settle on a busy machine.
    const COMES: Duration = Duration::from_secs(5);
    /// Long enough that a call that was going to come has come.
    const QUIET: Duration = Duration::from_millis(1500);

    /// A folder being watched; each call of the callback sends on the channel. `work` runs in
    /// each call (it may read the folder). The watcher stops at the first change after the
    /// folder is dropped.
    struct Watched {
        dir: tempfile::TempDir,
        calls: Receiver<()>,
        stop: Arc<AtomicBool>,
    }

    fn watched(work: impl Fn(&Path) + Send + 'static) -> Watched {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.epub"), b"a book").unwrap();
        let (tx, calls) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let (path, stopped) = (dir.path().to_path_buf(), stop.clone());
        std::thread::spawn(move || {
            watch_folder(&path, SETTLE, || {
                if stopped.load(Ordering::SeqCst) {
                    return ControlFlow::Break(());
                }
                work(&path);
                let _ = tx.send(());
                ControlFlow::Continue(())
            })
            .unwrap();
        });
        // The watch is set up on the thread: write a file until a change comes through, so the
        // test acts on a folder that is being watched, then let that change pass.
        let probe = dir.path().join("probe");
        let ready = (0..10).any(|n| {
            std::fs::write(&probe, n.to_string()).unwrap();
            calls.recv_timeout(SETTLE * 4).is_ok()
        });
        assert!(ready, "the folder was never watched");
        // Bounded: a watcher that wakes itself would never go quiet (the tests then fail).
        for _ in 0..3 {
            if calls.recv_timeout(QUIET).is_err() {
                break;
            }
        }
        Watched { dir, calls, stop }
    }

    impl Drop for Watched {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }

    fn read_all(dir: &Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let _ = std::fs::read(entry.unwrap().path());
        }
    }

    #[test]
    fn listing_the_folder_and_reading_its_files_is_not_a_change() {
        let folder = watched(|_| {});
        read_all(folder.dir.path());
        assert!(folder.calls.recv_timeout(QUIET).is_err());
    }

    #[test]
    fn a_new_file_is_one_change_once_the_folder_settles() {
        let folder = watched(|_| {});
        for n in 0..3 {
            std::fs::write(folder.dir.path().join(format!("{n}.epub")), b"x").unwrap();
        }
        assert!(folder.calls.recv_timeout(COMES).is_ok());
        assert!(folder.calls.recv_timeout(QUIET).is_err());
    }

    #[test]
    fn work_that_reads_the_folder_does_not_wake_the_watcher_again() {
        let folder = watched(read_all);
        std::fs::write(folder.dir.path().join("b.epub"), b"another").unwrap();
        assert!(folder.calls.recv_timeout(COMES).is_ok());
        assert!(
            folder.calls.recv_timeout(QUIET).is_err(),
            "the watcher woke itself"
        );
    }
}
