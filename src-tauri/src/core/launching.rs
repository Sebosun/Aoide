//! Acting on what a start of Aoide asks for (`crate::launch`): a book file opened from the file
//! manager or the command line is shown on its page, added to the library first when it is new;
//! a page sent from the browser (`aoide://save?url=…`) is saved as an article and told.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_library::{BookId, BookStatus, Library};
use serde::Serialize;
use url::Url;

use super::error::Reporter;
use super::{lock, CoreEvent, Events, ImportResult};
use crate::launch::Request;

/// What launching asks of the rest of the app.
pub(super) trait LaunchHost {
    /// Adds the book file at `path` to the library, as a dropped file is.
    fn import(&self, path: &Path) -> ImportResult;
    /// Saves the page at `url` as an article, and nothing else: a feed's address is not
    /// subscribed to.
    fn save_link(&self, url: &Url) -> Result<SavedLink, String>;
}

/// A page sent from the browser, saved: what the window tells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SavedLink {
    /// The item it is.
    pub id: String,
    /// Its title, or its site until it is fetched.
    pub title: String,
    /// Whether it was not in the library before: only then does Undo (removing it) undo the
    /// save. An item a feed lists goes back to the feed shelf.
    pub new_to_library: bool,
}

/// Any web page can send a page to save, so saves are let through one a second at most…
pub const SAVE_EVERY: Duration = Duration::from_secs(1);
/// …and no more than this many are held while the window starts.
pub const HELD_SAVES: usize = 20;

/// Why a page sent from the browser was not saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dropped {
    /// Less than [`SAVE_EVERY`] after the last one.
    TooSoon,
    /// [`HELD_SAVES`] are held already.
    TooMany,
}

impl Dropped {
    fn name(self) -> &'static str {
        match self {
            Dropped::TooSoon => "too_soon",
            Dropped::TooMany => "too_many",
        }
    }
}

/// Whether a page sent at `now` is saved, with `held` saves waiting for the window and the
/// last one let through at `last`.
fn admit(held: usize, last: Option<Instant>, now: Instant) -> Result<(), Dropped> {
    if held >= HELD_SAVES {
        return Err(Dropped::TooMany);
    }
    match last {
        Some(last) if now.saturating_duration_since(last) < SAVE_EVERY => Err(Dropped::TooSoon),
        _ => Ok(()),
    }
}

/// Whether the window can be asked to open an item yet.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Window {
    /// Starting: what it is to open, the files that could not be opened and the pages saved
    /// are held, oldest first. Nothing listens yet, so a problem told now would be lost.
    Waiting {
        items: Vec<String>,
        failed: Vec<Failure>,
        saved: Vec<SavedLink>,
    },
    /// Listening: items are sent, and failures and saves told, as they come.
    Ready,
}

/// A book file that could not be opened: what the user is told.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Failure {
    /// What failed ("Could not open Torn.epub").
    doing: String,
    error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    /// An item is to be opened.
    Opened(String),
    /// A book file could not be opened, or a page could not be saved.
    Failed(Failure),
    /// A page sent from the browser was saved.
    Saved(SavedLink),
    /// The window asked for what was held.
    WindowReady,
}

/// What to do with the items to open.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Deliver {
    Nothing,
    /// Send this one to the window now.
    Send(String),
    /// Tell the user now.
    Tell(Failure),
    /// Tell the window this page was saved.
    Announce(SavedLink),
    /// Tell these failures and saves, and hand these items to the window, which asked.
    Hand {
        items: Vec<String>,
        failed: Vec<Failure>,
        saved: Vec<SavedLink>,
    },
}

