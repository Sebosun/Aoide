//! Whole-book preparation: books queued to have all their audio made ahead of time.
//!
//! Each book has one explicit [`Stage`], changed only by [`next`] in response to an [`Event`].
//! The queue order, the paused choice and the "all audio made" mark are saved in the library,
//! so the queue survives restarts; plans are rebuilt in each run.

use std::collections::{BTreeSet, HashMap, HashSet};

use aoide_generation::PlanItem;
use aoide_library::Library;
use aoide_tts::SynthesisRequest;
use serde::Serialize;

const QUEUE_KEY: &str = "prepare.queue";
const PAUSED_KEY: &str = "prepare.paused";
/// Books queued for some chapters only, with those chapters; the rest are whole books.
const CHAPTERS_KEY: &str = "prepare.chapters";

fn done_key(id: &str) -> String {
    format!("book.{id}.prepared")
}

/// Where a book is in whole-book preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "PrepareState"))]
pub enum Stage {
    /// Never sent to the UI: a book that is not prepared has no preparation view.
    #[cfg_attr(test, ts(skip))]
    NotPrepared,
    /// Queued, with no plan in this run yet (for example left from an earlier run).
    Waiting,
    /// Planned, behind the book being prepared.
    Queued,
    /// Planned and first in line: its audio is being made.
    Preparing,
    /// Kept in the queue, but not made until resumed.
    Paused,
    /// All of its audio is made.
    Done,
}

/// What can happen to a book's preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Enqueue,
    Planned,
    /// Its plan no longer matches (voice, settings or engine changed).
    PlanDropped,
    Pause,
    Resume,
    /// Became first in line among planned books.
    Promote,
    /// Another planned book is ahead of it now.
    Demote,
    /// Every speakable segment is made.
    Finish,
    /// Some of its finished audio was deleted.
    AudioLost,
    Cancel,
}

/// The whole state machine. Events that do not apply leave the stage as it is.
pub fn next(stage: Stage, event: Event) -> Stage {
    use Event::*;
    use Stage::*;
    match (stage, event) {
        (NotPrepared | Done, Enqueue) => Waiting,
        (Waiting, Planned) => Queued,
        (Queued | Preparing, PlanDropped) => Waiting,
        (Waiting | Queued | Preparing, Pause) => Paused,
        (Paused, Resume) => Waiting,
        (Queued, Promote) => Preparing,
        (Preparing, Demote) => Queued,
        (Queued | Preparing, Finish) => Done,
        (Done, AudioLost) => NotPrepared,
        (Waiting | Queued | Preparing | Paused, Cancel) => NotPrepared,
        (stage, _) => stage,
    }
}

/// A book's generation plan: one item and request per playlist entry.
pub struct Plan {
    pub items: Vec<PlanItem>,
    pub requests: Vec<Option<SynthesisRequest>>,
}

impl Plan {
    pub fn mark_cached(&mut self, key: &str, duration_ms: u32) {
        for item in self.items.iter_mut().filter(|i| i.key == key) {
            item.cached = true;
            item.duration_ms = duration_ms;
        }
    }

    /// Leaves out every item whose playlist index `keep` rejects: it is never made and does
    /// not count towards progress (a book prepared for some chapters only).
    pub fn only(&mut self, keep: impl Fn(usize) -> bool) {
        for (index, item) in self.items.iter_mut().enumerate() {
            if !keep(index) {
                item.speakable = false;
            }
        }
    }

    pub fn mark_lost(&mut self, keys: &HashSet<String>) {
        for item in self.items.iter_mut().filter(|i| keys.contains(&i.key)) {
            item.cached = false;
        }
    }

