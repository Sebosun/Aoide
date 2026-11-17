//! The inbox: a folder the user chose (Downloads, or one a sync tool fills) whose books are
//! added to the library as they appear. Only the files directly in it are looked at, and only
//! book files; each is taken once its size has held for [`SETTLE`], copied into the library,
//! and left where it is.

// Started by the core in a later step.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::lock;

/// What the inbox asks of the rest of the app.
pub(super) trait InboxHost {
    /// Copies the book file at `path` into the library, as a dropped file is.
    fn import(&self, path: &Path) -> Result<InboxBook, String>;
}

/// A book the inbox added to the library: what the window tells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct InboxBook {
    pub id: String,
    pub title: String,
}

/// How long a file's size must hold before it is taken: a download or a sync still writing
/// it grows meanwhile.
pub const SETTLE: Duration = Duration::from_secs(2);

/// A file as last seen: its length and when it was last written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

/// Where the inbox is. One import at a time: the folder is looked at on one thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Nothing waits.
    Idle,
    /// Files wait for their size to hold; the first was seen `since`.
    Settling { since: Instant },
    /// Files whose size held are being copied in.
    Importing,
    /// The folder cannot be listed (gone, or on a drive taken out).
    Unavailable,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::Idle => "idle",
            Stage::Settling { .. } => "settling",
            Stage::Importing => "importing",
            Stage::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    /// The folder was listed: files still wait since `waiting` (the earliest), and some are
    /// `ready` to take.
    Listed {
        waiting: Option<Instant>,
        ready: bool,
    },
    /// The ready files were taken; files still wait since `waiting`.
    Taken { waiting: Option<Instant> },
    /// The folder could not be listed.
    Gone,
}

fn next(stage: Stage, event: Event) -> Stage {
    let settled = |waiting: Option<Instant>| match waiting {
        Some(since) => Stage::Settling { since },
        None => Stage::Idle,
    };
    match (stage, event) {
        (_, Event::Gone) => Stage::Unavailable,
        (_, Event::Listed { ready: true, .. }) => Stage::Importing,
        (_, Event::Listed { waiting, .. }) => settled(waiting),
        (Stage::Importing, Event::Taken { waiting }) => settled(waiting),
        (stage, Event::Taken { .. }) => stage,
    }
}

/// The files the inbox knows of.
#[derive(Default)]
struct Files {
    /// Seen and not taken yet: as they were, and since when they have been so.
    waiting: HashMap<PathBuf, (Stamp, Instant)>,
    /// Taken (added, passed over or failed), as they were then: not looked at again unless
    /// they change.
    taken: HashMap<PathBuf, Stamp>,
}

pub(super) struct Inbox {
    stage: Mutex<Stage>,
    files: Mutex<Files>,
}

impl Inbox {
    pub fn new() -> Self {
        Self {
            stage: Mutex::new(Stage::Idle),
            files: Mutex::new(Files::default()),
        }
    }

    /// Looks at the folder `dir` at `now`, and takes the books in it whose size held for
    /// [`SETTLE`]. Returns the stage it is in after.
    fn pass_at(&self, dir: &Path, now: Instant, host: &dyn InboxHost) -> Stage {
        let Ok(found) = listed(dir) else {
            return self.apply(Event::Gone);
        };
        let ready = {
            let mut files = lock(&self.files);
            let Files { waiting, taken } = &mut *files;
            taken.retain(|path, _| found.iter().any(|(p, _)| p == path));
            waiting.retain(|path, _| found.iter().any(|(p, _)| p == path));
            let mut ready = Vec::new();
            for (path, stamp) in found {
                if taken.get(&path) == Some(&stamp) {
                    continue;
                }
                match waiting.get(&path) {
                    // An empty file is a placeholder a browser fills in later.
                    Some((seen, since))
                        if *seen == stamp && stamp.len > 0 && now >= *since + SETTLE =>
                    {
                        waiting.remove(&path);
                        ready.push((path, stamp));
                    }
                    Some((seen, _)) if *seen == stamp => {}
                    _ => {
                        waiting.insert(path, (stamp, now));
                    }
                }
            }
            ready
        };
        let stage = self.apply(Event::Listed {
            waiting: self.waiting_since(),
            ready: !ready.is_empty(),
        });
        if ready.is_empty() {
            return stage;
        }
        for (path, stamp) in ready {
            let _ = host.import(&path);
            lock(&self.files).taken.insert(path, stamp);
        }
        self.apply(Event::Taken {
            waiting: self.waiting_since(),
        })
    }

