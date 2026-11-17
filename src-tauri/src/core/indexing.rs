//! Searching the whole library's text: each item with text on the library shelf is indexed
//! block by block (`aoide_library::search_passages`), in the background, one at a time, never
//! while speech is being made; an item opened, or an article fetched again, is indexed from the
//! model it has. The index is dropped when the user turns it off.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aoide_library::{BookId, Library, PassageRow};
use serde::Serialize;

use super::lock;
use crate::book::BookModel;

/// The indexing rule: a change to what a row holds bumps it, and every item is indexed again.
pub const INDEX_VERSION: i64 = 1;

/// How long indexing waits before looking again while speech is being made.
#[cfg(not(test))]
const STEP_ASIDE: Duration = Duration::from_secs(2);
#[cfg(test)]
const STEP_ASIDE: Duration = Duration::from_millis(5);

/// Items a search gives at most.
pub const SEARCH_ITEMS: usize = 30;

/// Where indexing is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    /// Turned off in Settings: nothing is indexed and the index is empty.
    Off,
    /// Every item is indexed.
    #[default]
    Idle,
    Indexing,
    /// Speech is being made: indexing waits.
    SteppedAside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    TurnedOn,
    TurnedOff,
    /// An item waits to be indexed.
    Found,
    /// None waits.
    Emptied,
    /// Speech is being made.
    Busy,
}

/// The whole state machine. Events that do not apply leave the stage as it is.
pub fn next(stage: Stage, event: Event) -> Stage {
    use Event::*;
    use Stage::*;
    match (stage, event) {
        (_, TurnedOff) => Off,
        (Off, TurnedOn) => Idle,
        (Off, _) => Off,
        (Idle | SteppedAside, Found) => Indexing,
        (Indexing | SteppedAside, Emptied) => Idle,
        (Indexing | Idle, Busy) => SteppedAside,
        (stage, _) => stage,
    }
}

/// What indexing needs from the rest of the app.
pub(super) trait IndexHost: Send + Sync {
    /// The user wants the library's text searchable.
    fn indexing_on(&self) -> bool;
    /// Speech is being made (listening must not stutter).
    fn speaking(&self) -> bool;
    /// Item `id`'s text, parsed as the reader shows it, without keeping it.
    fn text_of(&self, id: &str) -> Result<Arc<BookModel>, String>;
}

/// What a step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Step {
    Indexed(String),
    /// Speech is being made: look again in a little while.
    Rest,
    /// Nothing to do until woken.
    Wait,
}

#[derive(Default)]
struct State {
    stage: Stage,
    /// Items opened (their model is at hand) to index first.
    offered: Vec<(String, Arc<BookModel>)>,
    /// Items that failed to parse in this run, not tried again until it restarts.
    failed: Vec<String>,
}

pub(super) struct Indexing {
    library: Arc<Mutex<Library>>,
    state: Mutex<State>,
    wake: tokio::sync::Notify,
}

/// Item `model`'s blocks as the index stores them: each block's text from its first sentence
/// to its last, with where each sentence starts (notes read aloud are not in the text).
pub(super) fn rows_of(model: &BookModel) -> Vec<PassageRow> {
    let mut rows = Vec::new();
    for (chapter, part) in model.chapters.iter().enumerate() {
        let chars: Vec<char> = part.text.chars().collect();
        let mut current: Option<PassageRow> = None;
        let mut end = 0;
        for segment in part.segments.iter().filter(|s| !s.range.is_empty()) {
            if segment.range.end > chars.len() {
                continue;
            }
            let block = segment.block as u32;
            if current.as_ref().is_some_and(|row| row.block != block) {
                rows.extend(current.take().map(|row| close(row, &chars, end)));
            }
            let row = current.get_or_insert_with(|| PassageRow {
                chapter: chapter as u32,
                block,
                at: segment.range.start as u32,
                text: String::new(),
                segments: Vec::new(),
            });
            row.segments
                .push((segment.range.start as u32 - row.at, segment.id.clone()));
            end = segment.range.end;
        }
        rows.extend(current.take().map(|row| close(row, &chars, end)));
    }
    rows.retain(|row| row.text.chars().any(char::is_alphanumeric));
    rows
}

fn close(mut row: PassageRow, chars: &[char], end: usize) -> PassageRow {
    row.text = chars[row.at as usize..end].iter().collect();
    row
}

