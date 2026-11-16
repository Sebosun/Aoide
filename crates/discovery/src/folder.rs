//! Suggesting audiobooks kept in chosen folders, by names close to a book's.
//!
//! The folders are searched once (as *Find books* searches for EPUBs: in parallel, skipping
//! hidden and build folders, links and other drives) into audiobooks: a folder that holds one
//! book's audio files (a folder of discs counts as one), or the files in a folder of many books
//! that share a name ("Salt Road - Part 1.mp3", "Salt Road - Part 2.mp3"). Names are compared as
//! sets of words, as beside an EPUB (see `beside`), but they only have to come close: enough
//! words in common, most of the book's among them, a misspelt long word allowed. A suggestion is
//! only that; pairing it checks it like any other recording.
//!
//! A folder of numbered files ("01 Introduction.mp3", "02 Chapter 1.mp3") is one audiobook,
//! however their titles differ, unless each file is the size of a book. Then
//! [`AudiobookFolder::classify`] hears a few files of each (their headers only) and drops what is
//! music or a clip; files of one album in a folder become one audiobook.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use rayon::prelude::*;

use crate::classify::{verdict, AudioFacts, Classified, Evidence, Verdict};
use crate::names::{is_audio, name, plain_words, stem, words, Book};

/// Suggestions per book at most.
const MAX_SUGGESTIONS: usize = 3;
/// Audio smaller than this, all its files together, is not an audiobook: a game's or an app's
/// sounds, a ringtone. 3 MiB is about twenty minutes of speech at a low bitrate; a book split
/// into many short chapters counts by its whole size.
pub const MIN_AUDIOBOOK_BYTES: u64 = 3 * 1024 * 1024;
/// Words two names must share to come close, per thousand of their words (the Dice coefficient:
/// twice the shared words over both counts). Besides, more than half the book's own words must
/// be there, so a shared series name ("Ember Watch") or a few small words are not enough.
const CLOSE: u32 = 500;

/// Numbered files this size each, on average, are books of a series rather than one book's
/// parts.
const BOOK_SIZED_BYTES: u64 = 200 * 1024 * 1024;

/// An audio file and its size.
type AudioFile = (PathBuf, u64);

/// The audiobooks found in some folders, searched once and asked about many books.
pub struct AudiobookFolder {
    audiobooks: Vec<Audiobook>,
    /// Shorter audio is not an audiobook (see [`crate::ScanOptions::min_audiobook_ms`]).
    min_ms: u64,
}

struct Audiobook {
    /// Paths to give the recording inspector: audio files, or one folder.
    paths: Vec<PathBuf>,
    /// The folder it is in, or is.
    dir: PathBuf,
    /// The names it goes by: its folder's or file's, and its files' when they share one.
    names: Vec<String>,
    /// The words of the folders it is in, below the searched one (an author's folder, say).
    place: Vec<String>,
    /// Its audio files, sorted, and their size together.
    files: Vec<AudioFile>,
    bytes: u64,
    verdict: Verdict,
    duration_ms: Option<u64>,
    author: Option<String>,
}

impl Audiobook {
    fn suggestion(&self, exact: bool) -> Suggestion {
        Suggestion {
            paths: self.paths.clone(),
            exact,
            files: self.files.iter().map(|(p, _)| p.clone()).collect(),
            bytes: self.bytes,
            verdict: self.verdict,
            duration_ms: self.duration_ms,
            author: self.author.clone(),
        }
    }

    /// The files to hear: the first, the middle and the last.
    fn samples(&self) -> Vec<&AudioFile> {
        let n = self.files.len();
        let mut at = vec![0, n / 2, n.saturating_sub(1)];
        at.dedup();
        at.into_iter().filter_map(|i| self.files.get(i)).collect()
    }
}

/// An audiobook whose name comes close to a book's.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    /// Paths to give the recording inspector: audio files, or one folder.
    pub paths: Vec<PathBuf>,
    /// Its name has exactly the book's words.
    pub exact: bool,
    /// The audio files it holds (in a folder: all of them, discs included), sorted.
    pub files: Vec<PathBuf>,
    /// Their size together.
    pub bytes: u64,
    /// Whether it sounds like an audiobook (doubtful until classified).
    pub verdict: Verdict,
    /// Its length, from the files heard.
    pub duration_ms: Option<u64>,
    /// The artist its files name.
    pub author: Option<String>,
}