    /// (made, total) speakable segments.
    pub fn progress(&self) -> (usize, usize) {
        let speakable = self.items.iter().filter(|i| i.speakable);
        let total = speakable.clone().count();
        (speakable.filter(|i| i.cached).count(), total)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PrepareView {
    pub state: Stage,
    pub done: usize,
    pub total: usize,
    /// Place in the queue (0 is first); `None` once done.
    pub position: Option<usize>,
    /// The chapters being prepared, in order; `None` for the whole book.
    pub chapters: Option<Vec<usize>>,
}

/// What of a book is being prepared.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Scope {
    Book,
    /// Only these chapters. Finishing them does not make the book "ready offline".
    Chapters(BTreeSet<usize>),
}

impl Scope {
    fn chapters(&self) -> Option<Vec<usize>> {
        match self {
            Scope::Book => None,
            Scope::Chapters(chapters) => Some(chapters.iter().copied().collect()),
        }
    }
}

struct Entry {
    id: String,
    scope: Scope,
    stage: Stage,
    /// Present exactly while the stage is `Queued` or `Preparing`.
    plan: Option<Plan>,
}

/// The preparation queue. Methods that change what is saved take the library.
#[derive(Default)]
pub struct Preparation {
    queue: Vec<Entry>,
}

impl Preparation {
    /// The queue as saved by an earlier run: paused books stay paused, the rest wait for a plan.
    pub fn load(library: &Library) -> Self {
        let ids: Vec<String> = read_json(library, QUEUE_KEY).unwrap_or_default();
        let paused: HashSet<String> = read_json(library, PAUSED_KEY).unwrap_or_default();
        let mut chapters: HashMap<String, BTreeSet<usize>> =
            read_json(library, CHAPTERS_KEY).unwrap_or_default();
        let queue = ids
            .into_iter()
            .map(|id| {
                let scope = chapters
                    .remove(&id)
                    .filter(|c| !c.is_empty())
                    .map_or(Scope::Book, Scope::Chapters);
                let stage = if paused.contains(&id) {
                    Stage::Paused
                } else {
                    Stage::Waiting
                };
                Entry {
                    id,
                    scope,
                    stage,
                    plan: None,
                }
            })
            .collect();
        Self { queue }
    }

    pub fn stage(&self, library: &Library, id: &str) -> Stage {
        match self.entry(id) {
            Some(entry) => entry.stage,
            None if done_count(library, id).is_some() => Stage::Done,
            None => Stage::NotPrepared,
        }
    }

    pub fn view(&self, library: &Library, id: &str) -> Option<PrepareView> {
        if let Some(position) = self.position(id) {
            let entry = &self.queue[position];
            let (done, total) = entry.plan.as_ref().map_or((0, 0), Plan::progress);
            return Some(PrepareView {
                state: entry.stage,
                done,
                total,
                position: Some(position),
                chapters: entry.scope.chapters(),
            });
        }
        let made = done_count(library, id)?;
        Some(PrepareView {
            state: Stage::Done,
            done: made,
            total: made,
            position: None,
            chapters: None,
        })
    }

    /// Whether the book is in the queue (in any stage but done).
    pub fn contains(&self, id: &str) -> bool {
        self.position(id).is_some()
    }

    /// Queues the whole book. A book queued for some chapters is widened to the whole book.
    pub fn enqueue(&mut self, library: &Library, id: &str) {
        if let Some(entry) = self.entry_mut(id) {
            if entry.scope != Scope::Book {
                entry.scope = Scope::Book;
                apply(entry, Event::PlanDropped);
                clear_done(library, id);
                self.settle();
                self.save(library);
            }
            return;
        }
        let stage = next(self.stage(library, id), Event::Enqueue);
        clear_done(library, id);
        self.queue.push(Entry {
            id: id.into(),
            scope: Scope::Book,
            stage,
            plan: None,
        });
        self.save(library);
    }

    /// Queues one chapter, joining the book's entry if it has one. A book queued whole
    /// already covers it.
    pub fn enqueue_chapter(&mut self, library: &Library, id: &str, chapter: usize) {
        if let Some(entry) = self.entry_mut(id) {
            if let Scope::Chapters(chapters) = &mut entry.scope {
                if chapters.insert(chapter) {
                    apply(entry, Event::PlanDropped);
                    self.settle();
                    self.save(library);
                }
            }
            return;
        }
        let stage = next(self.stage(library, id), Event::Enqueue);
        self.queue.push(Entry {
            id: id.into(),
            scope: Scope::Chapters(BTreeSet::from([chapter])),
            stage,
            plan: None,
        });
        self.save(library);
    }

