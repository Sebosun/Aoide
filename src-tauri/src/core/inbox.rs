//! The inbox: a folder the user chose (Downloads, or one a sync tool fills) whose books are
//! added to the library as they appear. Only the files directly in it are looked at, and only
//! book files; each is taken once its size has held for [`SETTLE`], copied into the library,
//! and left where it is.

// Started by the core in a later step.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use super::{lock, CoreEvent, Events};

/// What the inbox asks of the rest of the app.
pub(super) trait InboxHost {
    /// Copies the book file at `path` into the library, as a dropped file is.
    fn import(&self, path: &Path) -> Result<InboxBook, String>;
    /// Whether book `id` is in the library with its file present.
    fn in_library(&self, id: &str) -> bool;
}

/// A book the inbox added to the library: what the window tells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct InboxBook {
    /// The item it is.
    pub id: String,
    /// Its title.
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
    events: Arc<dyn Events>,
    stage: Mutex<Stage>,
    files: Mutex<Files>,
}

impl Inbox {
    pub fn new(events: Arc<dyn Events>) -> Self {
        Self {
            events,
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
        let mut ready = ready;
        ready.sort_by(|a, b| a.0.cmp(&b.0));
        let mut added = Vec::new();
        for (path, stamp) in ready {
            added.extend(take(&path, host));
            lock(&self.files).taken.insert(path, stamp);
        }
        if !added.is_empty() {
            self.emit(CoreEvent::InboxAdded(added));
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

    fn emit(&self, event: CoreEvent) {
        let name = event.name();
        match event.payload() {
            Ok(value) => self.events.emit(name, value),
            Err(error) => {
                tracing::error!(event = "app.emit", name, %error, "Cannot send an event to the window")
            }
        }
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

/// Copies the book at `path` into the library unless its content is there already: what
/// was added, if anything.
fn take(path: &Path, host: &dyn InboxHost) -> Option<InboxBook> {
    let file = crate::logging::file(path);
    let skipped = |why: &str, error: &str| {
        tracing::info!(event = "inbox.skipped", %file, why, error, "Passed over a file in the inbox");
    };
    let id = match std::fs::read(path) {
        Ok(bytes) => aoide_library::content_hash(&bytes).0,
        Err(error) => {
            skipped("unreadable", &error.to_string());
            return None;
        }
    };
    if host.in_library(&id) {
        skipped("in_library", "");
        return None;
    }
    match host.import(path) {
        Ok(book) => {
            tracing::info!(event = "inbox.imported", book = crate::logging::book(&book.id), %file, "Added a book from the inbox");
            Some(book)
        }
        Err(error) => {
            skipped("failed", &error);
            None
        }
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

    use aoide_library::{BookId, BookStatus, Library};

    use super::super::error::Reporter;
    use super::super::shelf::Shelf;
    use super::super::tests::{epub, Recorder};
    use super::super::CoreEvent;
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

        fn in_library(&self, _id: &str) -> bool {
            false
        }
    }

    impl InboxHost for Shelf {
        fn import(&self, path: &Path) -> Result<InboxBook, String> {
            let result = self
                .import_files(vec![path.to_string_lossy().into_owned()])
                .remove(0);
            let id = result.book_id.ok_or(result.error.unwrap_or_default())?;
            let title = self.shelve(|library| library.book(&BookId(id.clone())))?;
            Ok(InboxBook {
                id,
                title: title.and_then(|record| record.title).unwrap_or_default(),
            })
        }

        fn in_library(&self, id: &str) -> bool {
            self.shelve(|library| library.book(&BookId(id.into())))
                .ok()
                .flatten()
                .is_some_and(|record| record.status == BookStatus::Present)
        }
    }

    /// A library folder with its shelf, an inbox folder, and the inbox.
    struct World {
        _dir: tempfile::TempDir,
        root: PathBuf,
        shelf: Shelf,
        events: Arc<Recorder>,
        inbox: Inbox,
    }

    fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_owned();
        for folder in ["Books", "covers", "Downloads"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        let library = Arc::new(Mutex::new(Library::open_in_memory().unwrap()));
        let events = Arc::new(Recorder::default());
        let shelf = Shelf::new(
            library,
            root.join("Books"),
            root.join("covers"),
            Reporter::default(),
            events.clone(),
        );
        World {
            _dir: dir,
            root,
            shelf,
            inbox: Inbox::new(events.clone()),
            events,
        }
    }

    impl World {
        fn downloads(&self) -> PathBuf {
            self.root.join("Downloads")
        }

        /// The names of the files in `folder`, in order.
        fn files(&self, folder: &str) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.root.join(folder))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        /// Looks at the inbox twice, `SETTLE` apart, from `at`.
        fn settle(&self, at: Instant) {
            self.inbox.pass_at(&self.downloads(), at, &self.shelf);
            self.inbox
                .pass_at(&self.downloads(), at + SETTLE, &self.shelf);
        }

        fn added(&self) -> Vec<serde_json::Value> {
            self.events.payloads(CoreEvent::INBOX_ADDED)
        }
    }

    #[test]
    fn a_new_book_is_copied_in_once_and_the_same_content_under_another_name_is_not() {
        let world = world();
        std::fs::write(world.downloads().join("Lights.epub"), epub()).unwrap();
        std::fs::write(world.downloads().join("Lights (1).epub"), epub()).unwrap();
        let start = Instant::now();

        world.settle(start);

        assert_eq!(world.files("Books"), ["Lights (1).epub"]);
        assert_eq!(world.files("Downloads"), ["Lights (1).epub", "Lights.epub"]);
        assert_eq!(world.shelf.shelve(|l| l.books()).unwrap().len(), 1);
        let added = world.added();
        assert_eq!(added.len(), 1, "one toast for what one look added");
        assert_eq!(added[0].as_array().unwrap().len(), 1);
        assert_eq!(added[0][0]["title"], "Quay Lights");

        // Looked at again (a later change in the folder): nothing more.
        world.settle(start + Duration::from_secs(10));
        assert_eq!(world.files("Books"), ["Lights (1).epub"]);
        assert_eq!(world.added().len(), 1);
    }

    #[test]
    fn a_book_already_in_the_library_is_passed_over() {
        let world = world();
        std::fs::write(world.root.join("Books/Kept.epub"), epub()).unwrap();
        world.shelf.rescan().unwrap();
        std::fs::write(world.downloads().join("Kept again.epub"), epub()).unwrap();

        world.settle(Instant::now());

        assert_eq!(world.files("Books"), ["Kept.epub"]);
        assert!(world.added().is_empty());
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
        Inbox::new(Arc::new(Recorder::default()))
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