/// How close a name comes to the book's: exact before close, then by words in common.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Closeness {
    exact: bool,
    share: u32,
}

impl AudiobookFolder {
    /// Searches one folder; one that cannot be read holds no audiobooks.
    pub fn read(root: &Path) -> Self {
        let options = crate::ScanOptions {
            roots: vec![root.to_path_buf()],
            ..Default::default()
        };
        Self::scan(&options, &AtomicBool::new(false), &|_| {})
    }

    /// Searches the folders `options` names, reporting progress as [`crate::find_epubs`] does
    /// (`found` counts audio files). A set `cancel` stops early, with what was found so far.
    pub fn scan(
        options: &crate::ScanOptions,
        cancel: &AtomicBool,
        progress: &(dyn Fn(crate::Progress) + Sync),
    ) -> Self {
        let files = crate::find_files(options, cancel, progress, &is_audio);
        // Audio files by folder; a BTreeMap so a folder's subfolders follow it.
        let mut by_dir: BTreeMap<PathBuf, Vec<AudioFile>> = BTreeMap::new();
        for found in files {
            if let Some(dir) = found.path.parent() {
                by_dir
                    .entry(dir.to_path_buf())
                    .or_default()
                    .push((found.path, found.size));
            }
        }
        let mut folder = Self {
            audiobooks: Vec::new(),
            min_ms: options.min_audiobook_ms,
        };
        for (dir, files) in &by_dir {
            let Some(root) = options.roots.iter().filter(|r| dir.starts_with(r)).max() else {
                continue;
            };
            // Whether a folder below holds audio too (then this one is not one book's).
            let more_below = by_dir
                .range::<PathBuf, _>((std::ops::Bound::Excluded(dir), std::ops::Bound::Unbounded))
                .next()
                .is_some_and(|(next, _)| next.starts_with(dir));
            folder.add_folder(root, dir, files, more_below);
        }
        folder
            .audiobooks
            .retain(|a| a.bytes >= options.min_audiobook_bytes);
        folder
    }

    /// The audiobooks whose names come close to the book's (from its file name or its title),
    /// closest first; only those with exactly its words when there are any.
    pub fn suggest(&self, epub: &Path, title: Option<&str>, authors: &[String]) -> Vec<Suggestion> {
        self.suggest_each([(epub, title, authors)])
            .pop()
            .unwrap_or_default()
    }

    /// [`Self::suggest`] for several books at once (as `(epub, title, authors)`), except that an
    /// audiobook goes only to the books it comes closest to: "The Salt Road.m4b" is left out
    /// for *The Salt Road Home* when *The Salt Road* is among them too. Books it comes equally
    /// close to all keep it.
    pub fn suggest_each<'a>(
        &self,
        books: impl IntoIterator<Item = (&'a Path, Option<&'a str>, &'a [String])>,
    ) -> Vec<Vec<Suggestion>> {
        let ranked: Vec<Vec<(Closeness, bool, usize)>> = books
            .into_iter()
            .map(|(epub, title, authors)| {
                let found = self.ranked(&Book::new(epub, title, authors));
                // A name with exactly the book's words leaves no doubt: the close ones are
                // left out.
                let exact = found.iter().any(|(c, _, _)| c.exact);
                found
                    .into_iter()
                    .filter(|(c, _, _)| c.exact || !exact)
                    .collect()
            })
            .collect();
        // Each audiobook's best closeness to any of the books.
        let mut best: Vec<Option<Closeness>> = vec![None; self.audiobooks.len()];
        for &(closeness, _, at) in ranked.iter().flatten() {
            best[at] = best[at].max(Some(closeness));
        }
        ranked
            .into_iter()
            .map(|found| {
                found
                    .into_iter()
                    .filter(|&(closeness, _, at)| best[at] == Some(closeness))
                    .take(MAX_SUGGESTIONS)
                    .map(|(closeness, _, at)| self.audiobooks[at].suggestion(closeness.exact))
                    .collect()
            })
            .collect()
    }