fn next(window: Window, event: Event) -> (Window, Deliver) {
    match (window, event) {
        (
            Window::Waiting {
                mut items,
                failed,
                saved,
            },
            Event::Opened(id),
        ) => {
            items.push(id);
            (
                Window::Waiting {
                    items,
                    failed,
                    saved,
                },
                Deliver::Nothing,
            )
        }
        (
            Window::Waiting {
                items,
                mut failed,
                saved,
            },
            Event::Failed(failure),
        ) => {
            failed.push(failure);
            (
                Window::Waiting {
                    items,
                    failed,
                    saved,
                },
                Deliver::Nothing,
            )
        }
        (
            Window::Waiting {
                items,
                failed,
                mut saved,
            },
            Event::Saved(link),
        ) => {
            saved.push(link);
            (
                Window::Waiting {
                    items,
                    failed,
                    saved,
                },
                Deliver::Nothing,
            )
        }
        (
            Window::Waiting {
                items,
                failed,
                saved,
            },
            Event::WindowReady,
        ) => (
            Window::Ready,
            Deliver::Hand {
                items,
                failed,
                saved,
            },
        ),
        (Window::Ready, Event::Opened(id)) => (Window::Ready, Deliver::Send(id)),
        (Window::Ready, Event::Failed(failure)) => (Window::Ready, Deliver::Tell(failure)),
        (Window::Ready, Event::Saved(link)) => (Window::Ready, Deliver::Announce(link)),
        (Window::Ready, Event::WindowReady) => (
            Window::Ready,
            Deliver::Hand {
                items: Vec::new(),
                failed: Vec::new(),
                saved: Vec::new(),
            },
        ),
    }
}

impl Window {
    fn name(&self) -> &'static str {
        match self {
            Window::Waiting { .. } => "waiting",
            Window::Ready => "ready",
        }
    }
}

pub(super) struct Launching {
    library: Arc<Mutex<Library>>,
    events: Arc<dyn Events>,
    reporter: Reporter,
    window: Mutex<Window>,
    /// When the last page sent from the browser was let through.
    last_save: Mutex<Option<Instant>>,
}

impl Launching {
    pub fn new(library: Arc<Mutex<Library>>, events: Arc<dyn Events>, reporter: Reporter) -> Self {
        Self {
            library,
            events,
            reporter,
            window: Mutex::new(Window::Waiting {
                items: Vec::new(),
                failed: Vec::new(),
                saved: Vec::new(),
            }),
            last_save: Mutex::new(None),
        }
    }

    /// Acts on `requests`: every file is opened, and the first one's page shown; every page
    /// sent from the browser is saved.
    pub fn act(&self, requests: Vec<Request>, host: &dyn LaunchHost) {
        self.act_at(requests, host, Instant::now());
    }

    /// [`Launching::act`] at `now`.
    fn act_at(&self, requests: Vec<Request>, host: &dyn LaunchHost, now: Instant) {
        let mut first = None;
        for request in requests {
            match request {
                Request::OpenFile(path) => {
                    if let Some(id) = self.open_file(&path, host) {
                        first.get_or_insert(id);
                    }
                }
                Request::SaveLink(url) => self.save_link(&url, host, now),
                Request::Show => {}
            }
        }
        if let Some(id) = first {
            if let Deliver::Send(id) = self.apply(Event::Opened(id)) {
                self.emit(CoreEvent::OpenItem(id));
            }
        }
    }

    /// The book at `path`: the library's own when its content is there already (nothing is
    /// copied), else the one it is imported as.
    fn open_file(&self, path: &Path, host: &dyn LaunchHost) -> Option<String> {
        let file = crate::logging::file(path);
        let id = std::fs::read(path)
            .ok()
            .map(|bytes| aoide_library::content_hash(&bytes));
        if let Some(id) = id.as_ref().filter(|id| self.present(id)) {
            tracing::info!(event = "launch.request", kind = "file", outcome = "present", %file, book = crate::logging::book(&id.0), "Opening a book already in the library");
            return Some(id.0.clone());
        }
        // A book removed earlier left its file in the library folder: that one is taken back.
        let source = id
            .as_ref()
            .and_then(|id| self.own_file(id))
            .unwrap_or_else(|| path.to_owned());
        let result = host.import(&source);
        let Some(id) = result.book_id else {
            let failure = Failure {
                doing: format!("Could not open {file}"),
                error: result.error.unwrap_or_else(|| "it was not added".into()),
            };
            if let Deliver::Tell(failure) = self.apply(Event::Failed(failure)) {
                self.tell(failure);
            }
            return None;
        };
        tracing::info!(event = "launch.request", kind = "file", outcome = "imported", %file, book = crate::logging::book(&id), "Opening a book just added");
        Some(id)
    }

