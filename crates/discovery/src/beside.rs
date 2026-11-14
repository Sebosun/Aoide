//! Finding the audiobook someone keeps beside an EPUB, by file and folder names alone.
//!
//! Where: the EPUB's folder and the folder above it, and the folders directly inside those two.
//! Names are compared as sets of words: lower-cased, without apostrophes, a plural `s`, text in
//! brackets, numbers, the author's names, or words such as "unabridged" and "audiobook". A name
//! names the book when it has all of the book's words (from its file name or its title). One
//! with exactly those words beats one with more. A wrong guess is still possible ("Dune" in
//! "Dune Messiah"), so a found audiobook is checked like any other before it syncs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "m4a", "m4b", "mp4", "aac", "flac", "ogg", "oga", "opus", "wav", "wma", "aa", "aax",
    "aaxc", "m4p",
];
/// Words that say what a file is rather than which book it holds, as they read once a plural `s`
/// is dropped ("opus" is "opu").
const NOISE: &[&str] = &[
    "audiobook",
    "audio",
    "book",
    "unabridged",
    "abridged",
    "epub",
    "ebook",
    "mp3",
    "m4b",
    "aac",
    "flac",
    "ogg",
    "opu",
    "part",
    "pt",
    "disc",
    "disk",
    "cd",
    "track",
    "the",
    "a",
    "an",
];

/// The audiobook kept beside an EPUB, as paths to give the recording inspector (audio files, or
/// one folder). Empty when there is none.
pub fn audiobook_beside(epub: &Path, title: Option<&str>, authors: &[String]) -> Vec<PathBuf> {
    Neighbourhood::default().audiobook(epub, title, authors)
}

/// Looks for audiobooks beside many EPUBs, reading each folder once.
#[derive(Default)]
pub struct Neighbourhood {
    listings: HashMap<PathBuf, Vec<Entry>>,
}

#[derive(Clone)]
struct Entry {
    path: PathBuf,
    dir: bool,
}

/// How well a name names the book: exact before looser, then by the share of words in common.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    exact: bool,
    /// Book words per thousand of the name's words.
    share: u32,
}

impl Neighbourhood {
    /// Like [`audiobook_beside`].
    pub fn audiobook(
        &mut self,
        epub: &Path,
        title: Option<&str>,
        authors: &[String],
    ) -> Vec<PathBuf> {
        let Some(folder) = epub.parent() else {
            return Vec::new();
        };
        let book = Book::new(epub, title, authors);
        if book.keys.is_empty() {
            return Vec::new();
        }
        let mut places = vec![folder.to_path_buf()];
        places.extend(folder.parent().map(Path::to_path_buf));
        // (score, what to return, the folder its files are in) in order of preference at a tie.
        let mut best: Option<(Score, Found)> = None;
        let mut consider = |score: Option<Score>, found: Found| {
            if let Some(score) = score {
                match &mut best {
                    Some((s, Found::Files(files))) if *s == score => {
                        if let Found::Files(more) = &found {
                            if files[0].parent() == more[0].parent() {
                                files.extend(more.iter().cloned());
                            }
                        }
                    }
                    Some((s, _)) if *s >= score => {}
                    _ => best = Some((score, found)),
                }
            }
        };
        // Files and folders next to the EPUB, then next to its folder.
        for (i, place) in places.iter().enumerate() {
            let listing = self.list(place);
            for entry in listing.iter().filter(|e| !e.dir && is_audio(&e.path)) {
                consider(
                    book.score(&stem(&entry.path)),
                    Found::Files(vec![entry.path.clone()]),
                );
            }
            for entry in listing.iter().filter(|e| e.dir && e.path != folder) {
                let score = book.score(&name(&entry.path));
                if score.is_some() && self.holds_audio(&entry.path) {
                    consider(score, Found::Folder(entry.path.clone()));
                }
            }
            if i == 0 {
                // Its own folder, when it holds no other EPUB.
                let lone = listing
                    .iter()
                    .filter(|e| !e.dir && is_epub(&e.path))
                    .count()
                    == 1;
                let score = book.score(&name(folder));
                if lone && score.is_some() && self.holds_audio(folder) {
                    consider(score, Found::Folder(folder.to_path_buf()));
                }
            }
        }
        // Files inside the folders next to the EPUB and next to its folder.
        for place in &places {
            let listing = self.list(place);
            for sub in listing.iter().filter(|e| e.dir) {
                let inner = self.list(&sub.path);
                for entry in inner.iter().filter(|e| !e.dir && is_audio(&e.path)) {
                    consider(
                        book.score(&stem(&entry.path)),
                        Found::Files(vec![entry.path.clone()]),
                    );
                }
            }
        }
        match best {
            Some((_, Found::Files(mut files))) => {
                files.sort();
                files.dedup();
                files
            }
            Some((_, Found::Folder(folder))) => vec![folder],
            None => Vec::new(),
        }
    }