    /// Every audiobook found, by path (not suggested to any book: `exact` is false).
    pub fn all(&self) -> Vec<Suggestion> {
        let mut all: Vec<Suggestion> = self
            .audiobooks
            .iter()
            .map(|audiobook| audiobook.suggestion(false))
            .collect();
        all.sort_by(|a, b| a.paths.cmp(&b.paths));
        all
    }

    /// Every audiobook that comes close to `book` (by its index), closest first: by closeness,
    /// then those in a folder named for the book's author, then by path.
    fn ranked(&self, book: &Book) -> Vec<(Closeness, bool, usize)> {
        if book.keys.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(Closeness, bool, usize)> = self
            .audiobooks
            .iter()
            .enumerate()
            .filter_map(|(at, audiobook)| {
                let closeness = audiobook
                    .names
                    .iter()
                    .filter_map(|n| closeness(book, n))
                    .max()?;
                let by_author = audiobook
                    .place
                    .iter()
                    .any(|w| book.author_words.contains(w));
                Some((closeness, by_author, at))
            })
            .collect();
        found.sort_by(|a, b| {
            (b.0, b.1)
                .cmp(&(a.0, a.1))
                .then_with(|| self.audiobooks[a.2].paths.cmp(&self.audiobooks[b.2].paths))
        });
        found
    }

