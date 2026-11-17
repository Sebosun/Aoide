//! Importing links from another app (Pocket, Instapaper, Omnivore, Wallabag, Readwise Reader),
//! a browser's bookmarks or a list of links: the file is read (`crates/imports`), the user
//! chooses which links, and each becomes an article in the library, made from the export's
//! text when it kept one, else fetched on the article worker's background lane, so the user's
//! own saves go first and no site is hurried.
//!
//! One explicit [`Stage`], changed only by [`next`]. The import and its items are stored, so a
//! restart carries on; how far it got is counted from the items' article stages, never kept
//! apart.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use aoide_imports::{SavedLink, Source};
use aoide_library::BookId;
use serde::{Deserialize, Serialize};

use super::articles::ArticleSource;
use super::feeds::ItemShelf;
use super::fetching::Stage as Fetch;
use super::{lock, CoreEvent};

/// Where an import is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "LinkImportStage"))]
pub enum Stage {
    #[default]
    Idle,
    Reading,
    /// Read: the user chooses which links (nothing is made yet).
    Choosing,
    Fetching,
    Paused,
    /// Every link is settled (or the rest were cancelled): the summary waits to be dismissed.
    Done,
}

/// What can happen to an import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Opened,
    Read {
        found: bool,
    },
    Started,
    Pause,
    Resume,
    /// No item is waiting to be fetched any more.
    Settled,
    Cancelled,
    /// Archived copies of the failed ones were asked for.
    Retried,
    Dismissed,
}

/// The whole state machine. Events that do not apply leave the stage as it is.
pub fn next(stage: Stage, event: Event) -> Stage {
    use Event::*;
    use Stage::*;
    match (stage, event) {
        (Idle | Choosing | Done, Opened) => Reading,
        (Reading, Read { found: true }) => Choosing,
        (Reading, Read { found: false }) => Idle,
        (Choosing, Started) => Fetching,
        (Choosing, Cancelled) => Idle,
        (Fetching, Pause) => Paused,
        (Paused, Resume) => Fetching,
        (Fetching, Settled) => Done,
        (Fetching | Paused, Cancelled) => Done,
        (Done, Retried) => Fetching,
        (Done, Dismissed) => Idle,
        (stage, _) => stage,
    }
}

/// Which of the links read to import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "which")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum LinkChoice {
    All,
    /// Those not archived (read) in the other app.
    Unread,
    /// The `count` saved most recently.
    Recent {
        count: u32,
    },
}

/// An import as the window shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LinkImportView {
    pub stage: Stage,
    /// The app the file came from, as it calls itself ("Pocket").
    pub source: Option<String>,
    /// Links read (each page once).
    pub found: usize,
    /// Of those, not archived in the other app.
    pub unread: usize,
    /// Of those, already in the library.
    pub already: usize,
    /// The same page listed again.
    pub duplicates: usize,
    /// Entries that are not web pages.
    pub not_web: u32,
    /// Items being imported, and how they ended so far.
    pub of: usize,
    pub ready: usize,
    pub partial: usize,
    pub failed: usize,
}

#[derive(Default)]
struct State {
    stage: Stage,
    source: Option<Source>,
    links: Vec<SavedLink>,
    view: LinkImportView,
    import: Option<i64>,
    started: Option<Instant>,
    last_told: Option<Instant>,
}

/// The import of links: one at a time.
#[derive(Default)]
pub(super) struct LinkImports {
    state: Mutex<State>,
}

/// The largest file read as an export.
const MOST_BYTES: u64 = 64 * 1024 * 1024;

fn app_name(source: Source) -> &'static str {
    match source {
        Source::Pocket => "Pocket",
        Source::Instapaper => "Instapaper",
        Source::Omnivore => "Omnivore",
        Source::Wallabag => "Wallabag",
        Source::Readwise => "Readwise Reader",
        Source::Bookmarks => "your bookmarks",
        Source::PlainList => "a list of links",
    }
}

fn source_named(name: &str) -> Option<Source> {
    [
        Source::Pocket,
        Source::Instapaper,
        Source::Omnivore,
        Source::Wallabag,
        Source::Readwise,
        Source::Bookmarks,
        Source::PlainList,
    ]
    .into_iter()
    .find(|s| s.name() == name)
}