    /// The chapters a queued book is prepared for; `None` for a whole book or one not queued.
    pub fn chapters(&self, id: &str) -> Option<Vec<usize>> {
        self.entry(id)?.scope.chapters()
    }

    pub fn cancel(&mut self, library: &Library, id: &str) {
        if let Some(position) = self.position(id) {
            debug_assert_eq!(
                next(self.queue[position].stage, Event::Cancel),
                Stage::NotPrepared
            );
            self.queue.remove(position);
            self.settle();
            self.save(library);
        }
    }

    pub fn set_paused(&mut self, library: &Library, id: &str, paused: bool) {
        let event = if paused { Event::Pause } else { Event::Resume };
        if let Some(entry) = self.entry_mut(id) {
            apply(entry, event);
            self.settle();
            self.save(library);
        }
    }

    /// The book is gone from the library: forget its place and its done mark.
    pub fn forget(&mut self, library: &Library, id: &str) {
        self.cancel(library, id);
        clear_done(library, id);
    }

    /// Every queued book, in order.
    pub fn queued(&self) -> Vec<String> {
        self.queue.iter().map(|e| e.id.clone()).collect()
    }

    /// Books waiting for a plan, in queue order.
    pub fn unplanned(&self) -> Vec<String> {
        self.queue
            .iter()
            .filter(|e| e.stage == Stage::Waiting)
            .map(|e| e.id.clone())
            .collect()
    }

    /// Gives a waiting book its plan. A plan with nothing left to make finishes at once.
    pub fn planned(&mut self, library: &Library, id: &str, plan: Plan) {
        let Some(entry) = self.entry_mut(id) else {
            return;
        };
        if entry.stage != Stage::Waiting {
            return;
        }
        entry.plan = Some(plan);
        apply(entry, Event::Planned);
        self.finish_complete(library);
        self.settle();
    }

    /// Plans no longer match the engine or book settings; `None` drops every plan.
    pub fn drop_plans(&mut self, only: Option<&str>) {
        for entry in self
            .queue
            .iter_mut()
            .filter(|e| only.map_or(true, |id| e.id == id))
        {
            apply(entry, Event::PlanDropped);
        }
        self.settle();
    }

    /// A segment's audio was made. Returns whether any book finished.
    pub fn segment_made(&mut self, library: &Library, key: &str, duration_ms: u32) -> bool {
        // Keys are content hashes, so several books can share one.
        for plan in self.queue.iter_mut().filter_map(|e| e.plan.as_mut()) {
            plan.mark_cached(key, duration_ms);
        }
        let finished = self.finish_complete(library);
        self.settle();
        finished
    }

    /// Audio was deleted (cache limit, regenerate): those books need it made again.
    pub fn audio_lost(
        &mut self,
        library: &Library,
        books: &HashSet<String>,
        keys: &HashSet<String>,
    ) {
        for plan in self.queue.iter_mut().filter_map(|e| e.plan.as_mut()) {
            plan.mark_lost(keys);
        }
        for id in books.iter().filter(|id| !self.contains(id)) {
            if next(self.stage(library, id), Event::AudioLost) == Stage::NotPrepared {
                clear_done(library, id);
            }
        }
    }

    /// Planned books in the order their audio should be made.
    pub fn planned_books(&self) -> Vec<(&str, &Plan)> {
        self.queue
            .iter()
            .filter(|e| matches!(e.stage, Stage::Preparing | Stage::Queued))
            .filter_map(|e| Some((e.id.as_str(), e.plan.as_ref()?)))
            .collect()
    }

    /// The book being prepared and its (made, total).
    pub fn current(&self) -> Option<(&str, usize, usize)> {
        let entry = self.queue.iter().find(|e| e.stage == Stage::Preparing)?;
        let (done, total) = entry.plan.as_ref()?.progress();
        Some((&entry.id, done, total))
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.queue.iter().position(|e| e.id == id)
    }

