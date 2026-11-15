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

/// What the audiobook finder asks of the rest of the app.
pub(super) trait FinderHost: Sync {
    /// Audio files paired with a book, but for book `except`'s.
    fn paired_files(&self, except: Option<&str>) -> Vec<PathBuf>;
    /// Pairs book `id` with an audiobook's `paths` (its files, or its folder).
    fn attach(&self, id: &str, paths: Vec<String>) -> Result<(), String>;
    /// Unpairs book `id`, none of whose candidates passed.
    fn unpair(&self, id: &str);
}

/// How a search goes: where it looks, and whether Aoide verifies what it finds.
pub(super) struct Search {
    pub settings: AudiobookSearchSettings,
    pub verify: bool,
}

/// Whether `paths` (an audiobook's files, or its folder) hold audio paired with a book other
/// than `id`.
fn taken(host: &dyn FinderHost, id: &str, paths: &[String]) -> bool {
    let paired = host.paired_files(Some(id));
    paths
        .iter()
        .any(|p| paired.iter().any(|f| f.starts_with(p)))
}

/// "Find audiobooks": searches folders for the audiobooks of books without a recording, and
/// takes each match through checking and pairing.
pub(super) struct Finder {
    library: Arc<Mutex<Library>>,
    events: Arc<dyn Events>,
    /// Where a search starts when no folders are set.
    home_dir: PathBuf,
    /// Aoide's own folders, never searched.
    private_dirs: Vec<PathBuf>,
    matches: Mutex<Matches>,
}

impl Finder {
    pub fn new(library: Arc<Mutex<Library>>, paths: &Paths, events: Arc<dyn Events>) -> Self {
        let mut private_dirs = vec![paths.library_dir.clone(), paths.audio_dir.clone()];
        private_dirs.extend(paths.db_path.parent().map(Path::to_path_buf));
        Self {
            library,
            events,
            home_dir: paths.home_dir.clone(),
            private_dirs,
            matches: Default::default(),
        }
    }

    /// The matches the last searches found, and where each stands.
    pub fn matches(&self) -> Vec<AudiobookMatch> {
        lock(&self.matches).list.clone()
    }

    /// After books were added: looks for their audiobooks, if the user wants that. Returns how
    /// many of them have audiobooks waiting for the user (none when Aoide verifies them).
    pub async fn look_for(
        self: &Arc<Self>,
        ids: Vec<String>,
        search: Search,
        host: &dyn FinderHost,
    ) -> Result<usize, String> {
        if ids.is_empty() || !search.settings.on_add {
            return Ok(0);
        }
        self.search(Some(ids.clone()), search, host).await?;
        Ok(lock(&self.matches)
            .list
            .iter()
            .filter(|m| ids.contains(&m.book_id) && m.stage == MatchStage::Found)
            .count())
    }