    /// The audiobooks among one folder's audio files.
    fn add_folder(&mut self, root: &Path, dir: &Path, files: &[AudioFile], more_below: bool) {
        // Audio files by the words of their names; "01.mp3" has none.
        let mut groups: Vec<(Vec<String>, Vec<AudioFile>)> = Vec::new();
        for file in files {
            let key = words(&stem(&file.0), &[]);
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, group)) => group.push(file.clone()),
                None => groups.push((key, vec![file.clone()])),
            }
        }
        let named: Vec<&(Vec<String>, Vec<AudioFile>)> =
            groups.iter().filter(|(k, _)| !k.is_empty()).collect();
        let parts = named.len() > 1 && numbered(files);
        if dir != root && (named.len() <= 1 || parts) && !more_below {
            // One book's files: the folder is the audiobook, or the folder above when this is
            // one of its discs ("CD1", "Part 2": no words of its own).
            let disc = is_disc(&name(dir));
            let folder = match dir.parent() {
                Some(parent) if disc && parent != root => parent,
                _ => dir,
            };
            let mut names = vec![name(folder)];
            if !parts {
                names.extend(named.first().map(|(_, group)| stem(&group[0].0)));
            }
            let folder = folder.to_path_buf();
            self.add(root, folder.clone(), vec![folder], names, files);
        } else {
            for (_, group) in named {
                let mut paths: Vec<PathBuf> = group.iter().map(|(p, _)| p.clone()).collect();
                paths.sort();
                let names = vec![stem(&paths[0])];
                self.add(root, dir.to_path_buf(), paths, names, group);
            }
        }
    }

    /// Adds an audiobook, or more files to one already found (a folder's next disc).
    fn add(
        &mut self,
        root: &Path,
        dir: PathBuf,
        paths: Vec<PathBuf>,
        names: Vec<String>,
        files: &[AudioFile],
    ) {
        let bytes: u64 = files.iter().map(|(_, size)| size).sum();
        if let Some(known) = self.audiobooks.iter_mut().find(|a| a.paths == paths) {
            known.files.extend_from_slice(files);
            known.files.sort();
            known.bytes += bytes;
            return;
        }
        let place = dir
            .strip_prefix(root)
            .map(|p| plain_words(&p.to_string_lossy()))
            .unwrap_or_default();
        let mut files = files.to_vec();
        files.sort();
        self.audiobooks.push(Audiobook {
            paths,
            dir,
            names,
            place,
            files,
            bytes,
            verdict: Verdict::Doubtful,
            duration_ms: None,
            author: None,
        });
    }

    /// Hears a few files of each audiobook (`listen` reads a file's headers, on several threads)
    /// and keeps what sounds like one: drops music, clips and sounds, and joins the files of one
    /// album in a folder into one audiobook, named for the album too.
    pub fn classify(
        &mut self,
        listen: &(dyn Fn(&Path) -> Option<AudioFacts> + Sync),
    ) -> Classified {
        let samples: Vec<&AudioFile> = self
            .audiobooks
            .iter()
            .flat_map(Audiobook::samples)
            .collect();
        let heard: HashMap<&Path, (u64, AudioFacts)> = samples
            .into_par_iter()
            .filter_map(|(path, size)| Some((path.as_path(), (*size, listen(path)?))))
            .collect();
        let mut heard_of: Vec<Vec<(u64, AudioFacts)>> = self
            .audiobooks
            .iter()
            .map(|a| {
                a.samples()
                    .iter()
                    .filter_map(|(path, _)| heard.get(path.as_path()).cloned())
                    .collect()
            })
            .collect();
        drop(heard);
        self.join_albums(&mut heard_of);

        let mut classified = Classified::default();
        let min_ms = self.min_ms;
        for (audiobook, heard) in self.audiobooks.iter_mut().zip(&heard_of) {
            let evidence = Evidence {
                names: audiobook
                    .names
                    .iter()
                    .flat_map(|n| plain_words(n))
                    .collect(),
                place: audiobook.place.clone(),
                extensions: audiobook
                    .files
                    .iter()
                    .filter_map(|(p, _)| p.extension())
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .collect(),
                files: audiobook.files.len(),
                bytes: audiobook.bytes,
                heard,
                min_ms,
            };
            audiobook.verdict = verdict(&evidence);
            audiobook.duration_ms = evidence.duration_ms();
            audiobook.author = heard.iter().find_map(|(_, f)| f.artist.clone());
            match audiobook.verdict {
                Verdict::Audiobook => classified.audiobooks += 1,
                Verdict::Doubtful => classified.doubtful += 1,
                Verdict::NotAudiobook => classified.dropped += 1,
            }
        }
        self.audiobooks
            .retain(|a| a.verdict != Verdict::NotAudiobook);
        classified
    }

    /// Joins audiobooks of loose files in one folder whose files all name the same album, and
    /// names the joined audiobook for its album too. (A single audiobook keeps its own names:
    /// an album tag may name a series, or be left over from another file.)
    fn join_albums(&mut self, heard_of: &mut Vec<Vec<(u64, AudioFacts)>>) {
        let album = |heard: &[(u64, AudioFacts)]| -> Option<String> {
            let first = heard.first()?.1.album.as_deref()?.trim().to_lowercase();
            heard
                .iter()
                .all(|(_, f)| {
                    f.album.as_deref().map(|a| a.trim().to_lowercase()) == Some(first.clone())
                })
                .then_some(first)
                .filter(|a| !a.is_empty())
        };
        let mut at = 0;
        while at < self.audiobooks.len() {
            let Some(key) = album(&heard_of[at]) else {
                at += 1;
                continue;
            };
            let loose = |a: &Audiobook| a.paths != [a.dir.clone()];
            if loose(&self.audiobooks[at]) {
                let mut other = at + 1;
                while other < self.audiobooks.len() {
                    let same = loose(&self.audiobooks[other])
                        && self.audiobooks[other].dir == self.audiobooks[at].dir
                        && album(&heard_of[other]).as_ref() == Some(&key);
                    if same {
                        let joined = self.audiobooks.remove(other);
                        let joined_heard = heard_of.remove(other);
                        let into = &mut self.audiobooks[at];
                        into.paths.extend(joined.paths);
                        into.paths.sort();
                        into.files.extend(joined.files);
                        into.files.sort();
                        into.bytes += joined.bytes;
                        for name in joined.names {
                            if !into.names.contains(&name) {
                                into.names.push(name);
                            }
                        }
                        heard_of[at].extend(joined_heard);
                        // What joins them is their album: it is a name the audiobook goes by.
                        if let Some(name) = heard_of[at][0].1.album.clone() {
                            if !into.names.contains(&name) {
                                into.names.push(name);
                            }
                        }
                    } else {
                        other += 1;
                    }
                }
            }
            at += 1;
        }
    }
}