    fn entry(&self, id: &str) -> Option<&Entry> {
        self.queue.iter().find(|e| e.id == id)
    }

    fn entry_mut(&mut self, id: &str) -> Option<&mut Entry> {
        self.queue.iter_mut().find(|e| e.id == id)
    }

    /// The first planned book in queue order is the one being prepared; the others wait behind it.
    fn settle(&mut self) {
        let mut first = true;
        for entry in self.queue.iter_mut() {
            match entry.stage {
                Stage::Queued if first => apply(entry, Event::Promote),
                Stage::Preparing if !first => apply(entry, Event::Demote),
                _ => {}
            }
            if matches!(entry.stage, Stage::Preparing) {
                first = false;
            }
        }
    }

    /// Moves planned books with every segment made to done. Returns whether any did.
    fn finish_complete(&mut self, library: &Library) -> bool {
        let mut finished = Vec::new();
        for entry in self.queue.iter_mut() {
            let Some((done, total)) = entry.plan.as_ref().map(Plan::progress) else {
                continue;
            };
            if done == total {
                apply(entry, Event::Finish);
                // Zero would read as "not prepared"; a book with nothing to say is still done.
                if entry.scope == Scope::Book {
                    let _ = library.set_setting(&done_key(&entry.id), &total.max(1).to_string());
                }
                finished.push(entry.id.clone());
            }
        }
        self.queue.retain(|e| e.stage != Stage::Done);
        if !finished.is_empty() {
            self.save(library);
        }
        !finished.is_empty()
    }

    fn save(&self, library: &Library) {
        let ids: Vec<&str> = self.queue.iter().map(|e| e.id.as_str()).collect();
        let paused: Vec<&str> = self
            .queue
            .iter()
            .filter(|e| e.stage == Stage::Paused)
            .map(|e| e.id.as_str())
            .collect();
        let chapters: HashMap<&str, Vec<usize>> = self
            .queue
            .iter()
            .filter_map(|e| Some((e.id.as_str(), e.scope.chapters()?)))
            .collect();
        for (key, value) in [
            (QUEUE_KEY, serde_json::to_string(&ids)),
            (PAUSED_KEY, serde_json::to_string(&paused)),
            (CHAPTERS_KEY, serde_json::to_string(&chapters)),
        ] {
            if let Ok(json) = value {
                if let Err(error) = library.set_setting(key, &json) {
                    eprintln!("aoide: cannot save the preparation queue: {error}");
                }
            }
        }
    }
}

/// Applies one event; a plan is kept exactly while the book is queued or being prepared.
fn apply(entry: &mut Entry, event: Event) {
    entry.stage = next(entry.stage, event);
    if !matches!(entry.stage, Stage::Queued | Stage::Preparing) {
        entry.plan = None;
    }
}

fn read_json<T: serde::de::DeserializeOwned>(library: &Library, key: &str) -> Option<T> {
    library
        .setting(key)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
}

fn done_count(library: &Library, id: &str) -> Option<usize> {
    library
        .setting(&done_key(id))
        .ok()
        .flatten()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
}

fn clear_done(library: &Library, id: &str) {
    let _ = library.set_setting(&done_key(id), "0");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(keys: &[(&str, bool)]) -> Plan {
        Plan {
            items: keys
                .iter()
                .map(|(key, cached)| PlanItem {
                    key: (*key).into(),
                    speakable: true,
                    cached: *cached,
                    duration_ms: 1000,
                })
                .chain(std::iter::once(PlanItem {
                    key: "silent:9".into(),
                    speakable: false,
                    cached: false,
                    duration_ms: 300,
                }))
                .collect(),
            requests: Vec::new(),
        }
    }
    fn library() -> Library {
        Library::open_in_memory().unwrap()
    }
    fn stages(p: &Preparation, lib: &Library, ids: &[&str]) -> Vec<Stage> {
        ids.iter().map(|id| p.stage(lib, id)).collect()
    }
    use Event::*;
    use Stage::*;

    #[test]
    fn queued_books_wait_for_a_plan_and_the_first_planned_one_is_prepared() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue(&lib, "a");
        p.enqueue(&lib, "b");
        p.enqueue(&lib, "a"); // Already queued: keeps its place.
        assert_eq!(p.unplanned(), ["a", "b"]);
        assert_eq!(
            stages(&p, &lib, &["a", "b", "c"]),
            [Waiting, Waiting, NotPrepared]
        );
        p.planned(&lib, "b", plan(&[("b1", false)]));
        assert_eq!(stages(&p, &lib, &["a", "b"]), [Waiting, Preparing]);
        p.planned(&lib, "a", plan(&[("a1", false), ("a2", true)]));
        // Queue order decides: "a" was queued first.
        assert_eq!(stages(&p, &lib, &["a", "b"]), [Preparing, Queued]);
        assert_eq!(p.current(), Some(("a", 1, 2)));
        let order: Vec<&str> = p.planned_books().into_iter().map(|(id, _)| id).collect();
        assert_eq!(order, ["a", "b"]);
        let view = p.view(&lib, "b").unwrap();
        assert_eq!(
            (view.state, view.done, view.total, view.position),
            (Queued, 0, 1, Some(1))
        );
    }

