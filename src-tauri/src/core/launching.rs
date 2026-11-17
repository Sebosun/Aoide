//! Acting on what a start of Aoide asks for (`crate::launch`): a book file opened from the file
//! manager or the command line is shown on its page, added to the library first when it is new.

use std::path::Path;
use std::sync::{Arc, Mutex};

use aoide_library::{BookStatus, Library};

use super::error::Reporter;
use super::{lock, CoreEvent, Events, ImportResult};
use crate::launch::Request;

/// What launching asks of the rest of the app.
pub(super) trait LaunchHost {
    /// Adds the book file at `path` to the library, as a dropped file is.
    fn import(&self, path: &Path) -> ImportResult;
}

pub(super) struct Launching {
    library: Arc<Mutex<Library>>,
    events: Arc<dyn Events>,
    reporter: Reporter,
}

impl Launching {
    pub fn new(library: Arc<Mutex<Library>>, events: Arc<dyn Events>, reporter: Reporter) -> Self {
        Self {
            library,
            events,
            reporter,
        }
    }

    /// Acts on `requests`: every file is opened, and the first one's page shown.
    pub fn act(&self, requests: Vec<Request>, host: &dyn LaunchHost) {
        let mut first = None;
        for request in requests {
            match request {
                Request::OpenFile(path) => {
                    if let Some(id) = self.open_file(&path, host) {
                        first.get_or_insert(id);
                    }
                }
                Request::SaveLink(_) => {
                    // Saving a page sent from the browser is not built yet.
                    tracing::info!(
                        event = "launch.request",
                        kind = "link",
                        outcome = "ignored",
                        "A link to save was ignored"
                    );
                }
                Request::Show => {}
            }
        }
        if let Some(id) = first {
            self.emit(CoreEvent::OpenItem(id));
        }
    }

    /// The book at `path`: the library's own when its content is there already (nothing is
    /// copied), else the one it is imported as.
    fn open_file(&self, path: &Path, host: &dyn LaunchHost) -> Option<String> {
        let file = crate::logging::file(path);
        if let Some(id) = self.present(path) {
            tracing::info!(event = "launch.request", kind = "file", outcome = "present", %file, book = crate::logging::book(&id), "Opening a book already in the library");
            return Some(id);
        }
        let result = host.import(path);
        let Some(id) = result.book_id else {
            let error = result.error.unwrap_or_else(|| "it was not added".into());
            self.reporter
                .report("launch.open", &format!("Could not open {file}"), error);
            return None;
        };
        tracing::info!(event = "launch.request", kind = "file", outcome = "imported", %file, book = crate::logging::book(&id), "Opening a book just added");
        Some(id)
    }

    /// The id of the book in the library with the content of the file at `path`, if it is
    /// there and its file is present and readable.
    fn present(&self, path: &Path) -> Option<String> {
        let id = aoide_library::content_hash(&std::fs::read(path).ok()?);
        let record = lock(&self.library).book(&id).ok().flatten()?;
        (record.status == BookStatus::Present).then_some(id.0)
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
}
