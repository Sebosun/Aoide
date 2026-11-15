//! "Find audiobooks": matches audiobooks kept on this computer to books in the library, by name.
//!
//! A scan searches the chosen folders (the home folder by default) and lists, for each book
//! without a recording, the audiobooks whose names come close to its own, closest first. The user
//! pairs the one they pick. With `verify_audiobooks` set, Aoide pairs them itself: it pairs the
//! closest, and when the quick check (a few samples looked for in the book's text) fails, it
//! unpairs it and tries the next, until one passes or none are left.
//!
//! Matches live in memory; a scan or an added book brings them back.

use super::*;

use std::sync::atomic::AtomicBool;

use super::discovery::{ScanProgress, ScanStage};

/// Which folders "Find audiobooks" searches. Empty `roots` means the home folder.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AudiobookSearchSettings {
    pub roots: Vec<String>,
    pub exclude: Vec<String>,
    /// Hidden folders to search anyway.
    pub include_hidden: Vec<String>,
    /// Look for the audiobooks of books as they are added.
    pub on_add: bool,
}

impl Default for AudiobookSearchSettings {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            exclude: Vec::new(),
            include_hidden: Vec::new(),
            on_add: true,
        }
    }
}

/// An audiobook whose name comes close to a book's.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AudiobookSuggestion {
    /// What to pair: audio files, or one folder.
    pub paths: Vec<String>,
    /// Its name has exactly the book's words.
    pub exact: bool,
    /// `paths` is one folder (rather than audio files).
    pub folder: bool,
    /// Every audio file it holds (in a folder: all of them, discs included), sorted.
    pub files: Vec<String>,
    /// Their size together.
    pub bytes: u64,
}

/// Where a book's match stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub enum MatchStage {
    /// Found; waiting for the user to pair one.
    Found,
    /// Aoide paired this candidate and waits for its quick check.
    Verifying { candidate: usize },
    /// This candidate is paired: the user chose it, or it passed the quick check.
    Paired { candidate: usize },
    /// Aoide tried every candidate and none passed the quick check.
    NoneVerified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchEvent {
    /// The user paired this candidate.
    Pair(usize),
    /// Aoide starts trying the candidates.
    Verify,
    CheckPassed,
    /// The quick check failed, or the candidate could not be paired or read.
    CheckFailed,
    /// The book's recording was removed.
    Unpaired,
}

/// The whole state machine, for a book with `candidates` candidates. Events that do not apply
/// leave the stage as it is.
pub fn next(stage: MatchStage, event: MatchEvent, candidates: usize) -> MatchStage {
    use MatchEvent::*;
    use MatchStage::*;
    match (stage, event) {
        (_, Pair(candidate)) if candidate < candidates => Paired { candidate },
        (Found, Verify) if candidates > 0 => Verifying { candidate: 0 },
        (Verifying { candidate }, CheckPassed) => Paired { candidate },
        (Verifying { candidate }, CheckFailed) if candidate + 1 < candidates => Verifying {
            candidate: candidate + 1,
        },
        (Verifying { .. }, CheckFailed) => NoneVerified,
        (Verifying { .. } | Paired { .. }, Unpaired) => Found,
        (stage, _) => stage,
    }
}

/// A library book and the audiobooks that may read it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AudiobookMatch {
    pub book_id: String,
    /// Closest first.
    pub candidates: Vec<AudiobookSuggestion>,
    pub stage: MatchStage,
}

/// The user's pick for a book.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct AudiobookChoice {
    pub book_id: String,
    /// Index into the match's candidates.
    pub candidate: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(test, derive(ts_rs::TS))]
pub struct PairOutcome {
    pub book_id: String,
    pub error: Option<String>,
}

/// What Find audiobooks keeps between calls.
#[derive(Default)]
pub(super) struct Matches {
    list: Vec<AudiobookMatch>,
    /// Set while a search runs; setting the flag stops it.
    cancel: Option<Arc<AtomicBool>>,
}

/// What to do after a match changed stage.
enum Then {
    Nothing,
    Attach(Vec<String>),
    Unpair,
}

impl Core {
    /// The matches the last searches found, and where each stands.
    pub fn audiobook_matches(&self) -> Vec<AudiobookMatch> {
        lock(&self.audiobooks).list.clone()
    }