    #[test]
    fn pausing_lets_the_next_book_go_first_and_survives_a_restart() {
        let lib = library();
        let mut p = Preparation::default();
        for id in ["a", "b"] {
            p.enqueue(&lib, id);
            p.planned(&lib, id, plan(&[(id, false)]));
        }
        p.set_paused(&lib, "a", true);
        assert_eq!(stages(&p, &lib, &["a", "b"]), [Paused, Preparing]);
        assert_eq!(p.planned_books().len(), 1);

        let mut restarted = Preparation::load(&lib);
        assert_eq!(stages(&restarted, &lib, &["a", "b"]), [Paused, Waiting]);
        assert_eq!(restarted.unplanned(), ["b"]);
        restarted.set_paused(&lib, "a", false);
        assert_eq!(restarted.unplanned(), ["a", "b"]);
        assert_eq!(restarted.view(&lib, "a").unwrap().position, Some(0));
    }

    #[test]
    fn a_book_finishes_when_its_last_segment_is_made_and_stays_done_after_a_restart() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue(&lib, "a");
        p.planned(&lib, "a", plan(&[("k1", true), ("k2", false)]));
        assert!(!p.segment_made(&lib, "other", 500));
        assert!(p.segment_made(&lib, "k2", 500));
        assert_eq!(p.stage(&lib, "a"), Done);
        assert!(!p.contains("a"));
        let view = Preparation::load(&lib).view(&lib, "a").unwrap();
        assert_eq!(
            (view.state, view.done, view.total, view.position),
            (Done, 2, 2, None)
        );
    }

    #[test]
    fn a_book_with_all_its_audio_finishes_as_soon_as_it_is_planned() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue(&lib, "a");
        p.planned(&lib, "a", plan(&[("k1", true)]));
        assert_eq!(p.stage(&lib, "a"), Done);
    }

    #[test]
    fn lost_audio_undoes_done_and_puts_segments_back_to_make() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue(&lib, "done");
        p.planned(&lib, "done", plan(&[("d1", true)]));
        p.enqueue(&lib, "busy");
        p.planned(&lib, "busy", plan(&[("b1", true), ("b2", false)]));
        let books = HashSet::from(["done".to_string(), "busy".to_string()]);
        let keys = HashSet::from(["d1".to_string(), "b1".to_string()]);
        p.audio_lost(&lib, &books, &keys);
        assert_eq!(p.stage(&lib, "done"), NotPrepared);
        assert_eq!(p.current(), Some(("busy", 0, 2)));
    }

    #[test]
    fn removing_a_book_forgets_its_place_and_its_done_mark() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue(&lib, "a");
        p.planned(&lib, "a", plan(&[("k", true)]));
        p.enqueue(&lib, "b");
        p.forget(&lib, "a");
        p.forget(&lib, "b");
        // Importing the same file again (same id) must not claim its audio is ready.
        assert_eq!(
            stages(&Preparation::load(&lib), &lib, &["a", "b"]),
            [NotPrepared, NotPrepared]
        );
    }

    #[test]
    fn cancelling_and_dropping_plans() {
        let lib = library();
        let mut p = Preparation::default();
        for id in ["a", "b", "c"] {
            p.enqueue(&lib, id);
            p.planned(&lib, id, plan(&[(id, false)]));
        }
        p.set_paused(&lib, "c", true);
        p.cancel(&lib, "a");
        assert_eq!(
            stages(&p, &lib, &["a", "b", "c"]),
            [NotPrepared, Preparing, Paused]
        );
        p.drop_plans(None);
        assert_eq!(stages(&p, &lib, &["b", "c"]), [Waiting, Paused]);
        assert_eq!(p.current(), None);
        assert_eq!(
            stages(&Preparation::load(&lib), &lib, &["a", "b", "c"]),
            [NotPrepared, Waiting, Paused]
        );
    }

    #[test]
    fn a_chapter_is_prepared_on_its_own_and_does_not_mark_the_book_ready() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue_chapter(&lib, "a", 2);
        assert_eq!(p.chapters("a"), Some(vec![2]));
        assert_eq!(p.view(&lib, "a").unwrap().chapters, Some(vec![2]));
        p.planned(&lib, "a", plan(&[("k1", false)]));
        assert_eq!(p.stage(&lib, "a"), Preparing);
        assert!(p.segment_made(&lib, "k1", 500));
        assert!(!p.contains("a"));
        assert_eq!(p.stage(&lib, "a"), NotPrepared);
        assert_eq!(p.view(&lib, "a"), None);
    }

    #[test]
    fn more_chapters_join_the_books_entry_and_the_whole_book_takes_over() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue_chapter(&lib, "a", 5);
        p.planned(&lib, "a", plan(&[("k1", false)]));
        p.enqueue_chapter(&lib, "a", 2);
        assert_eq!(p.chapters("a"), Some(vec![2, 5]));
        assert_eq!(p.unplanned(), ["a"], "planned again for both chapters");
        p.enqueue(&lib, "a");
        assert_eq!(p.chapters("a"), None);
        assert_eq!(p.view(&lib, "a").unwrap().chapters, None);
        p.enqueue_chapter(&lib, "a", 7);
        assert_eq!(p.chapters("a"), None, "the whole book already covers it");
        assert_eq!(p.queued(), ["a"]);
    }

    #[test]
    fn chapter_entries_survive_a_restart_next_to_whole_books() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue_chapter(&lib, "a", 3);
        p.enqueue(&lib, "b");
        let restarted = Preparation::load(&lib);
        assert_eq!(restarted.queued(), ["a", "b"]);
        assert_eq!(restarted.chapters("a"), Some(vec![3]));
        assert_eq!(restarted.chapters("b"), None);
    }

    #[test]
    fn preparing_a_chapter_of_a_ready_book_keeps_it_ready() {
        let lib = library();
        let mut p = Preparation::default();
        p.enqueue(&lib, "a");
        p.planned(&lib, "a", plan(&[("k1", true)]));
        p.enqueue_chapter(&lib, "a", 0);
        assert_eq!(p.stage(&lib, "a"), Waiting);
        p.planned(&lib, "a", plan(&[("k1", true)]));
        assert_eq!(p.stage(&lib, "a"), Done);
    }

    #[test]
    fn events_that_do_not_apply_change_nothing() {
        assert_eq!(next(Done, Pause), Done);
        assert_eq!(next(NotPrepared, Finish), NotPrepared);
        assert_eq!(next(Paused, Planned), Paused);
        assert_eq!(next(Paused, Enqueue), Paused);
        assert_eq!(next(Waiting, Promote), Waiting);
        assert_eq!(next(Queued, AudioLost), Queued);
        assert_eq!(next(Done, Enqueue), Waiting);
        assert_eq!(next(Done, AudioLost), NotPrepared);
    }
}