/// Whether a folder's files are numbered parts of one book: past the words they all begin
/// with ("Mark 00a …", "Mark 01 …"), most begin with a number ("01", "00b") or a part word and
/// one ("Chapter 3"), and they are not each the size of a book.
fn numbered(files: &[AudioFile]) -> bool {
    if files.len() < 2 {
        return false;
    }
    let bytes: u64 = files.iter().map(|(_, size)| size).sum();
    if bytes / files.len() as u64 >= BOOK_SIZED_BYTES {
        return false;
    }
    let tokens: Vec<Vec<String>> = files
        .iter()
        .map(|(path, _)| {
            stem(path)
                .to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .collect();
    let shared = (0..)
        .take_while(|&i| {
            tokens[0]
                .get(i)
                .is_some_and(|t| tokens.iter().all(|other| other.get(i) == Some(t)))
        })
        .count();
    const PART: &[&str] = &[
        "chapter", "ch", "part", "pt", "track", "lecture", "lesson", "episode", "ep", "session",
        "section", "disc", "cd",
    ];
    let number = |t: &str| {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        (1..=4).contains(&digits) && t.chars().count() <= digits + 1
    };
    let count = tokens
        .iter()
        .filter(|t| match t.get(shared..) {
            Some([first, ..]) if number(first) => true,
            Some([word, next, ..]) => PART.contains(&word.as_str()) && number(next),
            _ => false,
        })
        .count();
    count * 5 >= files.len() * 4
}

/// Small words that many names share, left out when comparing ("Method and Madness" is not
/// close to "Migrations and Methods").
const SMALL: &[&str] = &[
    "and", "or", "of", "in", "on", "at", "to", "for", "with", "from", "by", "as", "is",
];

/// The words that tell names apart: without small words, unless that leaves none.
fn telling(words: Vec<String>) -> Vec<String> {
    let kept: Vec<String> = words
        .iter()
        .filter(|w| !SMALL.contains(&w.as_str()))
        .cloned()
        .collect();
    if kept.is_empty() {
        words
    } else {
        kept
    }
}

/// How close a name comes to the book's, if close enough.
fn closeness(book: &Book, name: &str) -> Option<Closeness> {
    let found = telling(words(name, &book.author_words));
    if found.is_empty() {
        return None;
    }
    book.keys
        .iter()
        .map(|key| telling(key.clone()))
        .filter_map(|key| {
            let shared = key
                .iter()
                .filter(|k| found.iter().any(|w| alike(k, w)))
                .count();
            let closeness = Closeness {
                exact: shared == key.len() && key.len() == found.len(),
                share: (2 * shared * 1000 / (key.len() + found.len())) as u32,
            };
            (closeness.share >= CLOSE && 2 * shared > key.len()).then_some(closeness)
        })
        .max()
}

/// Whether a folder's name only numbers a disc or part: "CD1", "Disc 2", "Part 03", "1".
fn is_disc(name: &str) -> bool {
    const DISC: &[&str] = &["cd", "disc", "disk", "part", "pt", "vol", "volume", "side"];
    plain_words(name).iter().all(|w| {
        let w = w.trim_matches(|c: char| c.is_ascii_digit());
        w.is_empty() || DISC.contains(&w)
    })
}

/// The same word, or a long one misspelt by one letter.
fn alike(a: &str, b: &str) -> bool {
    a == b || (a.chars().count() >= 5 && b.chars().count() >= 5 && one_edit(a, b))
}

/// Whether one letter added, dropped or changed turns `a` into `b`.
fn one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if long.len() - short.len() > 1 {
        return false;
    }
    let start = short
        .iter()
        .zip(long.iter())
        .take_while(|(x, y)| x == y)
        .count();
    if short.len() == long.len() {
        short[start + 1..] == long[start + 1..]
    } else {
        short[start..] == long[start + 1..]
    }
}