    pub fn cancel(&self) {
        if let Some(cancel) = lock(&self.matches).cancel.as_ref() {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Pairs the chosen candidates, in order. Each book's pairing succeeds or fails on its own;
    /// an audiobook another book has already is not paired again. A paired audiobook is offered
    /// to no other book.
    pub fn pair(&self, choices: Vec<AudiobookChoice>, host: &dyn FinderHost) -> Vec<PairOutcome> {
        // The candidates as they were chosen, before pairing drops any.
        let chosen: Vec<(AudiobookChoice, Option<Vec<String>>)> = {
            let matches = lock(&self.matches);
            choices
                .into_iter()
                .map(|choice| {
                    let paths = matches
                        .list
                        .iter()
                        .find(|m| m.book_id == choice.book_id)
                        .and_then(|m| m.candidates.get(choice.candidate))
                        .map(|c| c.paths.clone());
                    (choice, paths)
                })
                .collect()
        };
        let outcomes = chosen
            .into_iter()
            .map(|(choice, paths)| {
                let book = crate::logging::book(&choice.book_id).to_owned();
                let error = match paths {
                    None => Some("no such audiobook match".to_owned()),
                    Some(paths) if taken(host, &choice.book_id, &paths) => {
                        Some("This audiobook is paired with another book".to_owned())
                    }
                    Some(paths) => match host.attach(&choice.book_id, paths) {
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
        self.drop_taken(host);
        self.emit_matches();
        outcomes
    }

    /// Takes audiobooks paired with a book out of the matches waiting for the user (or left
    /// unverified), and the matches left with none out of the list. Matches under way keep
    /// their candidates, which their stages count.
    fn drop_taken(&self, host: &dyn FinderHost) {
        let paired = host.paired_files(None);
        let taken = |c: &AudiobookSuggestion| {
            c.paths
                .iter()
                .any(|p| paired.iter().any(|f| f.starts_with(p)))
        };
        let mut matches = lock(&self.matches);
        for m in &mut matches.list {
            if matches!(m.stage, MatchStage::Found | MatchStage::NoneVerified) {
                m.candidates.retain(|c| !taken(c));
            }
        }
        matches.list.retain(|m| {
            !m.candidates.is_empty()
                || !matches!(m.stage, MatchStage::Found | MatchStage::NoneVerified)
        });
    }

    /// The sync job's word on a recording: whether its quick check passed (false too when it
    /// could not be read). A match being verified moves on to its next candidate on a failure.
    pub fn checked(&self, id: &str, passed: bool, host: &dyn FinderHost) {
        let event = if passed {
            MatchEvent::CheckPassed
        } else {
            MatchEvent::CheckFailed
        };
        if self.match_event(id, event) {
            self.follow_match(id, host);
            if passed {
                self.drop_taken(host);
            }
            self.emit_matches();
        }
    }

    /// The book's recording was removed: its match waits for the user again.
    pub fn unpaired(&self, id: &str) {
        if self.match_event(id, MatchEvent::Unpaired) {
            self.emit_matches();
        }
    }

    /// Verifying was turned on: Aoide tries the matches waiting for the user.
    pub fn verify_found(&self, host: &dyn FinderHost) {
        let waiting: Vec<String> = lock(&self.matches)
            .list
            .iter()
            .filter(|m| m.stage == MatchStage::Found)
            .map(|m| m.book_id.clone())
            .collect();
        for id in &waiting {
            self.match_event(id, MatchEvent::Verify);
            self.follow_match(id, host);
        }
        if !waiting.is_empty() {
            self.emit_matches();
        }
    }

    /// Applies `event` to book `id`'s match. Returns whether its stage changed.
    fn match_event(&self, id: &str, event: MatchEvent) -> bool {
        let mut matches = lock(&self.matches);
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
    fn follow_match(&self, id: &str, host: &dyn FinderHost) {
        loop {
            let then = {
                let matches = lock(&self.matches);
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
                    host.unpair(id);
                    return;
                }
                Then::Attach(paths) if taken(host, id, &paths) => {
                    tracing::info!(
                        event = "audiobooks.pair",
                        book = crate::logging::book(id),
                        outcome = "skipped",
                        "Another book has this audiobook; trying the next"
                    );
                    self.match_event(id, MatchEvent::CheckFailed);
                }
                Then::Attach(paths) => match host.attach(id, paths) {
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
        self.emit(CoreEvent::AudiobookMatches(self.matches()));
    }

    fn emit(&self, event: CoreEvent) {
        match event.payload() {
            Ok(value) => self.events.emit(event.name(), value),
            Err(error) => {
                tracing::error!(event = "app.emit", name = event.name(), %error, "Cannot send an event to the window")
            }
        }
    }

    fn scan_options(&self, settings: &AudiobookSearchSettings) -> aoide_discovery::ScanOptions {
        let roots: Vec<PathBuf> = if settings.roots.is_empty() {
            vec![self.home_dir.clone()]
        } else {
            settings.roots.iter().map(PathBuf::from).collect()
        };
        let mut exclude: Vec<PathBuf> = settings.exclude.iter().map(PathBuf::from).collect();
        exclude.extend(self.private_dirs.iter().cloned());
        aoide_discovery::ScanOptions {
            roots,
            exclude,
            include_hidden: settings.include_hidden.iter().map(PathBuf::from).collect(),
            ..Default::default()
        }
    }

    /// Searches for the audiobooks of books `ids` (every book without a recording when `None`)
    /// and brings their matches up to date; with verifying on, starts on the new ones.
    pub async fn search(
        self: &Arc<Self>,
        ids: Option<Vec<String>>,
        search: Search,
        host: &dyn FinderHost,
    ) -> Result<(), String> {
        let cancel = {
            let mut matches = lock(&self.matches);
            if matches.cancel.is_some() {
                return Err("a search for audiobooks is already running".into());
            }
            let cancel = Arc::new(AtomicBool::new(false));
            matches.cancel = Some(Arc::clone(&cancel));
            cancel
        };
        let this = Arc::clone(self);
        // Audio already paired with a book is not offered again.
        let paired = host.paired_files(None);
        let settings = search.settings;
        let result = tokio::task::spawn_blocking(move || {
            this.search_blocking(ids, &settings, &paired, &cancel)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        lock(&self.matches).cancel = None;
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
        if search.verify {
            self.verify_found(host);
        }
        self.emit_matches();
        Ok(())
    }

    fn search_blocking(
        &self,
        ids: Option<Vec<String>>,
        settings: &AudiobookSearchSettings,
        paired: &[PathBuf],
        cancel: &AtomicBool,
    ) -> Result<(), String> {
        let options = self.scan_options(settings);
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
        // Each audiobook goes to the books it comes closest to.
        let suggested = folder.suggest_each(
            books
                .iter()
                .map(|b| (b.path.as_path(), b.title.as_deref(), b.authors.as_slice())),
        );
        let mut found: Vec<(String, AudiobookMatch)> = books
            .into_iter()
            .zip(suggested)
            .filter_map(|(book, suggested)| {
                let candidates: Vec<AudiobookSuggestion> = suggested
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
        let mut matches = lock(&self.matches);
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

impl Core {
    /// The matches the last searches found, and where each stands.
    pub fn audiobook_matches(&self) -> Vec<AudiobookMatch> {
        self.audiobooks.matches()
    }

    /// Searches the chosen folders for the audiobooks of every book without a recording.
    pub async fn scan_for_audiobooks(&self) -> Result<Vec<AudiobookMatch>, String> {
        self.audiobooks
            .search(None, self.audiobook_search(), self)
            .await?;
        Ok(self.audiobook_matches())
    }

    /// After books were added: looks for their audiobooks, if the user wants that. Returns how
    /// many of them have audiobooks waiting for the user (none when Aoide verifies them).
    pub async fn look_for_audiobooks(&self, ids: Vec<String>) -> Result<usize, String> {
        self.audiobooks
            .look_for(ids, self.audiobook_search(), self)
            .await
    }

    pub fn cancel_audiobook_scan(&self) {
        self.audiobooks.cancel()
    }

    /// Pairs the chosen candidates, in order.
    pub fn pair_audiobooks(&self, choices: Vec<AudiobookChoice>) -> Vec<PairOutcome> {
        self.audiobooks.pair(choices, self)
    }

    /// The sync job's word on a recording: whether its quick check passed.
    pub(super) fn audiobook_checked(&self, id: &str, passed: bool) {
        self.audiobooks.checked(id, passed, self)
    }

    /// The book's recording was removed: its match waits for the user again.
    pub(super) fn audiobook_unpaired(&self, id: &str) {
        self.audiobooks.unpaired(id)
    }

    /// Verifying was turned on: Aoide tries the matches waiting for the user.
    pub(super) fn verify_found_audiobooks(&self) {
        self.audiobooks.verify_found(self)
    }

    fn audiobook_search(&self) -> Search {
        let settings = self.app_settings();
        Search {
            settings: settings.audiobooks,
            verify: settings.verify_audiobooks,
        }
    }
}

impl FinderHost for Core {
    fn paired_files(&self, except: Option<&str>) -> Vec<PathBuf> {
        self.recordings.paired_files(except)
    }

    fn attach(&self, id: &str, paths: Vec<String>) -> Result<(), String> {
        self.attach_recording(id, paths).map(drop)
    }

    fn unpair(&self, id: &str) {
        self.forget_recording(id);
        let removed = lock(&self.library).remove_recording(&BookId(id.into()));
        if let Err(error) = removed {
            self.report(
                "audiobooks.pair",
                "Could not unpair an audiobook none of whose files passed",
                error,
            );
        }
        self.emit(CoreEvent::LibraryChanged);
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

    /// Pairs whatever it is asked to, and reports those as paired.
    #[derive(Default)]
    struct Host {
        paired: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl FinderHost for Host {
        fn paired_files(&self, except: Option<&str>) -> Vec<PathBuf> {
            lock(&self.paired)
                .iter()
                .filter(|(id, _)| Some(id.as_str()) != except)
                .flat_map(|(_, paths)| paths.iter().map(PathBuf::from))
                .collect()
        }
        fn attach(&self, id: &str, paths: Vec<String>) -> Result<(), String> {
            lock(&self.paired).push((id.into(), paths));
            Ok(())
        }
        fn unpair(&self, id: &str) {
            lock(&self.paired).retain(|(paired, _)| paired != id);
        }
    }

    #[tokio::test]
    async fn a_found_audiobook_once_paired_is_offered_to_no_other_book() {
        let dir = tempfile::tempdir().unwrap();
        let audiobooks = dir.path().join("Audiobooks");
        std::fs::create_dir_all(audiobooks.join("Book lanterns")).unwrap();
        std::fs::write(audiobooks.join("Book lanterns/part.mp3"), [0u8; 64]).unwrap();
        let finder = Arc::new(Finder::new(
            super::super::tests::library_with(&["lanterns"]),
            &super::super::tests::paths(dir.path()),
            Arc::new(super::super::tests::Recorder::default()),
        ));
        let search = || Search {
            settings: AudiobookSearchSettings {
                roots: vec![audiobooks.to_string_lossy().into_owned()],
                ..Default::default()
            },
            verify: false,
        };
        let host = Host::default();

        finder.search(None, search(), &host).await.unwrap();
        let found = finder.matches();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            (found[0].book_id.as_str(), found[0].stage),
            ("lanterns", MatchStage::Found)
        );

        let outcome = finder.pair(
            vec![AudiobookChoice {
                book_id: "lanterns".into(),
                candidate: 0,
            }],
            &host,
        );
        assert_eq!(outcome[0].error, None);
        assert_eq!(lock(&host.paired)[0].0, "lanterns");
        assert_ne!(finder.matches()[0].stage, MatchStage::Found);

        host.unpair("lanterns");
        lock(&host.paired).push(("other".into(), found[0].candidates[0].paths.clone()));
        finder.search(None, search(), &host).await.unwrap();
        assert!(
            finder
                .matches()
                .iter()
                .all(|m| m.stage != MatchStage::Found),
            "audio another book has is not offered"
        );
    }
}