    /// A folder's entries, sorted; nothing when it cannot be read. Links are not followed.
    fn list(&mut self, dir: &Path) -> Vec<Entry> {
        if let Some(listing) = self.listings.get(dir) {
            return listing.clone();
        }
        let mut listing: Vec<Entry> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter_map(|e| {
                        let kind = e.file_type().ok()?;
                        (kind.is_dir() || kind.is_file()).then(|| Entry {
                            path: e.path(),
                            dir: kind.is_dir(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        listing.sort_by(|a, b| a.path.cmp(&b.path));
        self.listings.insert(dir.to_path_buf(), listing.clone());
        listing
    }

    /// Whether a folder has audio files in it or in the folders directly inside it, as the
    /// recording inspector looks.
    fn holds_audio(&mut self, dir: &Path) -> bool {
        let listing = self.list(dir);
        listing.iter().any(|e| {
            if e.dir {
                self.list(&e.path)
                    .iter()
                    .any(|f| !f.dir && is_audio(&f.path))
            } else {
                is_audio(&e.path)
            }
        })
    }
}

enum Found {
    Files(Vec<PathBuf>),
    Folder(PathBuf),
}

/// What a name has to match.
struct Book {
    /// Word sets of the file name and the title.
    keys: Vec<Vec<String>>,
    author_words: Vec<String>,
}

impl Book {
    fn new(epub: &Path, title: Option<&str>, authors: &[String]) -> Self {
        let author_words: Vec<String> = authors.iter().flat_map(|a| plain_words(a)).collect();
        let stem = stem(epub);
        let mut keys = Vec::new();
        for name in [Some(stem.as_str()), title].into_iter().flatten() {
            let key = words(name, &author_words);
            if !key.is_empty() && !keys.contains(&key) {
                keys.push(key);
            }
        }
        Self { keys, author_words }
    }

    /// How well a file or folder name names this book, if it does.
    fn score(&self, name: &str) -> Option<Score> {
        let found = words(name, &self.author_words);
        if found.is_empty() {
            return None;
        }
        self.keys
            .iter()
            .filter(|key| key.iter().all(|w| found.contains(w)))
            .map(|key| Score {
                exact: key.len() == found.len(),
                share: (key.len() * 1000 / found.len()) as u32,
            })
            .max()
    }
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

fn name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Lower-case words of letters and digits; apostrophes join ("Keeper's" is "keepers") and a
/// plural `s` is dropped from longer words.
fn plain_words(text: &str) -> Vec<String> {
    text.chars()
        .filter(|c| !matches!(c, '\'' | '’'))
        .collect::<String>()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let w = w.to_lowercase();
            match w.strip_suffix('s') {
                Some(base) if base.chars().count() >= 3 && !base.ends_with('s') => base.to_owned(),
                _ => w,
            }
        })
        .collect()
}

/// The distinct words that tell books apart: no bracketed text, numbers, noise or author names.
fn words(name: &str, author_words: &[String]) -> Vec<String> {
    let mut outside = String::with_capacity(name.len());
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => outside.push(c),
            _ => {}
        }
    }
    let mut found: Vec<String> = Vec::new();
    for w in plain_words(&outside) {
        let keep = !w.chars().all(|c| c.is_ascii_digit())
            && !NOISE.contains(&w.as_str())
            && !author_words.contains(&w)
            && !found.contains(&w);
        if keep {
            found.push(w);
        }
    }
    found
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

fn is_epub(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}