    /// Saves the page at `url` sent at `now`, and tells the window, once it listens; drops it
    /// when it comes too soon after the last or too many are held.
    fn save_link(&self, url: &Url, host: &dyn LaunchHost, now: Instant) {
        if let Err(why) = self.admit(now) {
            tracing::warn!(
                event = "launch.save_link",
                outcome = "dropped",
                why = why.name(),
                "Dropped a page sent from the browser"
            );
            return;
        }
        match host.save_link(url) {
            Ok(saved) => {
                tracing::info!(
                    event = "launch.save_link",
                    outcome = "saved",
                    new = saved.new_to_library,
                    book = crate::logging::book(&saved.id),
                    "Saved a page sent from the browser"
                );
                if let Deliver::Announce(saved) = self.apply(Event::Saved(saved)) {
                    self.emit(CoreEvent::LinkSaved(saved));
                }
            }
            Err(error) => {
                tracing::warn!(
                    event = "launch.save_link",
                    outcome = "failed",
                    why = "error",
                    %error,
                    "Could not save a page sent from the browser"
                );
                let failure = Failure {
                    doing: "Could not save the page".into(),
                    error,
                };
                if let Deliver::Tell(failure) = self.apply(Event::Failed(failure)) {
                    self.tell(failure);
                }
            }
        }
    }

    /// Lets a save at `now` through, or says why not.
    fn admit(&self, now: Instant) -> Result<(), Dropped> {
        let held = match &*lock(&self.window) {
            Window::Waiting { saved, .. } => saved.len(),
            Window::Ready => 0,
        };
        let mut last = lock(&self.last_save);
        admit(held, *last, now)?;
        *last = Some(now);
        Ok(())
    }

    /// Whether book `id` is in the library with its file present.
    fn present(&self, id: &BookId) -> bool {
        lock(&self.library)
            .book(id)
            .ok()
            .flatten()
            .is_some_and(|record| record.status == BookStatus::Present)
    }

    /// A file in the library folder with the content of book `id`, as the last scan found it.
    fn own_file(&self, id: &BookId) -> Option<PathBuf> {
        let scanned = lock(&self.library).scanned_files().ok()?;
        scanned
            .into_iter()
            .find(|file| file.id == *id && file.path.is_file())
            .map(|file| file.path)
    }

    /// The items asked to be opened before the window was ready, which it opens now; the files
    /// that could not be opened and the pages saved meanwhile are told now. From now on all are
    /// sent as they come.
    pub fn take_open_requests(&self) -> Vec<String> {
        match self.apply(Event::WindowReady) {
            Deliver::Hand {
                items,
                failed,
                saved,
            } => {
                for failure in failed {
                    self.tell(failure);
                }
                for link in saved {
                    self.emit(CoreEvent::LinkSaved(link));
                }
                items
            }
            _ => Vec::new(),
        }
    }

    fn tell(&self, failure: Failure) {
        self.reporter
            .report("launch.open", &failure.doing, failure.error);
    }

    fn apply(&self, event: Event) -> Deliver {
        let mut window = lock(&self.window);
        let from = window.name();
        let (to, deliver) = next(std::mem::replace(&mut *window, Window::Ready), event);
        *window = to;
        if from != window.name() {
            tracing::info!(
                event = "launch.window",
                from,
                to = window.name(),
                "The window opens what it is asked to"
            );
        }
        deliver
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
}

impl LaunchHost for super::Core {
    fn import(&self, path: &Path) -> ImportResult {
        let mut results = self.import_files(vec![path.to_string_lossy().into_owned()]);
        results.pop().unwrap_or_else(|| ImportResult {
            path: path.to_string_lossy().into_owned(),
            book_id: None,
            error: Some("it was not added".into()),
        })
    }

    fn save_link(&self, url: &Url) -> Result<SavedLink, String> {
        let id = aoide_article::canonical_id(url);
        let new_to_library = self.feeds.shelf_of(&id) != Some(super::feeds::ItemShelf::Library);
        let id = self.save_article(url.as_str())?;
        let title = lock(&self.library)
            .book(&BookId(id.clone()))
            .ok()
            .flatten()
            .and_then(|record| record.title)
            .unwrap_or_else(|| url.host_str().unwrap_or_default().to_owned());
        Ok(SavedLink {
            id,
            title,
            new_to_library,
        })
    }
}

impl super::Core {
    /// Acts on what a start of Aoide asks for (blocks while a new book is added).
    pub fn launch(&self, requests: Vec<Request>) {
        self.launching.act(requests, self);
    }