    /// Searches the chosen folders for the audiobooks of every book without a recording.
    pub async fn scan_for_audiobooks(self: &Arc<Self>) -> Result<Vec<AudiobookMatch>, String> {
        self.search_audiobooks(None).await?;
        Ok(self.audiobook_matches())
    }

    /// After books were added: looks for their audiobooks, if the user wants that. Returns how
    /// many of them have audiobooks waiting for the user (none when Aoide verifies them).
    pub async fn look_for_audiobooks(self: &Arc<Self>, ids: Vec<String>) -> Result<usize, String> {
        if ids.is_empty() || !self.app_settings().audiobooks.on_add {
            return Ok(0);
        }
        self.search_audiobooks(Some(ids.clone())).await?;
        Ok(lock(&self.audiobooks)
            .list
            .iter()
            .filter(|m| ids.contains(&m.book_id) && m.stage == MatchStage::Found)
            .count())
    }

    pub fn cancel_audiobook_scan(&self) {
        if let Some(cancel) = lock(&self.audiobooks).cancel.as_ref() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Pairs the chosen candidates. Each book's pairing succeeds or fails on its own.
    pub fn pair_audiobooks(&self, choices: Vec<AudiobookChoice>) -> Vec<PairOutcome> {
        let outcomes = choices
            .into_iter()
            .map(|choice| {
                let paths = lock(&self.audiobooks)
                    .list
                    .iter()
                    .find(|m| m.book_id == choice.book_id)
                    .and_then(|m| m.candidates.get(choice.candidate))
                    .map(|c| c.paths.clone());
                let book = crate::logging::book(&choice.book_id).to_owned();
                let error = match paths {
                    None => Some("no such audiobook match".to_owned()),
                    Some(paths) => match self.attach_recording(&choice.book_id, paths) {
                        Ok(_) => {
                            self.match_event(&choice.book_id, MatchEvent::Pair(choice.candidate));
                            None
                        }
                        Err(error) => Some(error),
                    },
                };
                match &error {
                    None => tracing::info!(event = "audiobooks.pair", %book, candidate = choice.candidate, outcome = "ok", "Paired a found audiobook"),
                    Some(error) => tracing::warn!(event = "audiobooks.pair", %book, candidate = choice.candidate, outcome = "failed", %error, "Could not pair a found audiobook"),
                }
                PairOutcome {
                    book_id: choice.book_id,
                    error,
                }
            })
            .collect();
        self.emit_matches();
        outcomes
    }

    /// The sync job's word on a recording: whether its quick check passed (false too when it
    /// could not be read). A match being verified moves on to its next candidate on a failure.
    pub(super) fn audiobook_checked(&self, id: &str, passed: bool) {
        let event = if passed {
            MatchEvent::CheckPassed
        } else {
            MatchEvent::CheckFailed
        };
        if self.match_event(id, event) {
            self.follow_match(id);
            self.emit_matches();
        }
    }

    /// The book's recording was removed: its match waits for the user again.
    pub(super) fn audiobook_unpaired(&self, id: &str) {
        if self.match_event(id, MatchEvent::Unpaired) {
            self.emit_matches();
        }
    }

    /// Verifying was turned on: Aoide tries the matches waiting for the user.
    pub(super) fn verify_found_audiobooks(&self) {
        let waiting: Vec<String> = lock(&self.audiobooks)
            .list
            .iter()
            .filter(|m| m.stage == MatchStage::Found)
            .map(|m| m.book_id.clone())
            .collect();
        for id in &waiting {
            self.match_event(id, MatchEvent::Verify);
            self.follow_match(id);
        }
        if !waiting.is_empty() {
            self.emit_matches();
        }
    }

    /// Applies `event` to book `id`'s match. Returns whether its stage changed.
    fn match_event(&self, id: &str, event: MatchEvent) -> bool {
        let mut matches = lock(&self.audiobooks);
        let Some(found) = matches.list.iter_mut().find(|m| m.book_id == id) else {
            return false;
        };
        let before = found.stage;
        found.stage = next(before, event, found.candidates.len());
        if found.stage != before {
            tracing::info!(
                event = "match.stage",
                book = crate::logging::book(id),
                cause = ?event,
                from = crate::logging::name(before),
                to = crate::logging::name(found.stage),
                "Audiobook match changed"
            );
        }
        found.stage != before
    }

    /// Does what book `id`'s match stage asks for: pairs the candidate being verified (trying
    /// the next while one cannot be paired), or unpairs a book none of whose candidates passed.
    fn follow_match(&self, id: &str) {
        loop {
            let then = {
                let matches = lock(&self.audiobooks);
                match matches.list.iter().find(|m| m.book_id == id) {
                    Some(AudiobookMatch {
                        stage: MatchStage::Verifying { candidate },
                        candidates,
                        ..
                    }) => Then::Attach(candidates[*candidate].paths.clone()),
                    Some(AudiobookMatch {
                        stage: MatchStage::NoneVerified,
                        ..
                    }) => Then::Unpair,
                    _ => Then::Nothing,
                }
            };
            match then {
                Then::Nothing => return,
                Then::Unpair => {
                    self.forget_recording(id);
                    let _ = lock(&self.library).remove_recording(&BookId(id.into()));
                    self.emit(CoreEvent::LibraryChanged);
                    return;
                }
                Then::Attach(paths) => match self.attach_recording(id, paths) {
                    Ok(_) => return,
                    Err(error) => {
                        tracing::warn!(event = "audiobooks.pair", book = crate::logging::book(id), outcome = "failed", %error, "Could not pair a found audiobook; trying the next");
                        self.match_event(id, MatchEvent::CheckFailed);
                    }
                },
            }
        }
    }

    fn emit_matches(&self) {
        self.emit(CoreEvent::AudiobookMatches(self.audiobook_matches()));
    }

    fn audiobook_scan_options(&self) -> aoide_discovery::ScanOptions {
        let settings = self.app_settings().audiobooks;
        let roots: Vec<PathBuf> = if settings.roots.is_empty() {
            vec![self.paths.home_dir.clone()]
        } else {
            settings.roots.iter().map(PathBuf::from).collect()
        };
        let mut exclude: Vec<PathBuf> = settings.exclude.iter().map(PathBuf::from).collect();
        exclude.push(self.paths.library_dir.clone());
        exclude.push(self.paths.audio_dir.clone());
        if let Some(data) = self.paths.db_path.parent() {
            exclude.push(data.to_path_buf());
        }
        aoide_discovery::ScanOptions {
            roots,
            exclude,
            include_hidden: settings.include_hidden.iter().map(PathBuf::from).collect(),
            ..Default::default()
        }
    }

    /// Searches for the audiobooks of books `ids` (every book without a recording when `None`)
    /// and brings their matches up to date; with verifying on, starts on the new ones.
    async fn search_audiobooks(self: &Arc<Self>, ids: Option<Vec<String>>) -> Result<(), String> {
        let cancel = {
            let mut matches = lock(&self.audiobooks);
            if matches.cancel.is_some() {
                return Err("a search for audiobooks is already running".into());
            }
            let cancel = Arc::new(AtomicBool::new(false));
            matches.cancel = Some(Arc::clone(&cancel));
            cancel
        };
        let core = Arc::clone(self);
        let result = tokio::task::spawn_blocking(move || core.search_blocking(ids, &cancel))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);
        lock(&self.audiobooks).cancel = None;
        let stage = match &result {
            Ok(()) => ScanStage::Done,
            Err(error) if error == "cancelled" => ScanStage::Cancelled,
            Err(_) => ScanStage::Error,
        };
        self.emit(CoreEvent::AudiobookScan(ScanProgress {
            stage,
            dirs: 0,
            files: 0,
            found: 0,
            read: 0,
            to_read: 0,
            error: result.as_ref().err().cloned(),
        }));
        result?;
        if self.app_settings().verify_audiobooks {
            self.verify_found_audiobooks();
        }
        self.emit_matches();
        Ok(())
    }