impl Indexing {
    pub fn new(library: Arc<Mutex<Library>>) -> Self {
        Self {
            library,
            state: Mutex::default(),
            wake: tokio::sync::Notify::new(),
        }
    }

    #[cfg(test)]
    pub fn stage(&self) -> Stage {
        lock(&self.state).stage
    }

    fn go(&self, state: &mut State, event: Event) {
        let to = next(state.stage, event);
        if to != state.stage {
            tracing::info!(event = "index.stage", from = ?state.stage, to = ?to, "Indexing moved on");
            state.stage = to;
        }
    }

    /// Indexes the next item that needs it, unless speech is being made.
    pub fn step(&self, host: &dyn IndexHost) -> Step {
        {
            let mut state = lock(&self.state);
            if !host.indexing_on() {
                if state.stage != Stage::Off {
                    self.go(&mut state, Event::TurnedOff);
                }
                return Step::Wait;
            }
            if state.stage == Stage::Off {
                self.go(&mut state, Event::TurnedOn);
            }
        }
        let offered = lock(&self.state).offered.pop();
        let (id, model) = match offered {
            Some((id, model)) => {
                let wanted = lock(&self.library)
                    .indexed_version(&BookId(id.clone()))
                    .ok()
                    .flatten()
                    != Some(INDEX_VERSION);
                if !wanted {
                    return Step::Indexed(id);
                }
                (id, Some(model))
            }
            None => {
                let failed = lock(&self.state).failed.clone();
                let waiting = lock(&self.library)
                    .to_index(INDEX_VERSION)
                    .unwrap_or_default()
                    .into_iter()
                    .find(|id| !failed.contains(&id.0));
                let mut state = lock(&self.state);
                match waiting {
                    Some(id) => {
                        self.go(&mut state, Event::Found);
                        (id.0, None)
                    }
                    None => {
                        self.go(&mut state, Event::Emptied);
                        return Step::Wait;
                    }
                }
            }
        };
        if host.speaking() {
            let mut state = lock(&self.state);
            if let Some(model) = model {
                state.offered.push((id, model));
            }
            self.go(&mut state, Event::Busy);
            return Step::Rest;
        }
        if lock(&self.state).stage == Stage::SteppedAside {
            self.go(&mut lock(&self.state), Event::Found);
        }
        let started = Instant::now();
        // Parsed outside the library lock.
        let model = match model.map(Ok).unwrap_or_else(|| host.text_of(&id)) {
            Ok(model) => model,
            Err(error) => {
                tracing::warn!(event = "index.item", book = crate::logging::book(&id), outcome = "failed", %error, "Could not index an item");
                lock(&self.state).failed.push(id.clone());
                return Step::Indexed(id);
            }
        };
        let rows = rows_of(&model);
        let stored = lock(&self.library).index_passages(&BookId(id.clone()), INDEX_VERSION, &rows);
        match stored {
            Ok(()) => tracing::info!(
                event = "index.item",
                book = crate::logging::book(&id),
                blocks = rows.len(),
                took_ms = crate::logging::took(started),
                outcome = "ok",
                "Indexed an item"
            ),
            Err(error) => {
                tracing::warn!(event = "index.item", book = crate::logging::book(&id), outcome = "failed", %error, "Could not index an item");
                lock(&self.state).failed.push(id.clone());
            }
        }
        Step::Indexed(id)
    }

    /// Item `id` was opened and its model parsed: it is indexed from it, first, if it needs it.
    pub fn offer(&self, id: &str, model: Arc<BookModel>) {
        let mut state = lock(&self.state);
        state.offered.retain(|(offered, _)| offered != id);
        state.offered.push((id.into(), model));
        drop(state);
        self.wake.notify_one();
    }

    /// Item `id`'s text changed (an article fetched again): it is indexed again.
    pub fn changed(&self, id: &str) {
        let _ = lock(&self.library).forget_passages(&BookId(id.into()));
        let mut state = lock(&self.state);
        state.failed.retain(|failed| failed != id);
        state.offered.retain(|(offered, _)| offered != id);
        drop(state);
        self.wake.notify_one();
    }

    /// Something to index may have appeared (items added, a setting changed).
    pub fn poke(&self) {
        self.wake.notify_one();
    }

    /// Turned off: the index is dropped.
    pub fn turned_off(&self) -> Result<(), String> {
        let mut state = lock(&self.state);
        state.offered.clear();
        self.go(&mut state, Event::TurnedOff);
        drop(state);
        lock(&self.library)
            .clear_passages()
            .map_err(|e| e.to_string())
    }

