//! Seeded random books and messy readings of them: invariants and accuracy at scale.

use aoide_sync::{clips, match_book, HeardWord, Timing, Unit};

/// A small deterministic generator (64-bit LCG), so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

const COMMON: &[&str] = &[
    "the", "and", "of", "to", "a", "in", "was", "he", "she", "it", "that", "on", "with", "as",
    "his", "her", "at", "for", "by", "not", "had", "they", "from", "but",
];
const SYLLABLES: &[&str] = &[
    "ka", "lo", "mi", "ther", "an", "vel", "or", "tu", "sen", "ra", "bri", "dom", "el", "fa",
    "gor", "is", "jun", "pe", "qua", "sto",
];

fn vocabulary(rng: &mut Rng, size: usize) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    while words.len() < size {
        let n = 2 + rng.below(2);
        let word: String = (0..n)
            .map(|_| SYLLABLES[rng.below(SYLLABLES.len() as u64) as usize])
            .collect();
        if !words.contains(&word) {
            words.push(word);
        }
    }
    words
}

/// A book: chapters of sentences, each word one token.
fn book(rng: &mut Rng, chapters: usize, words_per_chapter: usize) -> Vec<Vec<String>> {
    let vocab = vocabulary(rng, 1_500);
    (0..chapters)
        .map(|_| {
            (0..words_per_chapter)
                .map(|_| {
                    if rng.chance(45) {
                        COMMON[rng.below(COMMON.len() as u64) as usize].to_owned()
                    } else {
                        // Skewed towards the start of the vocabulary, as real word use is.
                        let r = rng.below(vocab.len() as u64);
                        vocab[(r * r / vocab.len() as u64) as usize].clone()
                    }
                })
                .collect()
        })
        .collect()
}

struct Reading {
    heard: Vec<HeardWord>,
    /// For each chapter word, the index of the heard word it was read as, if read correctly.
    truth: Vec<Vec<Option<usize>>>,
    skipped: Option<usize>,
}

fn read(rng: &mut Rng, book: &[Vec<String>]) -> Reading {
    let mut order: Vec<usize> = (0..book.len()).collect();
    if rng.chance(50) {
        let a = rng.below(book.len() as u64) as usize;
        let b = rng.below(book.len() as u64) as usize;
        order.swap(a, b);
    }
    let skipped = rng
        .chance(50)
        .then(|| rng.below(book.len() as u64) as usize);
    let mut heard = Vec::new();
    let mut truth: Vec<Vec<Option<usize>>> = book.iter().map(|c| vec![None; c.len()]).collect();
    let mut at = 0u64;
    fn say(text: &str, heard: &mut Vec<HeardWord>, at: &mut u64, rng: &mut Rng) {
        let ms = 150 + 50 * text.len() as u64;
        heard.push(HeardWord {
            text: text.to_owned(),
            start_ms: *at,
            end_ms: *at + ms,
        });
        *at += ms + 40 + rng.below(80);
    }
    for _ in 0..30 {
        say("credit", &mut heard, &mut at, rng);
    }
    for &c in &order {
        if Some(c) == skipped {
            continue;
        }
        at += 2_000;
        for (i, word) in book[c].iter().enumerate() {
            let roll = rng.below(100);
            if roll < 3 {
                continue; // Not heard.
            } else if roll < 6 {
                say("zzq", &mut heard, &mut at, rng); // Misheard.
            } else {
                truth[c][i] = Some(heard.len());
                say(word, &mut heard, &mut at, rng);
            }
            if rng.chance(2) {
                say("umm", &mut heard, &mut at, rng); // Heard but not in the text.
            }
        }
    }
    Reading {
        heard,
        truth,
        skipped,
    }
}

fn check(seed: u64, chapters: usize, words: usize) {
    let mut rng = Rng(seed);
    let book = book(&mut rng, chapters, words);
    let reading = read(&mut rng, &book);
    let texts: Vec<String> = book.iter().map(|c| c.join(" ")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let map = match_book(&refs, &reading.heard);
    check_clips(
        seed,
        &texts,
        &map,
        reading.heard.last().unwrap().end_ms + 1_000,
    );

    for (c, chapter) in map.chapters.iter().enumerate() {
        let starts: Vec<u64> = chapter
            .words
            .iter()
            .filter_map(|w| w.timing.span())
            .map(|s| s.0)
            .collect();
        assert!(
            starts.windows(2).all(|w| w[0] <= w[1]),
            "seed {seed}: chapter {c} goes back"
        );
        let read_right = reading.truth[c].iter().filter(|t| t.is_some()).count();
        if Some(c) == reading.skipped {
            assert_eq!(
                map.report.chapters[c].timed_words, 0,
                "seed {seed}: skipped chapter {c}"
            );
            continue;
        }
        let mut correct = 0;
        for (w, truth) in chapter.words.iter().zip(&reading.truth[c]) {
            if let (Timing::Heard { start_ms, .. }, Some(h)) = (w.timing, truth) {
                if reading.heard[*h].start_ms == start_ms {
                    correct += 1;
                }
            }
        }
        assert!(
            correct * 100 >= read_right * 97,
            "seed {seed}: chapter {c}: {correct} of {read_right} placed on the right word"
        );
        assert_eq!(
            map.report.chapters[c].timed_words,
            chapter.words.len(),
            "seed {seed}: chapter {c} has untimed words"
        );
    }
}

/// Units of ten words, and their clips: timed ones tile the recording exactly.
fn check_clips(seed: u64, texts: &[String], map: &aoide_sync::SyncMap, total: u64) {
    let mut units = Vec::new();
    for (c, text) in texts.iter().enumerate() {
        let mut words = Vec::new();
        let mut at = 0;
        for word in text.split(' ') {
            let len = word.chars().count();
            words.push(at..at + len);
            at += len + 1;
        }
        for chunk in words.chunks(10) {
            units.push(Unit {
                chapter: c,
                range: chunk[0].start..chunk[chunk.len() - 1].end,
                words: chunk.to_vec(),
            });
        }
    }
    let clips = clips(map, &units, total, None);
    let mut timed: Vec<_> = clips.iter().filter(|c| c.text.is_some()).collect();
    timed.sort_by_key(|c| c.audio.start);
    assert_eq!(timed[0].audio.start, 0, "seed {seed}");
    assert_eq!(timed.last().unwrap().audio.end, total, "seed {seed}");
    for pair in timed.windows(2) {
        assert_eq!(
            pair[0].audio.end, pair[1].audio.start,
            "seed {seed}: clips must join"
        );
    }
    for clip in &timed {
        let text = clip.text.clone().unwrap();
        assert!(
            clip.audio.start <= text.start && text.end <= clip.audio.end,
            "seed {seed}"
        );
    }
}

#[test]
fn random_books_are_matched_word_for_word() {
    for seed in 1..=12 {
        check(seed, 6, 600);
    }
}

#[test]
fn a_long_book_is_matched() {
    check(99, 30, 3_000);
}
