//! Desktop notifications: when a long job ends while Aoide is not in front, the desktop says so.
//!
//! [`notice`] is the rule: what happened, how long it took and whether the window is in front
//! decide whether there is anything to say. Feed arrivals never notify.

// Wired to the jobs in the next slice.
#![allow(dead_code)]

use std::time::Duration;

/// Jobs shorter than this end while the user is still looking: they do not notify.
pub const LONG_JOB: Duration = Duration::from_secs(60);

/// A long job that ended, or one that needs the user. Each names the book it is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Happened {
    Prepared(String),
    /// The speech engine failed while the book was being prepared.
    PreparationStalled(String),
    Synced(String),
    /// A poor match or a doubtful sample check waits for the user's word.
    SyncNeedsYou(String),
    SyncFailed(String),
    Exported(String),
    ExportFailed(String),
    /// "Find books" finished importing what was chosen.
    ImportFinished {
        added: u32,
    },
}

impl Happened {
    /// The kind, as logged.
    pub fn kind(&self) -> &'static str {
        match self {
            Happened::Prepared(_) => "prepared",
            Happened::PreparationStalled(_) => "preparationStalled",
            Happened::Synced(_) => "synced",
            Happened::SyncNeedsYou(_) => "syncNeedsYou",
            Happened::SyncFailed(_) => "syncFailed",
            Happened::Exported(_) => "exported",
            Happened::ExportFailed(_) => "exportFailed",
            Happened::ImportFinished { .. } => "importFinished",
        }
    }

    /// The book it is about, if one.
    pub fn book(&self) -> Option<&str> {
        match self {
            Happened::Prepared(id)
            | Happened::PreparationStalled(id)
            | Happened::Synced(id)
            | Happened::SyncNeedsYou(id)
            | Happened::SyncFailed(id)
            | Happened::Exported(id)
            | Happened::ExportFailed(id) => Some(id),
            Happened::ImportFinished { .. } => None,
        }
    }

    /// Whether the user has to do something about it: these notify however long the job ran.
    fn needs_you(&self) -> bool {
        matches!(
            self,
            Happened::PreparationStalled(_)
                | Happened::SyncNeedsYou(_)
                | Happened::SyncFailed(_)
                | Happened::ExportFailed(_)
        )
    }
}

/// What the desktop is asked to show. Clicking it opens `opens`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    pub opens: Option<String>,
}

/// Where the user is when it happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    /// How long the job ran.
    pub took: Duration,
    /// Whether Aoide's window is in front (a hidden window is not).
    pub focused: bool,
    /// The user's switch for notifications.
    pub enabled: bool,
}

/// What to show for `happened` (to the book titled `title`), or nothing: never while the window
/// is in front or notifications are off, and for a job that ended well only when it ran longer
/// than [`LONG_JOB`].
pub fn notice(happened: &Happened, title: &str, moment: Moment) -> Option<Notice> {
    if moment.focused || !moment.enabled {
        return None;
    }
    if !happened.needs_you() && moment.took < LONG_JOB {
        return None;
    }
    let body = match happened {
        Happened::Prepared(_) => "Prepared: its speech is made and plays without waiting.",
        Happened::PreparationStalled(_) => {
            "Preparing has stopped: the speech engine failed. Aoide tries again."
        }
        Happened::Synced(_) => "Its recording is synced with the text.",
        Happened::SyncNeedsYou(_) => "Its recording needs a look before it is synced.",
        Happened::SyncFailed(_) => "Syncing its recording failed.",
        Happened::Exported(_) => "Exported to the Exports folder.",
        Happened::ExportFailed(_) => "The export failed.",
        Happened::ImportFinished { added } => {
            return Some(Notice {
                title: "Books imported".into(),
                body: match added {
                    0 => "No books were added to the library.".into(),
                    1 => "1 book was added to the library.".into(),
                    n => format!("{n} books were added to the library."),
                },
                opens: None,
            });
        }
    };
    Some(Notice {
        title: title.into(),
        body: body.into(),
        opens: happened.book().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTES: Duration = Duration::from_secs(120);
    const MOMENT: Duration = Duration::from_secs(5);

    fn away(took: Duration) -> Moment {
        Moment {
            took,
            focused: false,
            enabled: true,
        }
    }

    #[test]
    fn each_long_job_notifies_with_its_title_and_opens_its_book_when_away() {
        let id = || "b1".to_string();
        let table = [
            Happened::Prepared(id()),
            Happened::PreparationStalled(id()),
            Happened::Synced(id()),
            Happened::SyncNeedsYou(id()),
            Happened::SyncFailed(id()),
            Happened::Exported(id()),
            Happened::ExportFailed(id()),
        ];
        for happened in table {
            let notice = notice(&happened, "Quay Lights", away(MINUTES))
                .unwrap_or_else(|| panic!("{happened:?} says nothing"));
            assert_eq!(notice.title, "Quay Lights", "{happened:?}");
            assert!(!notice.body.is_empty(), "{happened:?}");
            assert_eq!(notice.opens.as_deref(), Some("b1"), "{happened:?}");
        }
        let imported = notice(&Happened::ImportFinished { added: 3 }, "", away(MINUTES)).unwrap();
        assert_eq!(imported.title, "Books imported");
        assert_eq!(imported.body, "3 books were added to the library.");
        assert_eq!(imported.opens, None);
    }

    #[test]
    fn nothing_is_said_while_the_window_is_in_front_or_notifications_are_off() {
        for happened in [
            Happened::Prepared("b".into()),
            Happened::SyncFailed("b".into()),
            Happened::ImportFinished { added: 1 },
        ] {
            let focused = Moment {
                focused: true,
                ..away(MINUTES)
            };
            let off = Moment {
                enabled: false,
                ..away(MINUTES)
            };
            assert_eq!(notice(&happened, "T", focused), None, "{happened:?}");
            assert_eq!(notice(&happened, "T", off), None, "{happened:?}");
        }
    }

    #[test]
    fn short_jobs_are_silent_but_what_needs_you_is_not() {
        let table = [
            (Happened::Prepared("b".into()), false),
            (Happened::Synced("b".into()), false),
            (Happened::Exported("b".into()), false),
            (Happened::ImportFinished { added: 2 }, false),
            (Happened::PreparationStalled("b".into()), true),
            (Happened::SyncNeedsYou("b".into()), true),
            (Happened::SyncFailed("b".into()), true),
            (Happened::ExportFailed("b".into()), true),
        ];
        for (happened, says) in table {
            assert_eq!(
                notice(&happened, "T", away(MOMENT)).is_some(),
                says,
                "{happened:?}"
            );
        }
    }
}
