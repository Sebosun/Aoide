//! Comparing file and folder names with a book's title, as sets of words.

use std::path::Path;

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

/// What a name has to match.
pub(crate) struct Book {
    /// Word sets of the file name and the title.
    pub(crate) keys: Vec<Vec<String>>,
    pub(crate) author_words: Vec<String>,
}

impl Book {
    pub(crate) fn new(epub: &Path, title: Option<&str>, authors: &[String]) -> Self {
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
}

pub(crate) fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

pub(crate) fn name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

/// Lower-case words of letters and digits; apostrophes join ("Keeper's" is "keepers") and a
/// plural `s` is dropped from longer words.
pub(crate) fn plain_words(text: &str) -> Vec<String> {
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
pub(crate) fn words(name: &str, author_words: &[String]) -> Vec<String> {
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

pub(crate) fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.iter().any(|a| a.eq_ignore_ascii_case(e)))
}

pub(crate) fn is_epub(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("epub"))
}