    /// The items asked to be opened before the window was ready; from now on they are sent.
    pub fn take_open_requests(&self) -> Vec<String> {
        self.launching.take_open_requests()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::super::shelf::Shelf;
    use super::super::tests::{epub, Recorder};
    use super::*;

    /// A library folder with its shelf, and launching beside it.
    struct World {
        _dir: tempfile::TempDir,
        root: PathBuf,
        shelf: Shelf,
        events: Arc<Recorder>,
        launching: Launching,
    }

    impl LaunchHost for Shelf {
        fn import(&self, path: &Path) -> ImportResult {
            self.import_files(vec![path.to_string_lossy().into_owned()])
                .remove(0)
        }

        fn save_link(&self, _url: &Url) -> Result<SavedLink, String> {
            Err("No articles in this test.".into())
        }
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
            Arc::clone(&library),
            root.join("Books"),
            root.join("covers"),
            Reporter::default(),
            events.clone(),
        );
        let launching = Launching::new(library, events.clone(), Reporter::new(events.clone()));
        assert_eq!(launching.take_open_requests(), [] as [String; 0]);
        World {
            _dir: dir,
            root,
            shelf,
            events,
            launching,
        }
    }

    impl World {
        fn books(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.root.join("Books"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        fn opened(&self) -> Vec<serde_json::Value> {
            self.events.payloads(CoreEvent::OPEN_ITEM)
        }
    }

    #[test]
    fn a_book_already_in_the_library_under_another_name_opens_without_a_second_copy() {
        let world = world();
        std::fs::write(world.root.join("Books/Quay.epub"), epub()).unwrap();
        world.shelf.rescan().unwrap();
        let downloaded = world.root.join("Downloads/quay-lights.epub");
        std::fs::write(&downloaded, epub()).unwrap();
        let id = aoide_library::content_hash(&epub()).0;

        world
            .launching
            .act(vec![Request::OpenFile(downloaded)], &world.shelf);

        assert_eq!(world.books(), ["Quay.epub"]);
        assert_eq!(world.opened(), [serde_json::json!(id)]);
    }

    #[test]
    fn a_book_removed_earlier_comes_back_from_its_own_file_without_a_second_copy() {
        let world = world();
        std::fs::write(world.root.join("Books/Quay.epub"), epub()).unwrap();
        world.shelf.rescan().unwrap();
        let id = aoide_library::content_hash(&epub()).0;
        world.shelf.remove(&id).unwrap();
        let downloaded = world.root.join("Downloads/quay-lights.epub");
        std::fs::write(&downloaded, epub()).unwrap();

        world
            .launching
            .act(vec![Request::OpenFile(downloaded)], &world.shelf);

        assert_eq!(world.books(), ["Quay.epub"]);
        assert!(!world.shelf.ignored().contains(&id));
        assert_eq!(world.opened(), [serde_json::json!(id)]);
    }

    #[test]
    fn a_new_book_is_added_and_then_opened() {
        let world = world();
        let downloaded = world.root.join("Downloads/Quay.epub");
        std::fs::write(&downloaded, epub()).unwrap();
        let id = aoide_library::content_hash(&epub()).0;

        world
            .launching
            .act(vec![Request::OpenFile(downloaded)], &world.shelf);

        assert_eq!(world.books(), ["Quay.epub"]);
        assert_eq!(world.opened(), [serde_json::json!(id)]);
        assert_eq!(world.events.count(CoreEvent::PROBLEM), 0);
    }

    #[test]
    fn a_file_that_cannot_be_added_is_a_problem_and_opens_nothing() {
        let world = world();
        let broken = world.root.join("Downloads/Torn.epub");
        std::fs::write(&broken, b"not a book").unwrap();

        world
            .launching
            .act(vec![Request::OpenFile(broken)], &world.shelf);

        assert_eq!(world.opened(), [] as [serde_json::Value; 0]);
        let problems = world.events.payloads(CoreEvent::PROBLEM);
        assert_eq!(problems.len(), 1);
        let message = problems[0]["message"].as_str().unwrap();
        assert!(message.starts_with("Could not open Torn.epub"), "{message}");
    }

    #[test]
    fn with_several_files_all_are_added_and_the_first_opened() {
        let world = world();
        let lamp = super::super::tests::epub_with(
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>The lamp burned.</p></body></html>".into(),
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Until morning.</p></body></html>".into(),
        );
        let broken = world.root.join("Downloads/Torn.epub");
        std::fs::write(&broken, b"not a book").unwrap();
        for (name, bytes) in [("Quay.epub", epub()), ("Lamp.epub", lamp.clone())] {
            std::fs::write(world.root.join("Downloads").join(name), bytes).unwrap();
        }

        world.launching.act(
            vec![
                Request::OpenFile(broken),
                Request::OpenFile(world.root.join("Downloads/Lamp.epub")),
                Request::OpenFile(world.root.join("Downloads/Quay.epub")),
            ],
            &world.shelf,
        );

        assert_eq!(world.books(), ["Lamp.epub", "Quay.epub", "Torn.epub"]);
        assert_eq!(
            world.opened(),
            [serde_json::json!(aoide_library::content_hash(&lamp).0)]
        );
    }

    #[test]
    fn what_comes_before_the_window_is_ready_is_held_until_it_asks() {
        let dir = tempfile::tempdir().unwrap();
        let library = Arc::new(Mutex::new(Library::open_in_memory().unwrap()));
        let events = Arc::new(Recorder::default());
        let launching = Launching::new(library, events.clone(), Reporter::default());
        let (quay, lamp) = (dir.path().join("Quay.epub"), dir.path().join("Lamp.epub"));
        let host = Imports(vec![(quay.clone(), "quay"), (lamp.clone(), "lamp")]);

        launching.act(vec![Request::OpenFile(quay)], &host);
        launching.act(vec![Request::Show], &host);
        assert_eq!(events.count(CoreEvent::OPEN_ITEM), 0);

        assert_eq!(launching.take_open_requests(), ["quay"]);
        launching.act(vec![Request::OpenFile(lamp)], &host);
        assert_eq!(
            events.payloads(CoreEvent::OPEN_ITEM),
            [serde_json::json!("lamp")]
        );
        // A window loaded again finds nothing held.
        assert_eq!(launching.take_open_requests(), [] as [String; 0]);
    }

    #[test]
    fn a_file_that_cannot_be_added_before_the_window_is_ready_is_told_once_it_asks() {
        let dir = tempfile::tempdir().unwrap();
        let library = Arc::new(Mutex::new(Library::open_in_memory().unwrap()));
        let events = Arc::new(Recorder::default());
        let launching = Launching::new(library, events.clone(), Reporter::new(events.clone()));
        let host = Imports(Vec::new());

        launching.act(vec![Request::OpenFile(dir.path().join("Torn.epub"))], &host);
        // Nothing listens yet: told now, it would be lost.
        assert_eq!(events.count(CoreEvent::PROBLEM), 0);

        assert_eq!(launching.take_open_requests(), [] as [String; 0]);
        let problems = events.payloads(CoreEvent::PROBLEM);
        assert_eq!(problems.len(), 1);
        let message = problems[0]["message"].as_str().unwrap();
        assert!(message.starts_with("Could not open Torn.epub"), "{message}");
    }

    /// Imports each of its files as the id it gives it.
    struct Imports(Vec<(PathBuf, &'static str)>);

    impl LaunchHost for Imports {
        fn import(&self, path: &Path) -> ImportResult {
            let id = self
                .0
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, id)| id.to_string());
            ImportResult {
                path: path.to_string_lossy().into_owned(),
                book_id: id,
                error: None,
            }
        }

        fn save_link(&self, url: &Url) -> Result<SavedLink, String> {
            Ok(saved(url.path()))
        }
    }

    fn saved(id: &str) -> SavedLink {
        SavedLink {
            id: id.into(),
            title: id.into(),
            new_to_library: true,
        }
    }

    /// Saves every page it is sent, and keeps their addresses.
    #[derive(Default)]
    struct Saves(Mutex<Vec<String>>);

    impl LaunchHost for Saves {
        fn import(&self, path: &Path) -> ImportResult {
            Imports(Vec::new()).import(path)
        }

        fn save_link(&self, url: &Url) -> Result<SavedLink, String> {
            lock(&self.0).push(url.path().to_owned());
            Ok(saved(url.path()))
        }
    }

    #[test]
    fn pages_are_saved_at_most_one_a_second_and_twenty_held_before_the_window_listens() {
        use std::time::{Duration, Instant};
        let library = Arc::new(Mutex::new(Library::open_in_memory().unwrap()));
        let events = Arc::new(Recorder::default());
        let launching = Launching::new(library, events.clone(), Reporter::default());
        let host = Saves::default();
        let start = Instant::now();
        let send = |n: u64, at_ms: u64| {
            let url = Url::parse(&format!("https://quay.example/{n}")).unwrap();
            let at = start + Duration::from_millis(at_ms);
            launching.act_at(vec![Request::SaveLink(url)], &host, at);
        };
        let told = || -> Vec<String> {
            events
                .payloads(CoreEvent::LINK_SAVED)
                .iter()
                .map(|saved| saved["id"].as_str().unwrap().to_owned())
                .collect()
        };

        send(0, 0);
        send(1, 999);
        send(2, 1_000);
        for n in 3..=20 {
            send(n, n * 1_000);
        }
        // Twenty are held: the next is dropped, not saved.
        send(21, 21_000);
        let paths = |ns: &[u64]| -> Vec<String> { ns.iter().map(|n| format!("/{n}")).collect() };
        let held: Vec<u64> = [0].into_iter().chain(2..=20).collect();
        assert_eq!(*lock(&host.0), paths(&held));
        assert_eq!(told(), [] as [String; 0]);

        assert_eq!(launching.take_open_requests(), [] as [String; 0]);
        assert_eq!(told(), paths(&held));

        // Listening, the window is told each as it comes, one a second.
        send(22, 22_000);
        send(23, 22_500);
        send(24, 23_500);
        assert_eq!(told()[held.len()..], paths(&[22, 24]));
    }

    #[test]
    fn a_save_is_let_through_only_a_second_after_the_last_and_with_room_to_hold_it() {
        use std::time::{Duration, Instant};
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        let cases = [
            (0, None, at(0), Ok(())),
            (0, Some(at(0)), at(999), Err(Dropped::TooSoon)),
            (0, Some(at(0)), at(1_000), Ok(())),
            (HELD_SAVES - 1, Some(at(0)), at(5_000), Ok(())),
            (HELD_SAVES, Some(at(0)), at(5_000), Err(Dropped::TooMany)),
            (HELD_SAVES, None, at(0), Err(Dropped::TooMany)),
        ];
        for (held, last, now, verdict) in cases {
            assert_eq!(
                admit(held, last, now),
                verdict,
                "{held} held, last {last:?}"
            );
        }
    }

    #[test]
    fn the_window_state_moves_on_named_events() {
        use Event::*;
        use Window::*;
        let failure = || Failure {
            doing: "Could not open Torn.epub".into(),
            error: "not a book".into(),
        };
        let ids = |ids: &[&str]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let held = |items: &[&str], failed: Vec<Failure>| Waiting {
            items: ids(items),
            failed,
            saved: vec![],
        };
        let cases = [
            (
                held(&[], vec![]),
                Opened("a".into()),
                held(&["a"], vec![]),
                Deliver::Nothing,
            ),
            (
                held(&["a"], vec![]),
                Failed(failure()),
                held(&["a"], vec![failure()]),
                Deliver::Nothing,
            ),
            (
                held(&["a"], vec![failure()]),
                Opened("b".into()),
                held(&["a", "b"], vec![failure()]),
                Deliver::Nothing,
            ),
            (
                held(&["a", "b"], vec![failure()]),
                WindowReady,
                Ready,
                Deliver::Hand {
                    items: ids(&["a", "b"]),
                    failed: vec![failure()],
                    saved: vec![],
                },
            ),
            (
                held(&["a"], vec![]),
                Saved(saved("s")),
                Waiting {
                    items: ids(&["a"]),
                    failed: vec![],
                    saved: vec![saved("s")],
                },
                Deliver::Nothing,
            ),
            (
                Waiting {
                    items: vec![],
                    failed: vec![],
                    saved: vec![saved("s"), saved("t")],
                },
                WindowReady,
                Ready,
                Deliver::Hand {
                    items: vec![],
                    failed: vec![],
                    saved: vec![saved("s"), saved("t")],
                },
            ),
            (Ready, Opened("c".into()), Ready, Deliver::Send("c".into())),
            (Ready, Failed(failure()), Ready, Deliver::Tell(failure())),
            (
                Ready,
                Saved(saved("s")),
                Ready,
                Deliver::Announce(saved("s")),
            ),
            (
                Ready,
                WindowReady,
                Ready,
                Deliver::Hand {
                    items: vec![],
                    failed: vec![],
                    saved: vec![],
                },
            ),
        ];
        for (from, event, to, deliver) in cases {
            assert_eq!(
                next(from.clone(), event.clone()),
                (to, deliver),
                "{from:?} on {event:?}"
            );
        }
    }
}

#[cfg(test)]
mod through_core {
    use aoide_tts::testing::FakeEngine;
    use url::Url;