    /// When the file that has waited longest was seen as it is.
    fn waiting_since(&self) -> Option<Instant> {
        lock(&self.files)
            .waiting
            .values()
            .map(|(_, since)| *since)
            .min()
    }

    fn apply(&self, event: Event) -> Stage {
        let mut stage = lock(&self.stage);
        let from = *stage;
        *stage = next(from, event);
        if from.name() != stage.name() {
            tracing::info!(
                event = "inbox.stage",
                from = from.name(),
                to = stage.name(),
                "The inbox moved on"
            );
        }
        *stage
    }
}

/// The book files directly in `dir` (hidden ones left out), each as it is now.
fn listed(dir: &Path) -> std::io::Result<Vec<(PathBuf, Stamp)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let hidden = path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with('.'));
        if hidden || !aoide_library::is_book_file(&path) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_file() {
            let stamp = Stamp {
                len: meta.len(),
                modified: meta.modified().ok(),
            };
            files.push((path, stamp));
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::super::tests::epub;
    use super::*;

    /// Records what it is asked to import, and imports nothing.
    #[derive(Default)]
    struct Imports(Mutex<Vec<String>>);

    impl InboxHost for Imports {
        fn import(&self, path: &Path) -> Result<InboxBook, String> {
            let name = crate::logging::file(path).into_owned();
            lock(&self.0).push(name.clone());
            Ok(InboxBook {
                id: name.clone(),
                title: name,
            })
        }
    }

    impl Imports {
        fn taken(&self) -> Vec<String> {
            lock(&self.0).clone()
        }
    }

    #[test]
    fn the_stage_moves_on_named_events() {
        let t = Instant::now();
        let settling = Stage::Settling { since: t };
        let cases = [
            (
                Stage::Idle,
                Event::Listed {
                    waiting: None,
                    ready: false,
                },
                Stage::Idle,
            ),
            (
                Stage::Idle,
                Event::Listed {
                    waiting: Some(t),
                    ready: false,
                },
                settling,
            ),
            (
                settling,
                Event::Listed {
                    waiting: Some(t),
                    ready: true,
                },
                Stage::Importing,
            ),
            (
                Stage::Importing,
                Event::Taken { waiting: None },
                Stage::Idle,
            ),
            (
                Stage::Importing,
                Event::Taken { waiting: Some(t) },
                settling,
            ),
            (Stage::Idle, Event::Taken { waiting: Some(t) }, Stage::Idle),
            (settling, Event::Gone, Stage::Unavailable),
            (Stage::Importing, Event::Gone, Stage::Unavailable),
            (
                Stage::Unavailable,
                Event::Listed {
                    waiting: None,
                    ready: false,
                },
                Stage::Idle,
            ),
        ];
        for (from, event, to) in cases {
            assert_eq!(next(from, event), to, "{from:?} on {event:?}");
        }
    }

    fn inbox() -> Inbox {
        Inbox::new()
    }

    #[test]
    fn a_file_is_taken_once_its_size_held_for_two_seconds_and_a_download_in_progress_never() {
        let dir = tempfile::tempdir().unwrap();
        let (inbox, host, start) = (inbox(), Imports::default(), Instant::now());
        let at = |s: u64| start + Duration::from_secs(s);
        let growing = dir.path().join("Growing.epub");
        std::fs::write(&growing, &epub()[..100]).unwrap();
        std::fs::write(dir.path().join("Coming.epub.part"), epub()).unwrap();
        std::fs::write(dir.path().join("Empty.epub"), b"").unwrap();

        inbox.pass_at(dir.path(), at(0), &host);
        assert!(host.taken().is_empty(), "nothing is taken on sight");

        // Still growing two seconds later: it waits two seconds more.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&growing)
            .unwrap();
        file.write_all(&epub()[100..]).unwrap();
        inbox.pass_at(dir.path(), at(2), &host);
        inbox.pass_at(dir.path(), at(3), &host);
        assert!(host.taken().is_empty());

        inbox.pass_at(dir.path(), at(4), &host);
        assert_eq!(host.taken(), ["Growing.epub"]);
        // A browser's placeholder stays empty until the download is renamed over it.
        inbox.pass_at(dir.path(), at(10), &host);
        assert_eq!(host.taken(), ["Growing.epub"]);
    }
}