    /// Waits until something may need indexing, or `rest` passes.
    pub async fn wait(&self, rest: bool) {
        if rest {
            tokio::select! {
                _ = tokio::time::sleep(STEP_ASIDE) => {}
                _ = self.wake.notified() => {}
            }
        } else {
            self.wake.notified().await;
        }
    }
}

/// An item's passages that match a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct LibraryHits {
    pub book_id: String,
    pub hits: Vec<PassageHitView>,
    /// Passages that matched beyond those given.
    pub more: u32,
}

/// A passage that matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PassageHitView {
    pub chapter: u32,
    /// The sentence that holds the first match: the reader opens there.
    pub segment_id: String,
    /// The words around the matches.
    pub snippet: String,
    /// The matches in `snippet`, as character ranges `[start, end)`.
    pub marks: Vec<[u32; 2]>,
    /// The first match in the chapter's text, as a character range: the reader marks it.
    pub found: [u32; 2],
}

impl super::Core {
    /// The passages in the library's text that hold every word of `query` (the last as the
    /// start of a word), best first, at most [`SEARCH_ITEMS`] items with three passages each.
    pub fn search_library(&self, query: &str) -> Result<Vec<LibraryHits>, String> {
        let Some(query) = aoide_library::fts_query(query) else {
            return Ok(Vec::new());
        };
        let started = Instant::now();
        let found = lock(&self.library)
            .search_passages(&query, SEARCH_ITEMS)
            .map_err(|e| e.to_string())?;
        let hits: Vec<LibraryHits> = found
            .into_iter()
            .map(|item| LibraryHits {
                book_id: item.book_id.0,
                more: item.more,
                hits: item
                    .hits
                    .into_iter()
                    .map(|hit| PassageHitView {
                        chapter: hit.chapter,
                        segment_id: hit.segment_id,
                        found: [hit.found.0, hit.found.1],
                        marks: hit
                            .marks
                            .iter()
                            .map(|&(from, to)| [from as u32, to as u32])
                            .collect(),
                        snippet: hit.snippet,
                    })
                    .collect(),
            })
            .collect();
        tracing::info!(
            event = "index.search",
            hits = hits.iter().map(|item| item.hits.len()).sum::<usize>(),
            took_ms = crate::logging::took(started),
            "Searched the library's text"
        );
        Ok(hits)
    }

    /// Indexes the library's text for as long as the app runs.
    pub(super) async fn index_loop(self: Arc<Self>) {
        loop {
            let core = Arc::clone(&self);
            let step = tokio::task::spawn_blocking(move || core.indexing.step(&*core))
                .await
                .unwrap_or(Step::Wait);
            match step {
                Step::Indexed(_) => {}
                Step::Rest => self.indexing.wait(true).await,
                Step::Wait => self.indexing.wait(false).await,
            }
        }
    }
}

impl IndexHost for super::Core {
    fn indexing_on(&self) -> bool {
        self.app_settings().search_text
    }

    fn speaking(&self) -> bool {
        self.generation.busy()
    }

