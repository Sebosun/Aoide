//! What the screens show about items without deriving it themselves: what would play if the
//! user listens ([`AudioSource`]) and what Home lists ([`HomeView`]).

use serde::Serialize;

use super::BookSummary;

/// What plays when the user listens to an item. For a text item this says what is set up, not
/// what is possible: any text can be narrated by a speech engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum AudioSource {
    None,
    Speech,
    Recording,
}

/// What a book has been set up to play with.
#[derive(Debug, Clone, Copy, Default)]
pub struct AudioFacts {
    /// Its narration is its recording, and the recording can be listened to.
    pub narrates_recording: bool,
    /// Speech audio is cached for it.
    pub cached_speech: bool,
    /// It is queued, running or finished in whole-book preparation.
    pub prepared: bool,
    /// It has been listened to.
    pub listened: bool,
}

/// The recording when it is the narration; speech when speech was made for or heard from the
/// book; otherwise nothing.
pub fn audio_source(facts: AudioFacts) -> AudioSource {
    if facts.narrates_recording {
        AudioSource::Recording
    } else if facts.cached_speech || facts.prepared || facts.listened {
        AudioSource::Speech
    } else {
        AudioSource::None
    }
}

/// Home's lists, as item ids; the screen looks the items up in the library it already has.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct HomeView {
    /// Up to 3 started, unfinished items, most recently opened first.
    pub continue_ids: Vec<String>,
    /// Up to 5 items never started, newest added first.
    pub up_next_ids: Vec<String>,
    /// Up to 8 items, newest added first, leaving out the ones listed above.
    pub recent_ids: Vec<String>,
}

const CONTINUE: usize = 3;
const UP_NEXT: usize = 5;
const RECENT: usize = 8;

/// Home from the library's items. Items whose file could not be read, and archived items, are
/// left out.
pub fn home(books: &[BookSummary]) -> HomeView {
    let usable: Vec<&BookSummary> = books
        .iter()
        .filter(|b| !b.archived && !matches!(b.status, aoide_library::BookStatus::Error(_)))
        .collect();
    let fraction = |b: &BookSummary| b.progress.as_ref().map(|p| p.fraction);
    let started = |b: &BookSummary| b.progress.is_some();
    let mut continuing: Vec<&BookSummary> = usable
        .iter()
        .copied()
        .filter(|b| {
            started(b)
                && fraction(b).is_some_and(|f| f < 1.0)
                && b.read_state != super::ReadState::Finished
        })
        .collect();
    continuing.sort_by_key(|b| std::cmp::Reverse(b.last_opened_at.unwrap_or(b.added_at)));
    let mut newest = usable.clone();
    newest.sort_by_key(|b| std::cmp::Reverse(b.added_at));
    let ids = |list: Vec<&BookSummary>, n: usize| -> Vec<String> {
        list.into_iter().take(n).map(|b| b.id.clone()).collect()
    };
    let continue_ids = ids(continuing, CONTINUE);
    let mut unstarted: Vec<&BookSummary> = newest.iter().copied().filter(|b| !started(b)).collect();
    // The next book of the series just finished goes first.
    if let Some(next) = next_in_series(books, &usable).filter(|b| !started(b)) {
        unstarted.retain(|b| b.id != next.id);
        unstarted.insert(0, next);
    }
    let up_next_ids = ids(unstarted, UP_NEXT);
    let recent_ids = ids(
        newest
            .into_iter()
            .filter(|b| !continue_ids.contains(&b.id) && !up_next_ids.contains(&b.id))
            .collect(),
        RECENT,
    );
    HomeView {
        continue_ids,
        up_next_ids,
        recent_ids,
    }
}