impl State {
    fn go(&mut self, event: Event) -> bool {
        let to = next(self.stage, event);
        if to == self.stage {
            return false;
        }
        tracing::info!(event = "link_import.stage", from = ?self.stage, to = ?to, "An import of links moved on");
        self.stage = to;
        self.view.stage = to;
        true
    }
}

impl super::Core {
    /// The import as it is now.
    pub fn link_import(&self) -> LinkImportView {
        lock(&self.link_imports.state).view.clone()
    }

    fn tell_link_import(&self, state: &mut State, now: bool) {
        let quiet = state
            .last_told
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(1));
        if now || !quiet {
            state.last_told = Some(Instant::now());
            self.emit(CoreEvent::LinkImport(state.view.clone()));
        }
    }

    /// Reads the export at `path`: which links it has, each page once. Nothing is made yet.
    pub fn read_links(&self, path: &Path) -> Result<LinkImportView, String> {
        let mut state = lock(&self.link_imports.state);
        if matches!(
            state.stage,
            Stage::Fetching | Stage::Paused | Stage::Reading
        ) {
            return Err("An import is already under way.".into());
        }
        let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if size > MOST_BYTES {
            return Err("That file is too large to be an export of links.".into());
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let source = aoide_imports::detect(&name, &bytes).ok_or(
            "Aoide cannot read this file. Choose an export from Pocket, Instapaper, Omnivore, \
             Wallabag or Readwise Reader, a browser's bookmarks, or a list of links.",
        )?;
        state.go(Event::Opened);
        let read = match aoide_imports::read(source, &bytes) {
            Ok(read) => read,
            Err(error) => {
                state.go(Event::Read { found: false });
                return Err(error.to_string());
            }
        };
        let found_in_file = read.links.len();
        let mut seen = HashSet::new();
        let links: Vec<SavedLink> = read
            .links
            .into_iter()
            .filter(|link| seen.insert(aoide_article::canonical_id(&link.url)))
            .collect();
        let library = lock(&self.library);
        let already = links
            .iter()
            .filter(|l| {
                let id = BookId(aoide_article::canonical_id(&l.url));
                library.article(&id).ok().flatten().is_some()
            })
            .count();
        drop(library);
        let duplicates = found_in_file - links.len();
        tracing::info!(
            event = "link_import.read",
            source = source.name(),
            found = links.len(),
            dropped = duplicates + read.not_web as usize,
            "Read an export of links"
        );
        let found = !links.is_empty();
        state.view = LinkImportView {
            stage: state.stage,
            source: Some(app_name(source).into()),
            found: links.len(),
            unread: links.iter().filter(|l| !l.archived).count(),
            already,
            duplicates,
            not_web: read.not_web,
            ..LinkImportView::default()
        };
        state.source = Some(source);
        state.links = links;
        state.go(Event::Read { found });
        self.tell_link_import(&mut state, true);
        if !found {
            return Err("No web addresses were found in that file.".into());
        }
        Ok(state.view.clone())
    }

    /// Imports the links chosen: each becomes an article in the library (made from the
    /// export's text when it kept one, else queued on the background lane).
    pub fn start_link_import(&self, choice: LinkChoice) -> Result<LinkImportView, String> {
        let now = super::now_ms();
        let mut state = lock(&self.link_imports.state);
        if state.stage != Stage::Choosing {
            return Err("Choose a file to import first.".into());
        }
        let mut chosen: Vec<SavedLink> = std::mem::take(&mut state.links);
        match choice {
            LinkChoice::All => {}
            LinkChoice::Unread => chosen.retain(|l| !l.archived),
            LinkChoice::Recent { count } => {
                let mut newest: Vec<usize> = (0..chosen.len()).collect();
                newest.sort_by_key(|&i| std::cmp::Reverse(chosen[i].saved_at.unwrap_or(0)));
                let keep: HashSet<usize> = newest.into_iter().take(count as usize).collect();
                chosen = chosen
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| keep.contains(i))
                    .map(|(_, l)| l)
                    .collect();
            }
        }
        let source = state.source.unwrap_or(Source::PlainList);
        let import = lock(&self.library)
            .start_link_import(source.name(), now)
            .map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        let mut to_fetch = Vec::new();
        let mut collections: HashMap<String, i64> = lock(&self.library)
            .collections()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|c| (c.name.to_lowercase(), c.id))
            .collect();
        let mut already = 0;
        for link in &chosen {
            let id = aoide_article::canonical_id(&link.url);
            let book = BookId(id.clone());
            let existing = lock(&self.library)
                .article(&book)
                .map_err(|e| e.to_string())?;
            let made = match existing {
                // Already in the library (or on a feed's shelf): it keeps its date and moves
                // to the library; fetched again unless it is ready.
                Some(row) => {
                    already += 1;
                    self.move_shelf(&id, super::feeds::ShelfEvent::Saved)?;
                    if Fetch::from_stored(&row.stage, row.reason.as_deref()) != Fetch::Ready {
                        self.articles.queue_background(&id, ArticleSource::Http)?;
                        items.push(book.clone());
                    }
                    false
                }
                None => {
                    let at = link.saved_at.unwrap_or(now);
                    match &link.content_html {
                        Some(html) => {
                            self.articles.create_from_html(
                                &link.url,
                                link.url.as_str(),
                                html.clone(),
                                ArticleSource::Import,
                                ItemShelf::Library,
                                at,
                            )?;
                        }
                        None => {
                            self.articles.create(
                                &link.url,
                                link.url.as_str(),
                                ItemShelf::Library,
                                at,
                            )?;
                            to_fetch.push(id.clone());
                        }
                    }
                    items.push(book.clone());
                    true
                }
            };
            let library = lock(&self.library);
            for label in &link.labels {
                let key = label.to_lowercase();
                let collection = match collections.get(&key) {
                    Some(id) => *id,
                    None => {
                        let id = library
                            .create_collection(label, now)
                            .map_err(|e| e.to_string())?;
                        collections.insert(key, id);
                        id
                    }
                };
                library
                    .add_to_collection(&book, collection)
                    .map_err(|e| e.to_string())?;
            }
            // Read there, archived here (only an item this import made).
            if made && link.archived {
                library
                    .set_archived(&book, Some(now))
                    .map_err(|e| e.to_string())?;
            }
        }
        lock(&self.library)
            .add_link_import_items(import, &items)
            .map_err(|e| e.to_string())?;
        for id in &to_fetch {
            self.articles.enqueue_background(id)?;
        }
        state.import = Some(import);
        state.started = Some(Instant::now());
        state.view.already = already;
        state.go(Event::Started);
        self.count_link_import(&mut state);
        drop(state);
        self.emit(CoreEvent::LibraryChanged);
        self.link_import_settled();
        Ok(self.link_import())
    }

    /// Counts the import's items by how their fetch ended.
    fn count_link_import(&self, state: &mut State) -> usize {
        let Some(import) = state.import else {
            return 0;
        };
        let library = lock(&self.library);
        let items = library.link_import_items(import).unwrap_or_default();
        let (mut ready, mut partial, mut failed, mut waiting) = (0, 0, 0, 0);
        for item in &items {
            let stage = library
                .article(item)
                .ok()
                .flatten()
                .map(|row| Fetch::from_stored(&row.stage, row.reason.as_deref()));
            match stage {
                Some(Fetch::Ready) => ready += 1,
                Some(Fetch::NeedsYou(_)) => partial += 1,
                Some(Fetch::Failed) => failed += 1,
                Some(Fetch::Queued | Fetch::Fetching) => waiting += 1,
                None => {}
            }
        }
        state.view.of = items.len();
        state.view.ready = ready;
        state.view.partial = partial;
        state.view.failed = failed;
        waiting
    }

    /// An import's fetch ended: the window hears how far it got, and the import is done once
    /// nothing waits.
    pub(super) fn link_import_settled(&self) {
        let mut state = lock(&self.link_imports.state);
        if !matches!(state.stage, Stage::Fetching | Stage::Paused) {
            return;
        }
        let waiting = self.count_link_import(&mut state);
        if waiting == 0 && state.go(Event::Settled) {
            tracing::info!(
                event = "link_import.done",
                fetched = state.view.ready,
                partial = state.view.partial,
                failed = state.view.failed,
                took_ms = state.started.map(crate::logging::took),
                "An import of links is done"
            );
            self.tell_link_import(&mut state, true);
        } else {
            self.tell_link_import(&mut state, false);
        }
    }

    pub fn pause_link_import(&self, paused: bool) -> Result<LinkImportView, String> {
        let mut state = lock(&self.link_imports.state);
        if !state.go(if paused { Event::Pause } else { Event::Resume }) {
            return Ok(state.view.clone());
        }
        if let Some(import) = state.import {
            lock(&self.library)
                .set_link_import_paused(import, paused)
                .map_err(|e| e.to_string())?;
        }
        self.articles.pause_background(paused);
        self.tell_link_import(&mut state, true);
        Ok(state.view.clone())
    }

    /// Stops the import: links never fetched are taken off the queue and their items removed;
    /// those fetched stay. Before it started, the links read are forgotten.
    pub fn cancel_link_import(&self) -> Result<LinkImportView, String> {
        let mut state = lock(&self.link_imports.state);
        if state.stage == Stage::Choosing {
            state.links.clear();
            state.go(Event::Cancelled);
            state.view = LinkImportView::default();
            self.tell_link_import(&mut state, true);
            return Ok(state.view.clone());
        }
        let Some(import) = state.import else {
            return Ok(state.view.clone());
        };
        let items: Vec<String> = lock(&self.library)
            .link_import_items(import)
            .unwrap_or_default()
            .into_iter()
            .map(|b| b.0)
            .collect();
        let unfetched = self.articles.unqueue_background(&items);
        drop(state);
        for id in &unfetched {
            self.remove_book(id)?;
        }
        let mut state = lock(&self.link_imports.state);
        self.articles.pause_background(false);
        if let Some(import) = state.import {
            let _ = lock(&self.library).set_link_import_paused(import, false);
        }
        self.count_link_import(&mut state);
        state.go(Event::Cancelled);
        self.tell_link_import(&mut state, true);
        Ok(state.view.clone())
    }

    /// Asks for an archived copy of each item that failed, on the background lane.
    pub fn retry_link_import_archived(&self) -> Result<LinkImportView, String> {
        let mut state = lock(&self.link_imports.state);
        let Some(import) = state.import.filter(|_| state.stage == Stage::Done) else {
            return Err("Nothing to try again.".into());
        };
        let items = lock(&self.library)
            .link_import_items(import)
            .unwrap_or_default();
        let mut asked = 0;
        for item in items {
            let failed = lock(&self.library)
                .article(&item)
                .ok()
                .flatten()
                .is_some_and(|row| {
                    Fetch::from_stored(&row.stage, row.reason.as_deref()) == Fetch::Failed
                });
            if failed {
                self.articles
                    .queue_background(&item.0, ArticleSource::Archive)?;
                asked += 1;
            }
        }
        if asked > 0 {
            state.go(Event::Retried);
            self.count_link_import(&mut state);
            self.tell_link_import(&mut state, true);
        }
        Ok(state.view.clone())
    }

    /// The summary was read: the import is over.
    pub fn dismiss_link_import(&self) -> Result<(), String> {
        let mut state = lock(&self.link_imports.state);
        if !state.go(Event::Dismissed) {
            return Ok(());
        }
        if let Some(import) = state.import.take() {
            lock(&self.library)
                .finish_link_import(import, super::now_ms())
                .map_err(|e| e.to_string())?;
        }
        *state = State::default();
        self.tell_link_import(&mut state, true);
        Ok(())
    }

    /// At start: an import an earlier run left unfinished carries on (paused if it was).
    pub(super) fn load_link_import(&self) {
        let Ok(Some(row)) = lock(&self.library).current_link_import() else {
            return;
        };
        let mut state = lock(&self.link_imports.state);
        state.import = Some(row.id);
        state.source = source_named(&row.source);
        state.stage = if row.paused {
            Stage::Paused
        } else {
            Stage::Fetching
        };
        state.view = LinkImportView {
            stage: state.stage,
            source: state.source.map(|s| app_name(s).to_owned()),
            ..LinkImportView::default()
        };
        state.started = Some(Instant::now());
        let waiting = self.count_link_import(&mut state);
        drop(state);
        if waiting == 0 {
            self.link_import_settled();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_import_moves_through_its_stages_on_named_events() {
        use Event::*;
        use Stage::*;
        let cases = [
            (Idle, Opened, Reading),
            (Reading, Read { found: true }, Choosing),
            (Reading, Read { found: false }, Idle),
            (Choosing, Started, Fetching),
            (Choosing, Cancelled, Idle),
            (Choosing, Opened, Reading),
            (Fetching, Pause, Paused),
            (Paused, Resume, Fetching),
            (Fetching, Settled, Done),
            (Paused, Settled, Paused),
            (Fetching, Cancelled, Done),
            (Paused, Cancelled, Done),
            (Done, Retried, Fetching),
            (Done, Dismissed, Idle),
            (Done, Opened, Reading),
            // An import under way is not replaced by reading another file.
            (Fetching, Opened, Fetching),
            (Idle, Started, Idle),
        ];
        for (from, event, to) in cases {
            assert_eq!(next(from, event), to, "{from:?} on {event:?}");
        }
    }
}

#[cfg(test)]
mod job_tests {
    use super::*;
    use crate::core::article_tests::{Pages, LIGHTS};
    use crate::core::tests::{library_folder, start_fetching, until};
    use crate::core::Core;
    use aoide_tts::testing::FakeEngine;
    use std::sync::Arc;

    fn export(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    async fn settled(core: &Core) {
        until("the import to settle", || {
            core.link_import().stage == Stage::Done
        })
        .await;
    }

    fn pages(urls: &[&str]) -> Arc<Pages> {
        let pages: Vec<(&str, &str)> = urls.iter().map(|u| (*u, LIGHTS)).collect();
        Pages::with(&pages)
    }

    fn id(url: &str) -> String {
        aoide_article::canonical_id(&url::Url::parse(url).unwrap())
    }

    #[tokio::test]
    async fn the_same_page_is_counted_once_and_links_already_saved_are_named() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages(&[]));
        core.save_article("https://quay.example/b").unwrap();
        let list = "https://quay.example/a\nhttps://quay.example/a?utm_source=x#top\n\
                    https://quay.example/b\nmailto:harbour@quay.example\n";
        let view = core
            .read_links(&export(dir.path(), "links.txt", list))
            .unwrap();
        assert_eq!(view.stage, Stage::Choosing);
        assert_eq!(view.source.as_deref(), Some("a list of links"));
        assert_eq!(
            (view.found, view.duplicates, view.not_web, view.already),
            (2, 1, 1, 1)
        );
        // A file that is no export is refused, and the links read wait.
        assert!(core
            .read_links(&export(dir.path(), "photo.png", "\u{89}PNG"))
            .is_err());
        assert_eq!(core.cancel_link_import().unwrap().stage, Stage::Idle);
    }

    #[tokio::test]
    async fn an_export_that_kept_the_page_makes_the_article_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let pages = pages(&[]);
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let json = format!(
            r#"[{{"is_archived":0,"tags":[],"title":"The Quay Lights","url":"https://quay.example/lights",
            "content":{},"created_at":"2023-01-01T12:00:00+0100"}}]"#,
            serde_json::to_string(LIGHTS).unwrap()
        );
        core.read_links(&export(dir.path(), "wallabag.json", &json))
            .unwrap();
        core.start_link_import(LinkChoice::All).unwrap();
        settled(&core).await;
        assert!(pages.asked.lock().unwrap().is_empty(), "nothing fetched");
        let view = core.article(&id("https://quay.example/lights")).unwrap();
        assert_eq!(view.via, Some(ArticleSource::Import));
        assert_eq!((core.link_import().of, core.link_import().ready), (1, 1));
    }

    #[tokio::test]
    async fn a_save_during_an_import_goes_first_and_sites_take_turns() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let urls = [
            "https://a.example/1",
            "https://a.example/2",
            "https://b.example/1",
            "https://a.example/3",
            "https://c.example/1",
        ];
        let mine = "https://mine.example/now";
        let pages = pages(&[&urls[..], &[mine]].concat());
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        core.read_links(&export(dir.path(), "links.txt", &urls.join("\n")))
            .unwrap();
        core.start_link_import(LinkChoice::All).unwrap();
        // Paused, nothing of the import is fetched; the user's save is.
        core.pause_link_import(true).unwrap();
        core.save_article(mine).unwrap();
        until("the user's save", || pages.asks() == 1).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(*pages.asked.lock().unwrap(), [mine]);
        assert_eq!(core.link_import().stage, Stage::Paused);
        core.pause_link_import(false).unwrap();
        settled(&core).await;
        assert_eq!(
            *pages.asked.lock().unwrap(),
            [
                mine,
                "https://a.example/1",
                "https://b.example/1",
                "https://a.example/2",
                "https://c.example/1",
                "https://a.example/3",
            ]
        );
        let view = core.link_import();
        assert_eq!((view.of, view.ready, view.failed), (5, 5, 0));
    }

    #[tokio::test]
    async fn after_a_restart_a_paused_import_stays_paused_and_then_carries_on_in_order() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let urls = [
            "https://a.example/1",
            "https://b.example/1",
            "https://a.example/2",
        ];
        let pages = pages(&urls);
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        core.read_links(&export(dir.path(), "links.txt", &urls.join("\n")))
            .unwrap();
        core.start_link_import(LinkChoice::All).unwrap();
        core.pause_link_import(true).unwrap();
        core.shutdown();
        drop(core);

        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        assert_eq!(core.link_import().stage, Stage::Paused);
        assert_eq!(core.link_import().of, 3);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(pages.asks(), 0);
        core.pause_link_import(false).unwrap();
        settled(&core).await;
        assert_eq!(*pages.asked.lock().unwrap(), urls);
        core.dismiss_link_import().unwrap();
        assert_eq!(core.link_import().stage, Stage::Idle);
    }

    #[tokio::test]
    async fn labels_become_collections_and_the_old_apps_dates_and_archive_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let pages = pages(&["https://quay.example/a", "https://quay.example/b"]);
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages);
        let sea = core.create_collection("sea").unwrap();
        let csv = "title,url,time_added,tags,status\n\
                   A,https://quay.example/a,1600000000,Sea|Harbours,unread\n\
                   B,https://quay.example/b,1500000000,,archive\n";
        core.read_links(&export(dir.path(), "pocket.csv", csv))
            .unwrap();
        // Only the unread one.
        core.start_link_import(LinkChoice::Unread).unwrap();
        settled(&core).await;
        let books = core.list_books().unwrap();
        let a = books
            .iter()
            .find(|b| b.id == id("https://quay.example/a"))
            .unwrap();
        assert_eq!(a.added_at, 1_600_000_000_000);
        assert!(books.iter().all(|b| b.id != id("https://quay.example/b")));
        let names: Vec<String> = core
            .collections()
            .unwrap()
            .into_iter()
            .filter(|c| a.collections.contains(&c.id))
            .map(|c| c.name)
            .collect();
        assert_eq!(names.len(), 2);
        assert!(
            a.collections.contains(&sea),
            "an existing collection is reused"
        );
        assert!(names.contains(&"Harbours".to_owned()));

        // All of them: the archived one arrives archived.
        core.dismiss_link_import().unwrap();
        core.read_links(&export(dir.path(), "pocket.csv", csv))
            .unwrap();
        core.start_link_import(LinkChoice::All).unwrap();
        settled(&core).await;
        let books = core.list_books().unwrap();
        let b = books
            .iter()
            .find(|b| b.id == id("https://quay.example/b"))
            .unwrap();
        assert!(b.archived);
        assert_eq!(core.link_import().already, 1);
    }

    #[tokio::test]
    async fn failed_links_can_try_archived_copies_and_cancel_drops_what_was_not_fetched() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let pages = pages(&["https://quay.example/a"]);
        let gone = "https://gone.example/old";
        pages.archive(
            gone,
            "https://web.archive.org/web/20200101000000id_/https://gone.example/old",
        );
        pages.set(
            "https://web.archive.org/web/20200101000000id_/https://gone.example/old",
            Ok(LIGHTS.into()),
        );
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        core.read_links(&export(
            dir.path(),
            "links.txt",
            &format!("https://quay.example/a\n{gone}\n"),
        ))
        .unwrap();
        core.start_link_import(LinkChoice::All).unwrap();
        settled(&core).await;
        assert_eq!(core.link_import().failed, 1);
        core.retry_link_import_archived().unwrap();
        settled(&core).await;
        let view = core.link_import();
        assert_eq!((view.ready, view.failed), (2, 0));
        core.dismiss_link_import().unwrap();

        // Cancelled while paused: the links never fetched leave the library.
        let more = ["https://quay.example/c", "https://quay.example/d"];
        core.read_links(&export(dir.path(), "more.txt", &more.join("\n")))
            .unwrap();
        core.start_link_import(LinkChoice::All).unwrap();
        core.pause_link_import(true).unwrap();
        let cancelled = core.cancel_link_import().unwrap();
        assert_eq!(cancelled.stage, Stage::Done);
        let books = core.list_books().unwrap();
        assert!(more.iter().all(|u| books.iter().all(|b| b.id != id(u))));
    }
}
