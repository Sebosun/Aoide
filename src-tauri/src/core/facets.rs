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
    let up_next_ids = ids(
        newest.iter().copied().filter(|b| !started(b)).collect(),
        UP_NEXT,
    );
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

impl super::Core {
    /// Home's lists, from the library as it is now.
    pub fn home(&self) -> Result<HomeView, String> {
        Ok(home(&self.list_books()?))
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
        }
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