    fn search_blocking(&self, ids: Option<Vec<String>>, cancel: &AtomicBool) -> Result<(), String> {
        let options = self.audiobook_scan_options();
        let started = Instant::now();
        let last_emit = Mutex::new(Instant::now());
        let seen = Mutex::new(aoide_discovery::Progress::default());
        let folder = aoide_discovery::AudiobookFolder::scan(&options, cancel, &|p| {
            *lock(&seen) = p;
            let mut last = lock(&last_emit);
            if last.elapsed() >= Duration::from_millis(150) {
                *last = Instant::now();
                self.emit(CoreEvent::AudiobookScan(ScanProgress {
                    stage: ScanStage::Walking,
                    dirs: p.dirs,
                    files: p.files,
                    found: p.found,
                    read: 0,
                    to_read: 0,
                    error: None,
                }));
            }
        });
        let seen = *lock(&seen);
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            tracing::info!(
                event = "audiobooks.scan",
                outcome = "cancelled",
                took_ms = crate::logging::took(started),
                "Search for audiobooks cancelled"
            );
            return Err("cancelled".into());
        }
        // Audio already paired with a book is not offered again.
        let paired = self.recordings.paired_files();
        let taken = |path: &Path| paired.iter().any(|f| f.starts_with(path));
        let books: Vec<aoide_library::BookRecord> = {
            let library = lock(&self.library);
            let with_recording: Vec<BookId> = library
                .recordings()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|r| r.book_id)
                .collect();
            library
                .books()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|b| !matches!(b.status, BookStatus::Error(_)))
                .filter(|b| !with_recording.contains(&b.id))
                .filter(|b| ids.as_ref().map_or(true, |ids| ids.contains(&b.id.0)))
                .collect()
        };
        let mut found: Vec<(String, AudiobookMatch)> = books
            .into_iter()
            .filter_map(|book| {
                let candidates: Vec<AudiobookSuggestion> = folder
                    .suggest(&book.path, book.title.as_deref(), &book.authors)
                    .into_iter()
                    .filter(|s| !s.paths.iter().any(|p| taken(p)))
                    .map(|s| AudiobookSuggestion {
                        folder: s.paths.len() == 1 && !s.files.contains(&s.paths[0]),
                        paths: s
                            .paths
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect(),
                        exact: s.exact,
                        files: s
                            .files
                            .iter()
                            .map(|p| p.to_string_lossy().into_owned())
                            .collect(),
                        bytes: s.bytes,
                    })
                    .collect();
                (!candidates.is_empty()).then(|| {
                    let title = book.title.clone().unwrap_or_default();
                    (
                        title,
                        AudiobookMatch {
                            book_id: book.id.0,
                            candidates,
                            stage: MatchStage::Found,
                        },
                    )
                })
            })
            .collect();
        found.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        tracing::info!(
            event = "audiobooks.scan",
            outcome = "ok",
            roots = options.roots.len(),
            only_added = ids.is_some(),
            dirs = seen.dirs,
            files = seen.files,
            audio = seen.found,
            matches = found.len(),
            took_ms = crate::logging::took(started),
            "Searched for audiobooks"
        );
        let mut matches = lock(&self.audiobooks);
        let searched = |id: &str| ids.as_ref().map_or(true, |ids| ids.iter().any(|i| i == id));
        // Matches already under way stay as they are; the ones waiting for the user are
        // replaced by what this search found.
        let kept: Vec<AudiobookMatch> = std::mem::take(&mut matches.list)
            .into_iter()
            .filter(|m| m.stage != MatchStage::Found || !searched(&m.book_id))
            .collect();
        let fresh: Vec<AudiobookMatch> = found
            .into_iter()
            .map(|(_, m)| m)
            .filter(|m| !kept.iter().any(|k| k.book_id == m.book_id))
            .collect();
        // Books just added come first; a whole scan lists by title.
        matches.list = if ids.is_some() {
            fresh.into_iter().chain(kept).collect()
        } else {
            kept.into_iter().chain(fresh).collect()
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::MatchEvent::*;
    use super::MatchStage::*;
    use super::*;

    #[test]
    fn verifying_tries_each_candidate_in_turn_until_one_passes() {
        let stage = next(Found, Verify, 3);
        assert_eq!(stage, Verifying { candidate: 0 });
        let stage = next(stage, CheckFailed, 3);
        assert_eq!(stage, Verifying { candidate: 1 });
        assert_eq!(next(stage, CheckPassed, 3), Paired { candidate: 1 });
        assert_eq!(
            next(Verifying { candidate: 2 }, CheckFailed, 3),
            NoneVerified
        );
    }

    #[test]
    fn the_user_can_pair_any_candidate_and_a_removed_recording_waits_again() {
        assert_eq!(next(Found, Pair(2), 3), Paired { candidate: 2 });
        assert_eq!(next(NoneVerified, Pair(0), 3), Paired { candidate: 0 });
        assert_eq!(next(Found, Pair(3), 3), Found, "no such candidate");
        assert_eq!(
            next(Paired { candidate: 0 }, CheckFailed, 3),
            Paired { candidate: 0 }
        );
        assert_eq!(next(Paired { candidate: 0 }, Unpaired, 3), Found);
        assert_eq!(next(Found, CheckPassed, 3), Found);
    }
}