    use super::super::article_tests::{Pages, LIGHTS, LIGHTS_URL};
    use super::super::fetching::Stage;
    use super::super::tests::{library_folder, start_fetching, until};
    use super::super::{Core, CoreEvent};
    use crate::launch::Request;

    fn listed(core: &Core, id: &str) -> bool {
        core.list_books().unwrap().iter().any(|b| b.id == id)
    }

    #[tokio::test]
    async fn a_page_sent_from_the_browser_is_saved_and_told_once_the_window_listens() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
        let (core, events) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let lights = Url::parse(LIGHTS_URL).unwrap();
        let id = aoide_article::canonical_id(&lights);

        core.launch(vec![Request::SaveLink(lights.clone())]);

        assert!(listed(&core, &id));
        // Nothing listens yet: told now, it would be lost.
        assert_eq!(events.count(CoreEvent::LINK_SAVED), 0);
        // A saved page is told, not opened.
        assert_eq!(core.take_open_requests(), [] as [String; 0]);
        assert_eq!(
            events.payloads(CoreEvent::LINK_SAVED),
            [serde_json::json!({ "id": id, "title": "quay.example", "newToLibrary": true })]
        );
        until("the page is fetched", || {
            core.article(&id).is_ok_and(|a| a.stage == Stage::Ready)
        })
        .await;

