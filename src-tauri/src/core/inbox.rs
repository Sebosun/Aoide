//! The inbox: a folder the user chose (Downloads, or one a sync tool fills) whose books are
//! added to the library as they appear. Only the files directly in it are looked at, and only
//! book files; each is taken once its size has held for [`SETTLE`], copied into the library,
//! and left where it is.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use notify::{RecursiveMode, Watcher};
use serde::Serialize;

use super::{lock, CoreEvent, Events};

/// What the inbox asks of the rest of the app.
pub(super) trait InboxHost {
    /// Copies the book file at `path` into the library, as a dropped file is.
    fn import(&self, path: &Path) -> Result<InboxBook, String>;
    /// Whether book `id` is in the library with its file present.
    fn in_library(&self, id: &str) -> bool;
    /// The books the user removed: their files are not added again from the inbox.
    fn removed(&self) -> HashSet<String>;
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

/// The inbox as Settings shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct InboxView {
    /// The folder chosen, if any.
    pub folder: Option<String>,
    /// Whether it is watched, or gone.
    pub state: InboxState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum InboxState {
    /// No folder is chosen.
    Off,
    /// Books that appear in the folder are added.
    Watching,
    /// The folder is not there (on a drive taken out?); it is looked for every 30 seconds.
    Unavailable,
}

/// Whether `folder` can be the inbox: a full path, outside the library folder `library_dir`
/// (whose books are in the library already).
pub(super) fn check_folder(folder: &Path, library_dir: &Path) -> Result<(), String> {
    if !folder.is_absolute() {
        return Err("The inbox must be given as a folder's full path".into());
    }
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    if real(folder).starts_with(real(library_dir)) {
        return Err(
            "The inbox cannot be the library folder or a folder inside it: books there are in the library already"
                .into(),
        );
    }
    Ok(())
}

/// How long a file's size must hold before it is taken: a download or a sync still writing
/// it grows meanwhile.
pub const SETTLE: Duration = Duration::from_secs(2);
/// How often a folder that is gone is looked for, and a watched one checked to be the same
/// folder still (a drive taken out leaves an empty folder where it was).
pub const RECHECK: Duration = Duration::from_secs(30);

/// How long the inbox waits for what.
#[derive(Debug, Clone, Copy)]
struct Timing {
    /// A file's size must hold this long before it is taken.
    settle: Duration,
    /// After a change in the folder, it is looked at this much later (a copy comes as a
    /// burst of changes); changes meanwhile do not put the look off.
    quiet: Duration,
    /// See [`RECHECK`].
    recheck: Duration,
}

const TIMING: Timing = Timing {
    settle: SETTLE,
    quiet: Duration::from_millis(500),
    recheck: RECHECK,
};

/// What wakes the inbox's thread.
#[derive(Debug)]
enum Wake {
    /// The folder changed (not a file opened or read in it).
    Changed,
    /// Another folder was chosen, or none.
    Folder,
    /// Aoide is closing.
    Stop,
}

/// Why watching a folder ended.
enum Ended {
    /// It is gone, or cannot be watched: look for it again later.
    Lost,
    /// Watch the folder chosen now (or the same one again: it was replaced).
    Again,
    Stop,
}

/// A file as last seen: its length and when it was last written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

/// Where the inbox is. One import at a time: the folder is looked at on one thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// No folder is chosen.
    Off,
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
            Stage::Off => "off",
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
    /// The folder could not be listed, or watched.
    Gone,
    /// No folder is chosen.
    Off,
}