/// A series' name as it is matched: trimmed, case folded. No fuzzier matching: a wrong merge
/// is worse than two shelves, and a name can be corrected by hand.
pub(super) fn series_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// The book after the most recently finished item in its series, among `items`: the one with
/// the next higher place. None when that item has no series or nothing follows it.
fn next_in_series<'a>(books: &[BookSummary], items: &[&'a BookSummary]) -> Option<&'a BookSummary> {
    let last = books
        .iter()
        .filter(|b| b.finished_at.is_some())
        .max_by_key(|b| b.finished_at)?;
    let series = last.series.as_ref()?;
    let (key, after) = (series_key(&series.name), series.position?);
    items
        .iter()
        .copied()
        .filter(|b| {
            b.series.as_ref().is_some_and(|s| {
                series_key(&s.name) == key && s.position.is_some_and(|p| p > after)
            })
        })
        .min_by(|a, b| {
            let place = |b: &BookSummary| b.series.as_ref().and_then(|s| s.position);
            place(a)
                .partial_cmp(&place(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// The books of one series, for an item's page.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SeriesShelfView {
    /// The series' name as the asked item gives it.
    pub name: String,
    /// By place in the series (those without one last), then by title.
    pub items: Vec<SeriesItemView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct SeriesItemView {
    pub book_id: String,
    pub position: Option<f32>,
    pub state: super::ReadState,
}

/// The series item `id` is in, with every library item in it; `None` when it is in none.
pub fn series_shelf(books: &[BookSummary], id: &str) -> Option<SeriesShelfView> {
    let name = books
        .iter()
        .find(|b| b.id == id)?
        .series
        .as_ref()?
        .name
        .clone();
    let key = series_key(&name);
    let mut members: Vec<&BookSummary> = books
        .iter()
        .filter(|b| {
            b.series
                .as_ref()
                .is_some_and(|s| series_key(&s.name) == key)
        })
        .collect();
    let place = |b: &BookSummary| b.series.as_ref().and_then(|s| s.position);
    members.sort_by(|a, b| {
        let (pa, pb) = (place(a), place(b));
        match (pa, pb) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
            // Those without a place last.
            _ => pa.is_none().cmp(&pb.is_none()),
        }
        .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
    });
    Some(SeriesShelfView {
        name,
        items: members
            .into_iter()
            .map(|b| SeriesItemView {
                book_id: b.id.clone(),
                position: place(b),
                state: b.read_state,
            })
            .collect(),
    })
}

impl super::Core {
    /// Home's lists, from the library as it is now.
    pub fn home(&self) -> Result<HomeView, String> {
        Ok(home(&self.list_books()?))
    }

    /// The series item `id` is in, with its library's books in it.
    pub fn series(&self, id: &str) -> Result<Option<SeriesShelfView>, String> {
        Ok(series_shelf(&self.list_books()?, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::BookProgressView;
    use aoide_library::BookStatus;

    #[test]
    fn audio_source_prefers_the_recording_then_any_speech() {
        let none = AudioFacts::default();
        let cases = [
            (none, AudioSource::None),
            (
                AudioFacts {
                    cached_speech: true,
                    ..none
                },
                AudioSource::Speech,
            ),
            (
                AudioFacts {
                    prepared: true,
                    ..none
                },
                AudioSource::Speech,
            ),
            (
                AudioFacts {
                    listened: true,
                    ..none
                },
                AudioSource::Speech,
            ),
            (
                AudioFacts {
                    narrates_recording: true,
                    cached_speech: true,
                    prepared: true,
                    listened: true,
                },
                AudioSource::Recording,
            ),
        ];
        for (facts, expected) in cases {
            assert_eq!(audio_source(facts), expected, "{facts:?}");
        }
    }

    fn book(id: &str, added_at: i64, opened: Option<i64>, fraction: Option<f64>) -> BookSummary {
        BookSummary {
            id: id.into(),
            title: id.into(),
            authors: vec![],
            language: None,
            status: BookStatus::Present,
            cover_path: None,
            progress: fraction.map(|fraction| BookProgressView {
                chapter: 0,
                segment_id: Some("0:0:0".into()),
                fraction,
                position_ms: None,
            }),
            audio_bytes: 0,
            kind: aoide_library::ItemKind::Book,
            audio: AudioSource::None,
            mode: crate::core::ReadingMode::Read,
            shape: crate::core::Shape::Read,
            read_state: if fraction.is_some() {
                crate::core::ReadState::InProgress
            } else {
                crate::core::ReadState::Unread
            },
            word_count: None,
            duration_ms: None,
            prepare: None,
            added_at,
            last_opened_at: opened,
            collections: vec![],
            tags: Vec::new(),
            recording: None,
            article: None,
            archived: false,
            queued: false,
            edited: Vec::new(),
            series: None,
            finished_at: None,
        }
    }

    fn in_series(
        id: &str,
        added_at: i64,
        position: f32,
        finished_at: Option<i64>,
        started: bool,
    ) -> BookSummary {
        let mut item = book(id, added_at, None, started.then_some(0.3));
        item.series = Some(crate::core::SeriesView {
            name: "Harbour Lights".into(),
            position: Some(position),
        });
        if finished_at.is_some() {
            item.finished_at = finished_at;
            item.read_state = crate::core::ReadState::Finished;
            item.progress = Some(BookProgressView {
                chapter: 0,
                segment_id: None,
                fraction: 1.0,
                position_ms: None,
            });
        }
        item
    }

    #[test]
    fn a_series_shelf_lists_its_books_by_place_with_their_state() {
        let mut other_case = in_series("half", 5, 2.5, None, true);
        other_case.series.as_mut().unwrap().name = " harbour LIGHTS ".into();
        let mut unplaced = in_series("unplaced", 6, 0.0, None, false);
        unplaced.series.as_mut().unwrap().position = None;
        let books = vec![
            in_series("three", 4, 3.0, None, false),
            in_series("one", 1, 1.0, Some(100), false),
            other_case,
            unplaced,
            book("alone", 9, None, None),
        ];
        let shelf = series_shelf(&books, "three").unwrap();
        assert_eq!(shelf.name, "Harbour Lights");
        let listed: Vec<(&str, Option<f32>, crate::core::ReadState)> = shelf
            .items
            .iter()
            .map(|i| (i.book_id.as_str(), i.position, i.state))
            .collect();
        use crate::core::ReadState::*;
        assert_eq!(
            listed,
            [
                ("one", Some(1.0), Finished),
                ("half", Some(2.5), InProgress),
                ("three", Some(3.0), Unread),
                ("unplaced", None, Unread),
            ]
        );
        assert_eq!(series_shelf(&books, "alone"), None);
        assert_eq!(series_shelf(&books, "missing"), None);
    }

    #[test]
    fn the_next_book_of_a_series_just_finished_comes_first_up_next() {
        let other = |id: &str, added_at| book(id, added_at, None, None);
        // Two finished, the second most recently: its next book (by place, not by when it
        // was added) goes first, ahead of newer books.
        let books = vec![
            in_series("one", 1, 1.0, Some(100), false),
            in_series("two", 2, 2.0, Some(200), false),
            in_series("four", 3, 4.0, None, false),
            in_series("three", 4, 3.0, None, false),
            other("newer", 50),
        ];
        assert_eq!(home(&books).up_next_ids[..2], ["three", "newer"]);

        // Its next one started, or archived, or none left: the usual order.
        let mut started = books.clone();
        started[3] = in_series("three", 4, 3.0, None, true);
        assert_eq!(home(&started).up_next_ids[0], "newer");
        let mut archived = books.clone();
        archived[3].archived = true;
        assert_eq!(home(&archived).up_next_ids[0], "four");
        let last = vec![
            in_series("one", 1, 1.0, Some(300), false),
            in_series("two", 2, 2.0, Some(200), false),
            other("newer", 50),
        ];
        assert_eq!(home(&last).up_next_ids[0], "newer");
        // The latest finished has no series: nothing moves.
        let mut plain = books.clone();
        plain[4].finished_at = Some(900);
        plain[4].read_state = crate::core::ReadState::Finished;
        plain[4].progress = started[3].progress.clone();
        plain.push(other("newest", 60));
        assert_eq!(home(&plain).up_next_ids[0], "newest");
    }

    #[test]
    fn home_leaves_archived_items_out_of_every_list() {
        // Each would be in one list: continued, up next, recent.
        let cases = [
            ("started", Some(50), Some(0.4)),
            ("never-started", None, None),
            ("finished", Some(99), Some(1.0)),
        ];
        for (id, opened, fraction) in cases {
            let mut item = book(id, 1, opened, fraction);
            assert_ne!(home(&[item.clone()]), HomeView::default(), "{id} is listed");
            item.archived = true;
            assert_eq!(home(&[item]), HomeView::default(), "{id} is archived");
        }
    }

    #[test]
    fn home_lists_started_unstarted_and_the_rest_by_date() {
        let mut books = vec![
            book("old-started", 1, Some(50), Some(0.4)),
            book("new-started", 2, Some(90), Some(0.1)),
            book("finished", 3, Some(99), Some(1.0)),
            book("at-start", 4, Some(70), Some(0.0)),
            book("broken", 100, None, None),
        ];
        books[4].status = BookStatus::Error("bad zip".into());
        for i in 0..7 {
            books.push(book(&format!("fresh-{i}"), 10 + i, None, None));
        }
        let home = home(&books);
        assert_eq!(
            home.continue_ids,
            ["new-started", "at-start", "old-started"]
        );
        assert_eq!(
            home.up_next_ids,
            ["fresh-6", "fresh-5", "fresh-4", "fresh-3", "fresh-2"]
        );
        assert_eq!(home.recent_ids, ["fresh-1", "fresh-0", "finished"]);
    }

    #[test]
    fn home_is_empty_for_an_empty_library() {
        assert_eq!(home(&[]), HomeView::default());
    }
}