        // Sent again, it is the same item, already in the library: nothing to undo.
        let later = std::time::Instant::now() + super::SAVE_EVERY;
        core.launching
            .act_at(vec![Request::SaveLink(lights)], &*core, later);
        assert_eq!(
            events.payloads(CoreEvent::LINK_SAVED)[1],
            serde_json::json!({ "id": id, "title": "The Quay Lights", "newToLibrary": false })
        );
        assert_eq!(pages.asks(), 1);
    }

    #[tokio::test]
    async fn a_feed_sent_from_the_browser_is_saved_as_a_page_and_not_subscribed() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let feed = "https://harbour.example/feed.xml";
        let pages = Pages::with(&[]);
        pages.feed(
            feed,
            "<?xml version=\"1.0\"?><rss version=\"2.0\"><channel><title>Harbour Notes</title>\
             <link>https://harbour.example/</link></channel></rss>",
            "v1",
        );
        let (core, events) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let url = Url::parse(feed).unwrap();
        let id = aoide_article::canonical_id(&url);
        core.take_open_requests();

        core.launch(vec![Request::SaveLink(url)]);

        assert!(listed(&core, &id));
        assert!(core.article(&id).is_ok(), "an article");
        assert!(core.feeds().unwrap().is_empty(), "not subscribed");
        assert_eq!(pages.feed_asks(feed), 0);
        assert_eq!(events.count(CoreEvent::LINK_SAVED), 1);
    }
}