fn next(stage: Stage, event: Event) -> Stage {
    let settled = |waiting: Option<Instant>| match waiting {
        Some(since) => Stage::Settling { since },
        None => Stage::Idle,
    };
    match (stage, event) {
        (_, Event::Off) => Stage::Off,
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
    timing: Timing,
    /// The folder chosen.
    folder: Mutex<Option<PathBuf>>,
    stage: Mutex<Stage>,
    files: Mutex<Files>,
    wake: Mutex<std::sync::mpsc::Sender<Wake>>,
    /// Taken by the thread that runs the inbox.
    woken: Mutex<Option<Receiver<Wake>>>,
}

impl Inbox {
    pub fn new(events: Arc<dyn Events>) -> Self {
        let (wake, woken) = std::sync::mpsc::channel();
        Self {
            events,
            timing: TIMING,
            folder: Mutex::new(None),
            stage: Mutex::new(Stage::Off),
            files: Mutex::new(Files::default()),
            wake: Mutex::new(wake),
            woken: Mutex::new(Some(woken)),
        }
    }

    /// Watches `folder` from now on, or nothing.
    pub fn set_folder(&self, folder: Option<PathBuf>) {
        let before = self.view();
        *lock(&self.folder) = folder;
        let _ = lock(&self.wake).send(Wake::Folder);
        self.tell(before);
    }

    /// The inbox as Settings shows it.
    pub fn view(&self) -> InboxView {
        let folder = lock(&self.folder).clone();
        let state = match (&folder, self.stage()) {
            (None, _) => InboxState::Off,
            (Some(_), Stage::Unavailable) => InboxState::Unavailable,
            (Some(_), _) => InboxState::Watching,
        };
        InboxView {
            folder: folder.map(|folder| folder.to_string_lossy().into_owned()),
            state,
        }
    }

    /// Tells the window the inbox's view when it is no longer `before`.
    fn tell(&self, before: InboxView) {
        let now = self.view();
        if now != before {
            self.emit(CoreEvent::InboxState(now));
        }
    }

    /// Ends [`Inbox::run`].
    pub fn stop(&self) {
        let _ = lock(&self.wake).send(Wake::Stop);
    }

    fn stage(&self) -> Stage {
        *lock(&self.stage)
    }

    /// Watches the folder chosen, taking the books that appear in it, until [`Inbox::stop`].
    /// A folder that is gone is looked for every [`RECHECK`]. Blocks its thread; runs once.
    pub fn run(&self, host: &dyn InboxHost) {
        let Some(woken) = lock(&self.woken).take() else {
            return;
        };
        loop {
            let folder = lock(&self.folder).clone();
            let wait = match folder {
                None => {
                    self.apply(Event::Off);
                    None
                }
                Some(dir) => match self.watch(&dir, &woken, host) {
                    Ended::Again => continue,
                    Ended::Lost => Some(self.timing.recheck),
                    Ended::Stop => return,
                },
            };
            // Until another folder is chosen, or it is time to look for the one gone.
            loop {
                let woke = match wait {
                    Some(wait) => woken.recv_timeout(wait),
                    None => woken.recv().map_err(|_| RecvTimeoutError::Disconnected),
                };
                match woke {
                    Ok(Wake::Folder) | Err(RecvTimeoutError::Timeout) => break,
                    Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                    // From a watch that has ended.
                    Ok(Wake::Changed) => {}
                }
            }
        }
    }

    /// Watches the folder `dir` and takes its books until it is gone, another is chosen, or
    /// the inbox stops.
    fn watch(&self, dir: &Path, woken: &Receiver<Wake>, host: &dyn InboxHost) -> Ended {
        let wake = lock(&self.wake).clone();
        let watcher = notify::recommended_watcher(move |event| {
            if super::watching::changes(&event) {
                let _ = wake.send(Wake::Changed);
            }
        })
        .and_then(|mut watcher| {
            watcher
                .watch(dir, RecursiveMode::NonRecursive)
                .map(|()| watcher)
        });
        let _watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                self.lost(&error.to_string());
                return Ended::Lost;
            }
        };
        let same = identity(dir);
        let mut look_at = Some(Instant::now());
        let mut check_at = Instant::now() + self.timing.recheck;
        loop {
            let now = Instant::now();
            if look_at.is_some_and(|at| at <= now) {
                look_at = match self.pass_at(dir, now, host) {
                    Stage::Unavailable => return Ended::Lost,
                    Stage::Settling { since } => Some(since + self.timing.settle),
                    _ => None,
                };
            }
            if check_at <= now {
                check_at = now + self.timing.recheck;
                match identity(dir) {
                    None => look_at = Some(now),
                    found if found != same => return Ended::Again,
                    _ => {}
                }
            }
            let until = look_at.map_or(check_at, |at| at.min(check_at));
            match woken.recv_timeout(until.saturating_duration_since(Instant::now())) {
                Ok(Wake::Changed) => {
                    let soon = Instant::now() + self.timing.quiet;
                    look_at = Some(look_at.map_or(soon, |at| at.min(soon)));
                }
                Ok(Wake::Folder) => return Ended::Again,
                Ok(Wake::Stop) | Err(RecvTimeoutError::Disconnected) => return Ended::Stop,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// The folder is gone or cannot be watched, for `error`.
    fn lost(&self, error: &str) {
        let was = self.stage();
        if self.apply(Event::Gone) != was {
            tracing::warn!(
                event = "inbox.unavailable",
                error,
                "The inbox folder cannot be watched"
            );
        }
    }

    /// Looks at the folder `dir` at `now`, and takes the books in it whose size held for
    /// [`SETTLE`]. Returns the stage it is in after.
    fn pass_at(&self, dir: &Path, now: Instant, host: &dyn InboxHost) -> Stage {
        let found = match listed(dir) {
            Ok(found) => found,
            Err(error) => {
                self.lost(&error.to_string());
                return Stage::Unavailable;
            }
        };
        let ready = {
            let mut files = lock(&self.files);
            let Files { waiting, taken } = &mut *files;
            taken.retain(|path, _| found.iter().any(|(p, _)| p == path));
            waiting.retain(|path, _| found.iter().any(|(p, _)| p == path));
            let mut ready = Vec::new();
            for (path, stamp) in found {
                // An empty file is a placeholder a browser fills in later: nothing to wait
                // for until it is.
                if stamp.len == 0 || taken.get(&path) == Some(&stamp) {
                    continue;
                }
                match waiting.get(&path) {
                    Some((seen, since)) if *seen == stamp && now >= *since + self.timing.settle => {
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
        let before = self.view();
        let (from, to) = {
            let mut stage = lock(&self.stage);
            let from = *stage;
            *stage = next(from, event);
            (from, *stage)
        };
        if from.name() != to.name() {
            tracing::info!(
                event = "inbox.stage",
                from = from.name(),
                to = to.name(),
                "The inbox moved on"
            );
        }
        self.tell(before);
        to
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
    // Importing takes a book off the removed list: removing it is what the user asked for.
    if host.removed().contains(&id) {
        skipped("removed", "");
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

impl InboxHost for super::Core {
    fn import(&self, path: &Path) -> Result<InboxBook, String> {
        let result = self
            .import_files(vec![path.to_string_lossy().into_owned()])
            .pop()
            .ok_or("it was not added")?;
        let id = result
            .book_id
            .ok_or_else(|| result.error.unwrap_or_else(|| "it was not added".into()))?;
        let record = lock(&self.library)
            .book(&aoide_library::BookId(id.clone()))
            .ok()
            .flatten();
        let title = record
            .and_then(|record| record.title)
            .unwrap_or_else(|| crate::logging::file(path).into_owned());
        Ok(InboxBook { id, title })
    }

    fn in_library(&self, id: &str) -> bool {
        lock(&self.library)
            .book(&aoide_library::BookId(id.into()))
            .ok()
            .flatten()
            .is_some_and(|record| record.status == aoide_library::BookStatus::Present)
    }

    fn removed(&self) -> HashSet<String> {
        self.shelf.ignored()
    }
}

impl super::Core {
    /// The inbox folder, and whether it is being watched.
    pub fn inbox_status(&self) -> InboxView {
        self.inbox.view()
    }
}

/// Which folder `dir` is, so one put in its place is told apart; `None` when it is gone.
#[cfg(unix)]
fn identity(dir: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(dir).ok().filter(|meta| meta.is_dir())?;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity(dir: &Path) -> Option<()> {
    dir.is_dir().then_some(())
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
    use super::{InboxState, InboxView};

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

        fn removed(&self) -> HashSet<String> {
            HashSet::new()
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

        fn removed(&self) -> HashSet<String> {
            self.ignored()
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

    #[test]
    fn a_book_removed_from_the_library_is_not_added_again_from_the_inbox() {
        let world = world();
        std::fs::write(world.downloads().join("Lights.epub"), epub()).unwrap();
        world.settle(Instant::now());
        let id = world.added()[0][0]["id"].as_str().unwrap().to_owned();

        world.shelf.remove(&id).unwrap();
        // Aoide starts again: the inbox has not seen the file yet.
        let restarted = Inbox::new(world.events.clone());
        let start = Instant::now();
        restarted.pass_at(&world.downloads(), start, &world.shelf);
        restarted.pass_at(&world.downloads(), start + SETTLE, &world.shelf);

        assert!(!world.shelf.in_library(&id));
        assert!(world.shelf.ignored().contains(&id), "still removed");
        assert_eq!(world.added().len(), 1);
    }

    /// Waits until `done` holds, or fails the test after a while.
    fn until(what: &str, done: impl Fn() -> bool) {
        let start = Instant::now();
        while !done() {
            assert!(start.elapsed() < Duration::from_secs(10), "never {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn watching_stops_while_the_folder_is_gone_and_starts_again_when_it_is_back() {
        let mut world = world();
        world.inbox.timing = Timing {
            settle: Duration::from_millis(300),
            quiet: Duration::from_millis(100),
            recheck: Duration::from_millis(300),
        };
        world.inbox.set_folder(Some(world.downloads()));
        std::thread::scope(|scope| {
            let world = &world;
            scope.spawn(|| world.inbox.run(&world.shelf));
            until("watching", || world.inbox.stage() == Stage::Idle);
            std::fs::write(world.downloads().join("Lights.epub"), epub()).unwrap();
            until("added", || world.added().len() == 1);

            std::fs::remove_dir_all(world.downloads()).unwrap();
            until("unavailable", || world.inbox.stage() == Stage::Unavailable);

            std::fs::create_dir(world.downloads()).unwrap();
            let other = super::super::tests::epub_with(
                "<html><body><p>The tide came in.</p></body></html>".into(),
                "<html><body><p>The tide went out.</p></body></html>".into(),
            );
            std::fs::write(world.downloads().join("Tide.epub"), other).unwrap();
            until("added again", || world.added().len() == 2);
            assert_eq!(world.files("Books"), ["Lights.epub", "Tide.epub"]);

            world.inbox.set_folder(None);
            until("off", || world.inbox.stage() == Stage::Off);
            world.inbox.stop();
        });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_folder_chosen_in_settings_is_watched_and_the_library_folder_is_refused() {
        use super::super::AppSettingsPatch;
        let dir = tempfile::tempdir().unwrap();
        // The library holds another book.
        let other = super::super::tests::epub_with(
            "<html><body><p>The tide came in.</p></body></html>".into(),
            "<html><body><p>The tide went out.</p></body></html>".into(),
        );
        super::super::tests::library_folder_holding(
            dir.path(),
            "",
            "voiceId = \"Test.wav\"",
            &other,
        );
        let (core, events) = super::super::tests::start_with(
            dir.path(),
            &aoide_tts::testing::FakeEngine::new(),
            Arc::new(|_| Err("no speech recognition in this test".into())),
        );
        let choose = |folder: Option<PathBuf>| {
            core.update_app_settings(AppSettingsPatch {
                inbox: Some(folder),
                ..Default::default()
            })
        };
        let library = dir.path().join("Books");
        let refused = choose(Some(library.clone())).unwrap_err();
        assert!(refused.contains("library folder"), "{refused}");
        assert!(choose(Some(library.join("Incoming"))).is_err());
        assert!(choose(Some(PathBuf::from("Downloads"))).is_err());
        assert_eq!(core.app_settings().inbox, None);
        assert_eq!(core.inbox_status().state, InboxState::Off);

        let downloads = dir.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        choose(Some(downloads.clone())).unwrap();
        let watching = InboxView {
            folder: Some(downloads.to_string_lossy().into_owned()),
            state: InboxState::Watching,
        };
        assert_eq!(core.inbox_status(), watching);
        let told = events.payloads(CoreEvent::INBOX_STATE);
        assert_eq!(told.last(), Some(&serde_json::to_value(&watching).unwrap()));

        // Saving an EPUB to Downloads puts it in the library without touching Aoide.
        let running = Arc::clone(&core);
        let thread = std::thread::spawn(move || running.inbox.run(&*running));
        std::fs::write(downloads.join("Lights.epub"), epub()).unwrap();
        until("added", || events.count(CoreEvent::INBOX_ADDED) == 1);
        let added = &events.payloads(CoreEvent::INBOX_ADDED)[0][0];
        assert_eq!(added["title"], "Quay Lights");
        let id = added["id"].as_str().unwrap();
        assert!(core.list_books().unwrap().iter().any(|book| book.id == id));
        assert!(downloads.join("Lights.epub").is_file(), "left where it was");

        choose(None).unwrap();
        assert_eq!(core.inbox_status().state, InboxState::Off);
        core.shutdown();
        thread.join().unwrap();
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
        let listed = |waiting, ready| Event::Listed { waiting, ready };
        let taken = |waiting| Event::Taken { waiting };
        let cases = [
            (Stage::Off, listed(None, false), Stage::Idle),
            (Stage::Idle, listed(None, false), Stage::Idle),
            (Stage::Idle, listed(Some(t), false), settling),
            (settling, listed(Some(t), true), Stage::Importing),
            (Stage::Importing, taken(None), Stage::Idle),
            (Stage::Importing, taken(Some(t)), settling),
            (Stage::Idle, taken(Some(t)), Stage::Idle),
            (settling, Event::Gone, Stage::Unavailable),
            (Stage::Importing, Event::Gone, Stage::Unavailable),
            (Stage::Unavailable, listed(None, false), Stage::Idle),
            (Stage::Unavailable, Event::Off, Stage::Off),
            (settling, Event::Off, Stage::Off),
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

    #[test]
    fn an_empty_placeholder_does_not_keep_the_inbox_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let (inbox, host, start) = (inbox(), Imports::default(), Instant::now());
        // Firefox leaves it under the final name for as long as the download takes.
        std::fs::write(dir.path().join("Coming.epub"), b"").unwrap();

        assert_eq!(inbox.pass_at(dir.path(), start, &host), Stage::Idle);
        assert_eq!(
            inbox.pass_at(dir.path(), start + Duration::from_secs(60), &host),
            Stage::Idle,
            "nothing to look at again on its own"
        );

        // The download is renamed over it: it waits its two seconds, then it is taken.
        std::fs::write(dir.path().join("Coming.epub"), epub()).unwrap();
        let later = start + Duration::from_secs(61);
        assert!(matches!(
            inbox.pass_at(dir.path(), later, &host),
            Stage::Settling { .. }
        ));
        inbox.pass_at(dir.path(), later + SETTLE, &host);
        assert_eq!(host.taken(), ["Coming.epub"]);
    }
}