    fn text_of(&self, id: &str) -> Result<Arc<BookModel>, String> {
        self.shelf.parse(id, self).map(Arc::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Layout;
    use crate::core::tests::epub_of_chapters;
    use aoide_library::{ItemKind, NewBook};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct Host {
        on: AtomicBool,
        speaking: AtomicBool,
        parsed: AtomicUsize,
        broken: Vec<String>,
    }

    impl Host {
        fn new() -> Self {
            Self {
                on: AtomicBool::new(true),
                speaking: AtomicBool::new(false),
                parsed: AtomicUsize::new(0),
                broken: Vec::new(),
            }
        }
    }

    fn model(id: &str) -> BookModel {
        let bytes = epub_of_chapters(&[
            (
                "The Quay",
                "Gulls argued over the nets. The tide came in slowly.",
            ),
            (
                "The Lamp",
                "Someone lit the lamp at dusk. The harbour café stayed open until morning.",
            ),
        ]);
        BookModel::parse(id, &bytes, &Layout::default()).unwrap()
    }

    impl IndexHost for Host {
        fn indexing_on(&self) -> bool {
            self.on.load(Ordering::SeqCst)
        }
        fn speaking(&self) -> bool {
            self.speaking.load(Ordering::SeqCst)
        }
        fn text_of(&self, id: &str) -> Result<Arc<BookModel>, String> {
            self.parsed.fetch_add(1, Ordering::SeqCst);
            if self.broken.iter().any(|b| b == id) {
                return Err("not an EPUB".into());
            }
            Ok(Arc::new(model(id)))
        }
    }

    fn library(ids: &[&str]) -> Arc<Mutex<Library>> {
        let library = Library::open_in_memory().unwrap();
        for (i, id) in ids.iter().enumerate() {
            library
                .upsert_book(
                    &NewBook {
                        id: BookId((*id).into()),
                        kind: ItemKind::Book,
                        path: format!("{id}.epub").into(),
                        title: Some("Synthetic".into()),
                        authors: vec![],
                        language: None,
                        cover_path: None,
                        error: None,
                    },
                    i as i64,
                )
                .unwrap();
        }
        Arc::new(Mutex::new(library))
    }

    fn until_waiting(indexing: &Indexing, host: &Host) -> Vec<String> {
        let mut done = Vec::new();
        for _ in 0..20 {
            match indexing.step(host) {
                Step::Indexed(id) => done.push(id),
                Step::Wait => return done,
                Step::Rest => panic!("rested"),
            }
        }
        panic!("never done");
    }

    #[test]
    fn indexing_moves_through_its_stages_on_named_events() {
        use Event::*;
        use Stage::*;
        let cases = [
            (Idle, Found, Indexing),
            (Indexing, Emptied, Idle),
            (Indexing, Busy, SteppedAside),
            (Idle, Busy, SteppedAside),
            (SteppedAside, Found, Indexing),
            (SteppedAside, Emptied, Idle),
            (Indexing, TurnedOff, Off),
            (SteppedAside, TurnedOff, Off),
            (Off, Found, Off),
            (Off, Busy, Off),
            (Off, TurnedOn, Idle),
            (Idle, TurnedOn, Idle),
        ];
        for (from, event, to) in cases {
            assert_eq!(next(from, event), to, "{from:?} on {event:?}");
        }
    }

    #[test]
    fn every_item_is_indexed_once_newest_first_and_its_words_lead_to_their_sentence() {
        let library = library(&["old", "new"]);
        let indexing = Indexing::new(Arc::clone(&library));
        let host = Host::new();
        assert_eq!(until_waiting(&indexing, &host), ["new", "old"]);
        assert_eq!(indexing.stage(), Stage::Idle);
        assert!(until_waiting(&indexing, &host).is_empty(), "nothing twice");
        let found = lock(&library)
            .search_passages(&aoide_library::fts_query("cafe").unwrap(), 10)
            .unwrap();
        assert_eq!(found.len(), 2);
        let hit = &found[0].hits[0];
        let book = model("new");
        let chapter = &book.chapters[hit.chapter as usize];
        let segment = chapter
            .segments
            .iter()
            .find(|s| s.id == hit.segment_id)
            .unwrap();
        assert!(segment.display.contains("café"), "{}", segment.display);
        let text: String = chapter
            .text
            .chars()
            .skip(hit.found.0 as usize)
            .take((hit.found.1 - hit.found.0) as usize)
            .collect();
        assert_eq!(text, "café");
    }

    #[test]
    fn it_steps_aside_while_speech_is_made_and_an_opened_item_is_not_parsed_again() {
        let library = library(&["a", "b"]);
        let indexing = Indexing::new(Arc::clone(&library));
        let host = Host::new();
        host.speaking.store(true, Ordering::SeqCst);
        indexing.offer("a", Arc::new(model("a")));
        assert_eq!(indexing.step(&host), Step::Rest);
        assert_eq!(indexing.stage(), Stage::SteppedAside);
        assert_eq!(lock(&library).passages_rows().unwrap(), 0);
        host.speaking.store(false, Ordering::SeqCst);
        assert_eq!(until_waiting(&indexing, &host), ["a", "b"]);
        assert_eq!(host.parsed.load(Ordering::SeqCst), 1, "only b was parsed");
    }

    #[test]
    fn a_new_rule_or_changed_text_indexes_again_and_turning_it_off_drops_the_index() {
        let library = library(&["a"]);
        let id = BookId("a".into());
        lock(&library)
            .index_passages(&id, INDEX_VERSION - 1, &[])
            .unwrap();
        let indexing = Indexing::new(Arc::clone(&library));
        let host = Host::new();
        assert_eq!(until_waiting(&indexing, &host), ["a"]);
        indexing.changed("a");
        assert_eq!(lock(&library).indexed_version(&id).unwrap(), None);
        assert_eq!(until_waiting(&indexing, &host), ["a"]);

        host.on.store(false, Ordering::SeqCst);
        indexing.turned_off().unwrap();
        assert_eq!(indexing.step(&host), Step::Wait);
        assert_eq!(indexing.stage(), Stage::Off);
        assert_eq!(lock(&library).passages_rows().unwrap(), 0);
        host.on.store(true, Ordering::SeqCst);
        assert_eq!(until_waiting(&indexing, &host), ["a"]);
    }

    #[test]
    fn an_item_that_cannot_be_read_is_passed_over_until_the_next_start() {
        let library = library(&["broken", "fine"]);
        let indexing = Indexing::new(Arc::clone(&library));
        let mut host = Host::new();
        host.broken = vec!["fine".into()];
        assert_eq!(until_waiting(&indexing, &host), ["fine", "broken"]);
        assert_eq!(
            lock(&library).to_index(INDEX_VERSION).unwrap(),
            [BookId("fine".into())]
        );
        // The next start tries it again.
        host.broken.clear();
        let indexing = Indexing::new(library);
        assert_eq!(until_waiting(&indexing, &host), ["fine"]);
    }
}

#[cfg(test)]
mod core_tests {
    use super::*;
    use crate::core::article_tests::{Pages, LIGHTS, LIGHTS_URL};
    use crate::core::tests::{library_folder, start, start_fetching, until};
    use aoide_tts::testing::FakeEngine;

    fn index_all(core: &crate::core::Core) {
        while let Step::Indexed(_) = core.indexing.step(core) {}
    }

    #[tokio::test]
    async fn the_library_search_finds_a_books_words_and_opens_at_their_sentence() {
        let dir = tempfile::tempdir().unwrap();
        let id = library_folder(dir.path(), "");
        let (core, _) = start(dir.path(), &FakeEngine::new());
        until("the scan", || core.list_books().is_ok_and(|b| b.len() == 1)).await;
        index_all(&core);
        let found = core.search_library("TIDE came").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].book_id, id);
        let hit = &found[0].hits[0];
        assert!(hit.snippet.contains("tide came in"), "{}", hit.snippet);
        let chapter = core.chapter(&id, hit.chapter as usize).unwrap();
        let segment = chapter
            .blocks
            .iter()
            .flat_map(|b| &b.segments)
            .find(|s| s.id == hit.segment_id)
            .unwrap();
        assert!(
            segment.words.iter().any(|w| w.starts_with("tide")),
            "{:?}",
            segment.words
        );
        assert!(core.search_library("  ").unwrap().is_empty());
        assert!(core.search_library("lantern").unwrap().is_empty());

        // Off: nothing is found, and nothing is kept.
        let mut settings = core.app_settings();
        settings.search_text = false;
        core.save_app_settings(settings).unwrap();
        assert!(core.search_library("tide").unwrap().is_empty());
        assert_eq!(core.indexing.stage(), Stage::Off);
    }

    #[tokio::test]
    async fn an_article_is_indexed_once_fetched_and_again_when_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        library_folder(dir.path(), "");
        let pages = Pages::with(&[(LIGHTS_URL, LIGHTS)]);
        let (core, _) = start_fetching(dir.path(), &FakeEngine::new(), pages.clone());
        let id = core.save_article(LIGHTS_URL).unwrap();
        until("the fetch", || {
            core.article(&id)
                .is_ok_and(|a| a.stage == crate::core::FetchStage::Ready)
        })
        .await;
        index_all(&core);
        assert_eq!(core.search_library("foggy nights").unwrap().len(), 1);
        pages.set(
            LIGHTS_URL,
            Ok(LIGHTS.replace(
                "foggy nights",
                "stormy evenings all through the long winter",
            )),
        );
        core.refetch_article(&id, crate::core::ArticleSource::Http)
            .unwrap();
        until("the second fetch", || {
            core.search_library("stormy").is_ok_and(|_| {
                index_all(&core);
                !core.search_library("stormy").unwrap().is_empty()
            })
        })
        .await;
        assert!(core.search_library("foggy").unwrap().is_empty());
    }
}
