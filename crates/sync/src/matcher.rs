//! The matcher. In outline:
//!
//! 1. Both sides are tokenised the same way and interned to ids.
//! 2. Each chapter is located in the transcript by votes: its word sequences that occur once in
//!    the whole book and once in the whole transcript vote for an offset, and the densest window
//!    of offsets wins. Chapters are placed strongest first and cannot share transcript.
//!    Chapters too short to vote keep their book-order position.
//! 3. The placed chapters, in recording order, form one sequence. Their vote pairs, chained by a
//!    longest increasing subsequence, are the first anchors.
//! 4. Gaps between anchors are refined recursively with sequences unique within the gap, and
//!    small gaps are aligned word by word (Needleman–Wunsch with fuzzy and number matches).
//! 5. Spurious pairs are cleaned up, words get times, and unmatched words between timed ones
//!    are placed by their letters when the time between them allows it.

use std::collections::HashMap;
use std::ops::Range;

use crate::{
    tokenize, AudioGap, ChapterReport, ChapterSync, HeardWord, Report, SyncMap, TimedWord, Timing,
    Token,
};

/// Words in a voting and anchoring sequence.
const ANCHOR: usize = 5;
/// Fewest votes that place a chapter.
const MIN_VOTES: usize = 3;
/// Chapters with fewer words cannot be located by votes; they keep their book-order place.
const WEAK_CHAPTER: usize = 25;
/// Gaps up to this many cells are aligned word by word straight away.
const DP_CELLS: usize = 1_000_000;
/// Gaps up to this many cells are aligned word by word when no anchor splits them.
const MAX_DP_CELLS: usize = 16_000_000;
/// Unmatched narration shorter than this is not reported.
const GAP_REPORT_MS: u64 = 3_000;
/// Words of an unmatched stretch quoted in the report.
const EXCERPT_WORDS: usize = 12;

// Alignment costs, doubled so they are integers.
const COST_FUZZY: u32 = 1;
const COST_NUMBER: u32 = 1;
const COST_SUBSTITUTE: u32 = 3;
const COST_GAP: u32 = 2;
/// Longest run of number words read for one written number ("one thousand eight hundred and …").
const MAX_NUMBER_WORDS: usize = 7;

/// Matches the heard words to the chapters' display texts.
pub fn match_book(chapters: &[&str], heard: &[HeardWord]) -> SyncMap {
    let mut vocab = Vocab::default();
    let tokens: Vec<Vec<Token>> = chapters.iter().map(|text| tokenize(text)).collect();
    let book: Vec<Vec<Tok>> = tokens
        .iter()
        .map(|chapter| chapter.iter().map(|t| vocab.tok(t)).collect())
        .collect();
    let heard_toks = heard_tokens(heard, &mut vocab);
    let h: Vec<Tok> = heard_toks.iter().map(|t| t.tok).collect();

    let located = locate(&book, &h);
    let order = sequence(&book, &located);

    // The placed chapters laid end to end, in recording order.
    let mut b: Vec<Tok> = Vec::new();
    let mut origin: Vec<(usize, usize)> = Vec::new();
    let mut anchors: Vec<(usize, usize)> = Vec::new();
    for &chapter in &order {
        let base = b.len();
        if let Placement::Placed(pairs) = &located[chapter] {
            anchors.extend(pairs.iter().map(|&(i, j)| (base + i, j)));
        }
        b.extend_from_slice(&book[chapter]);
        origin.extend((0..book[chapter].len()).map(|i| (chapter, i)));
    }
    let chain = lis(&anchors);
    let seeds = expand(&chain, ANCHOR);

    let aligner = Aligner {
        b: &b,
        h: &h,
        vocab: &vocab,
    };
    let mut pairs = Vec::new();
    let mut from = (0, 0);
    let last = seeds.len();
    for (n, seed) in seeds.iter().enumerate() {
        pairs.extend(aligner.fill(from.0..seed.b.start, from.1..seed.h.start, n == 0, false));
        pairs.push(seed.clone());
        from = (seed.b.end, seed.h.end);
    }
    pairs.extend(aligner.fill(from.0..b.len(), from.1..h.len(), last == 0, true));
    let pairs = clean(pairs);

    let mut timings: Vec<Vec<Timing>> =
        book.iter().map(|c| vec![Timing::Unread; c.len()]).collect();
    let mut placed_heard = vec![false; heard_toks.len()];
    for pair in &pairs {
        let start = heard_toks[pair.h.start].start;
        let end = heard_toks[pair.h.end - 1].end;
        placed_heard[pair.h.clone()]
            .iter_mut()
            .for_each(|p| *p = true);
        let letters: Vec<u64> = pair.b.clone().map(|i| vocab.letters(b[i].id)).collect();
        for (k, span) in share(start, end, &letters).into_iter().enumerate() {
            let (chapter, index) = origin[pair.b.start + k];
            timings[chapter][index] = match pair.kind {
                Kind::Substitute => Timing::Estimated {
                    start_ms: span.0,
                    end_ms: span.1,
                },
                Kind::Exact | Kind::Fuzzy | Kind::Number => Timing::Heard {
                    start_ms: span.0,
                    end_ms: span.1,
                },
            };
        }
    }
    for (chapter, timing) in timings.iter_mut().enumerate() {
        let letters: Vec<u64> = book[chapter].iter().map(|t| vocab.letters(t.id)).collect();
        fill_between(timing, &letters);
    }

    let report = report(&timings, heard, &heard_toks, &placed_heard);
    SyncMap {
        chapters: tokens
            .into_iter()
            .zip(timings)
            .map(|(tokens, timings)| ChapterSync {
                words: tokens
                    .into_iter()
                    .zip(timings)
                    .map(|(t, timing)| TimedWord {
                        range: t.range,
                        timing,
                    })
                    .collect(),
            })
            .collect(),
        report,
    }
}

// ---------------------------------------------------------------- tokens

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tok {
    id: u32,
    numeric: bool,
}

#[derive(Default)]
struct Vocab {
    ids: HashMap<String, u32>,
    words: Vec<String>,
}

impl Vocab {
    fn tok(&mut self, token: &Token) -> Tok {
        let next = self.words.len() as u32;
        let id = *self.ids.entry(token.norm.clone()).or_insert(next);
        if id == next {
            self.words.push(token.norm.clone());
        }
        Tok {
            id,
            numeric: token.numeric,
        }
    }

    fn word(&self, id: u32) -> &str {
        &self.words[id as usize]
    }

    fn letters(&self, id: u32) -> u64 {
        self.word(id).chars().count().max(1) as u64
    }

    fn is_number_word(&self, id: u32) -> bool {
        NUMBER_WORDS.contains(&self.word(id))
    }

    fn similar(&self, a: u32, b: u32) -> bool {
        let (a, b): (Vec<char>, Vec<char>) = (
            self.word(a).chars().collect(),
            self.word(b).chars().collect(),
        );
        let longest = a.len().max(b.len());
        if a.len().min(b.len()) < 4 || a.len().abs_diff(b.len()) * 5 > longest {
            return false;
        }
        levenshtein(&a, &b) * 5 <= longest
    }
}

struct HeardTok {
    tok: Tok,
    start: u64,
    end: u64,
    /// Index of the heard word it came from.
    word: usize,
}

/// Tokens of the heard words; a word that splits into several tokens shares its span by letters.
fn heard_tokens(heard: &[HeardWord], vocab: &mut Vocab) -> Vec<HeardTok> {
    let mut out = Vec::new();
    for (index, word) in heard.iter().enumerate() {
        let tokens = tokenize(&word.text);
        let letters: Vec<u64> = tokens
            .iter()
            .map(|t| t.norm.chars().count() as u64)
            .collect();
        let end = word.end_ms.max(word.start_ms);
        for (token, (start, end)) in tokens.iter().zip(share(word.start_ms, end, &letters)) {
            out.push(HeardTok {
                tok: vocab.tok(token),
                start,
                end,
                word: index,
            });
        }
    }
    out
}

/// Splits `start..end` into consecutive spans in proportion to `weights`.
fn share(start: u64, end: u64, weights: &[u64]) -> Vec<(u64, u64)> {
    let total: u64 = weights.iter().sum::<u64>().max(1);
    let span = end.saturating_sub(start);
    let mut cumulative = 0;
    let mut from = start;
    weights
        .iter()
        .map(|w| {
            cumulative += w;
            let to = start + span * cumulative / total;
            let piece = (from, to.max(from));
            from = to.max(from);
            piece
        })
        .collect()
}

// ---------------------------------------------------------------- locating chapters

enum Placement {
    /// Located in the recording; vote pairs as (index in chapter, heard index), increasing.
    Placed(Vec<(usize, usize)>),
    /// Too short to locate by votes.
    Weak,
    /// Not in the recording.
    Absent,
}

fn gram_key(toks: &[Tok]) -> Option<u64> {
    if toks.iter().any(|t| t.numeric) {
        return None;
    }
    Some(toks.iter().fold(0xcbf2_9ce4_8422_2325, |h: u64, t| {
        (h ^ u64::from(t.id)).wrapping_mul(0x0000_0100_0000_01b3)
    }))
}

/// Positions of each n-gram key in `toks`, and how often it occurs.
fn gram_index(toks: &[Tok], n: usize) -> HashMap<u64, (u32, usize)> {
    let mut index: HashMap<u64, (u32, usize)> = HashMap::new();
    if toks.len() >= n {
        for i in 0..=toks.len() - n {
            if let Some(key) = gram_key(&toks[i..i + n]) {
                let entry = index.entry(key).or_insert((0, i));
                entry.0 += 1;
            }
        }
    }
    index
}

fn locate(book: &[Vec<Tok>], h: &[Tok]) -> Vec<Placement> {
    let heard = gram_index(h, ANCHOR);
    let mut book_counts: HashMap<u64, u32> = HashMap::new();
    for chapter in book {
        for (key, (count, _)) in gram_index(chapter, ANCHOR) {
            *book_counts.entry(key).or_default() += count;
        }
    }
    // Votes per chapter, strongest cluster first.
    let mut clusters: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
    let mut placement: Vec<Placement> = Vec::with_capacity(book.len());
    for (c, chapter) in book.iter().enumerate() {
        if chapter.len() < WEAK_CHAPTER {
            placement.push(Placement::Weak);
            continue;
        }
        let mut votes = Vec::new();
        for i in 0..=chapter.len() - ANCHOR {
            let gram = &chapter[i..i + ANCHOR];
            let Some(key) = gram_key(gram) else { continue };
            if book_counts.get(&key) != Some(&1) {
                continue;
            }
            if let Some(&(1, j)) = heard.get(&key) {
                if h[j..j + ANCHOR] == *gram {
                    votes.push((i, j));
                }
            }
        }
        let grams = chapter.len() + 1 - ANCHOR;
        let cluster = densest(&votes, 200 + chapter.len() / 5);
        if cluster.len() >= MIN_VOTES.max(grams / 20) {
            clusters.push((c, lis(&cluster)));
        }
        placement.push(Placement::Absent);
    }
    clusters.sort_by_key(|(c, pairs)| (std::cmp::Reverse(pairs.len()), *c));
    let mut claimed: Vec<Range<usize>> = Vec::new();
    for (c, pairs) in clusters {
        let free: Vec<(usize, usize)> = pairs
            .into_iter()
            .filter(|&(_, j)| {
                !claimed
                    .iter()
                    .any(|r| r.contains(&j) || r.contains(&(j + ANCHOR - 1)))
            })
            .collect();
        let grams = book[c].len() + 1 - ANCHOR;
        if free.len() < MIN_VOTES.max(grams / 20) {
            continue;
        }
        let region = free[0].1..free[free.len() - 1].1 + ANCHOR;
        if claimed
            .iter()
            .any(|r| r.start < region.end && region.start < r.end)
        {
            continue;
        }
        claimed.push(region);
        placement[c] = Placement::Placed(free);
    }
    placement
}

/// The votes whose offsets (heard index minus chapter index) fall in the densest window.
fn densest(votes: &[(usize, usize)], width: usize) -> Vec<(usize, usize)> {
    let mut by_offset: Vec<(i64, usize)> = votes
        .iter()
        .enumerate()
        .map(|(k, &(i, j))| (j as i64 - i as i64, k))
        .collect();
    by_offset.sort_unstable();
    let (mut best, mut best_from, mut from) = (0, 0, 0);
    for to in 0..by_offset.len() {
        while by_offset[to].0 - by_offset[from].0 > width as i64 {
            from += 1;
        }
        if to + 1 - from > best {
            best = to + 1 - from;
            best_from = from;
        }
    }
    let mut chosen: Vec<(usize, usize)> = by_offset[best_from..best_from + best]
        .iter()
        .map(|&(_, k)| votes[k])
        .collect();
    chosen.sort_unstable();
    chosen
}

/// Chapters in recording order: placed ones by where they were found, weak ones after the
/// chapter before them in the book. Absent chapters are left out.
fn sequence(book: &[Vec<Tok>], located: &[Placement]) -> Vec<usize> {
    let mut placed: Vec<(usize, usize)> = located
        .iter()
        .enumerate()
        .filter_map(|(c, p)| match p {
            Placement::Placed(pairs) => Some((pairs[0].1, c)),
            _ => None,
        })
        .collect();
    placed.sort_unstable();
    let mut order: Vec<usize> = placed.into_iter().map(|(_, c)| c).collect();
    for (c, p) in located.iter().enumerate() {
        if !matches!(p, Placement::Weak) || book[c].is_empty() {
            continue;
        }
        let at = order
            .iter()
            .enumerate()
            .filter(|(_, &o)| o < c)
            .max_by_key(|(_, &o)| o)
            .map_or(0, |(k, _)| k + 1);
        order.insert(at, c);
    }
    order
}

/// Longest chain of pairs increasing in both positions. Input must be sorted by the first.
fn lis(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![usize::MAX; pairs.len()];
    for (k, &(i, j)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&t| pairs[t].1 < j && pairs[t].0 < i);
        if at > 0 {
            previous[k] = tails[at - 1];
        }
        if at == tails.len() {
            tails.push(k);
        } else {
            tails[at] = k;
        }
    }
    let mut chain = Vec::with_capacity(tails.len());
    let mut k = tails.last().copied().unwrap_or(usize::MAX);
    while k != usize::MAX {
        chain.push(pairs[k]);
        k = previous[k];
    }
    chain.reverse();
    chain
}

// ---------------------------------------------------------------- pairing

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Exact,
    Fuzzy,
    /// A written number and the words it was read as.
    Number,
    /// Different words in the same place.
    Substitute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Pair {
    b: Range<usize>,
    h: Range<usize>,
    kind: Kind,
}

impl Pair {
    fn one(b: usize, h: usize, kind: Kind) -> Self {
        Pair {
            b: b..b + 1,
            h: h..h + 1,
            kind,
        }
    }
}

/// Word pairs covered by a chain of `n`-word anchors, kept strictly increasing.
fn expand(chain: &[(usize, usize)], n: usize) -> Vec<Pair> {
    let mut pairs: Vec<Pair> = Vec::new();
    for &(i, j) in chain {
        for k in 0..n {
            let after = pairs
                .last()
                .is_none_or(|p| i + k >= p.b.end && j + k >= p.h.end);
            if after {
                pairs.push(Pair::one(i + k, j + k, Kind::Exact));
            }
        }
    }
    pairs
}

struct Aligner<'a> {
    b: &'a [Tok],
    h: &'a [Tok],
    vocab: &'a Vocab,
}

impl Aligner<'_> {
    /// Pairs within a gap. `free_prefix`/`free_suffix`: heard words before/after the text may go
    /// unmatched at no cost (credits at either end of the recording).
    fn fill(
        &self,
        b: Range<usize>,
        h: Range<usize>,
        free_prefix: bool,
        free_suffix: bool,
    ) -> Vec<Pair> {
        if b.is_empty() || h.is_empty() {
            return Vec::new();
        }
        let cells = b.len() * h.len();
        if cells <= DP_CELLS {
            return self.align(b, h, free_prefix, free_suffix);
        }
        for n in (2..=4).rev() {
            let chain = lis(&self.unique_pairs(&b, &h, n));
            if chain.is_empty() {
                continue;
            }
            let seeds = expand(&chain, n);
            let mut pairs = Vec::new();
            let mut from = (b.start, h.start);
            for (k, seed) in seeds.iter().enumerate() {
                pairs.extend(self.fill(
                    from.0..seed.b.start,
                    from.1..seed.h.start,
                    free_prefix && k == 0,
                    false,
                ));
                pairs.push(seed.clone());
                from = (seed.b.end, seed.h.end);
            }
            pairs.extend(self.fill(from.0..b.end, from.1..h.end, false, free_suffix));
            return pairs;
        }
        if cells <= MAX_DP_CELLS {
            return self.align(b, h, free_prefix, free_suffix);
        }
        Vec::new()
    }

    /// `n`-word sequences occurring once in each range, as (book index, heard index), sorted.
    fn unique_pairs(&self, b: &Range<usize>, h: &Range<usize>, n: usize) -> Vec<(usize, usize)> {
        let heard = gram_index(&self.h[h.clone()], n);
        let mut pairs: Vec<(usize, usize)> = gram_index(&self.b[b.clone()], n)
            .into_iter()
            .filter_map(|(key, (count, i))| {
                let &(heard_count, j) = heard.get(&key)?;
                let same =
                    self.b[b.start + i..b.start + i + n] == self.h[h.start + j..h.start + j + n];
                (count == 1 && heard_count == 1 && same).then_some((b.start + i, h.start + j))
            })
            .collect();
        pairs.sort_unstable();
        pairs
    }

    /// Word-by-word alignment of a gap (Needleman–Wunsch).
    fn align(
        &self,
        b: Range<usize>,
        h: Range<usize>,
        free_prefix: bool,
        free_suffix: bool,
    ) -> Vec<Pair> {
        let (n, m) = (b.len(), h.len());
        let width = m + 1;
        let mut cost = vec![u32::MAX; (n + 1) * width];
        // Move into each cell: 0 diagonal, 1 book word unmatched, 2 heard word unmatched,
        // 3 + k a written book number read as k heard words, 3 + MAX + k the other way round.
        let mut moves = vec![0u8; (n + 1) * width];
        cost[0] = 0;
        for j in 1..=m {
            cost[j] = if free_prefix { 0 } else { j as u32 * COST_GAP };
            moves[j] = 2;
        }
        for i in 1..=n {
            let row = i * width;
            // At the start of the recording, text before the first match may go unread.
            cost[row] = if free_prefix { 0 } else { i as u32 * COST_GAP };
            moves[row] = 1;
            let bt = self.b[b.start + i - 1];
            for j in 1..=m {
                let ht = self.h[h.start + j - 1];
                let mut best = cost[row - width + j - 1] + self.pair_cost(bt, ht);
                let mut how = 0u8;
                // At the end of the recording, text after the last match may go unread.
                let free = free_suffix && j == m;
                let up = cost[row - width + j] + if free { 0 } else { COST_GAP };
                if up < best {
                    best = up;
                    how = 1;
                }
                let free = free_suffix && i == n;
                let left = cost[row + j - 1] + if free { 0 } else { COST_GAP };
                if left < best {
                    best = left;
                    how = 2;
                }
                if bt.numeric {
                    for k in 1..=MAX_NUMBER_WORDS.min(j) {
                        if !self.vocab.is_number_word(self.h[h.start + j - k].id) {
                            break;
                        }
                        let via = cost[row - width + j - k] + COST_NUMBER;
                        if via < best {
                            best = via;
                            how = 2 + k as u8;
                        }
                    }
                }
                if ht.numeric {
                    for k in 1..=MAX_NUMBER_WORDS.min(i) {
                        if !self.vocab.is_number_word(self.b[b.start + i - k].id) {
                            break;
                        }
                        let via = cost[row - k * width + j - 1] + COST_NUMBER;
                        if via < best {
                            best = via;
                            how = 2 + MAX_NUMBER_WORDS as u8 + k as u8;
                        }
                    }
                }
                cost[row + j] = best;
                moves[row + j] = how;
            }
        }
        let mut pairs = Vec::new();
        let (mut i, mut j) = (n, m);
        while i > 0 || j > 0 {
            let how = moves[i * width + j] as usize;
            match how {
                0 => {
                    let (bi, hj) = (b.start + i - 1, h.start + j - 1);
                    let kind = self.pair_kind(self.b[bi], self.h[hj]);
                    pairs.push(Pair::one(bi, hj, kind));
                    i -= 1;
                    j -= 1;
                }
                1 => i -= 1,
                2 => j -= 1,
                k if k <= 2 + MAX_NUMBER_WORDS => {
                    let k = k - 2;
                    pairs.push(Pair {
                        b: b.start + i - 1..b.start + i,
                        h: h.start + j - k..h.start + j,
                        kind: Kind::Number,
                    });
                    i -= 1;
                    j -= k;
                }
                k => {
                    let k = k - 2 - MAX_NUMBER_WORDS;
                    pairs.push(Pair {
                        b: b.start + i - k..b.start + i,
                        h: h.start + j - 1..h.start + j,
                        kind: Kind::Number,
                    });
                    i -= k;
                    j -= 1;
                }
            }
        }
        pairs.reverse();
        pairs
    }

    fn pair_kind(&self, b: Tok, h: Tok) -> Kind {
        if b.id == h.id {
            Kind::Exact
        } else if !b.numeric && !h.numeric && self.vocab.similar(b.id, h.id) {
            Kind::Fuzzy
        } else {
            Kind::Substitute
        }
    }

    fn pair_cost(&self, b: Tok, h: Tok) -> u32 {
        match self.pair_kind(b, h) {
            Kind::Exact => 0,
            Kind::Fuzzy => COST_FUZZY,
            Kind::Number => COST_NUMBER,
            Kind::Substitute => COST_SUBSTITUTE,
        }
    }
}

/// Drops pairs that are probably coincidences: long runs of substitutions (different text read
/// in the same place) and tiny islands of matches inside unmatched stretches.
fn clean(pairs: Vec<Pair>) -> Vec<Pair> {
    let runs = runs(&pairs);
    let mut keep = vec![true; pairs.len()];
    // Substitution runs of four or more.
    let mut k = 0;
    while k < pairs.len() {
        if pairs[k].kind != Kind::Substitute {
            k += 1;
            continue;
        }
        let mut end = k + 1;
        while end < pairs.len()
            && pairs[end].kind == Kind::Substitute
            && touching(&pairs[end - 1], &pairs[end])
        {
            end += 1;
        }
        if end - k >= 4 {
            keep[k..end].iter_mut().for_each(|x| *x = false);
        }
        k = end;
    }
    // Islands: runs with at most two matched words, far from other pairs on both sides.
    for run in &runs {
        let matched = run
            .clone()
            .filter(|&k| keep[k] && pairs[k].kind != Kind::Substitute)
            .count();
        if matched > 2 {
            continue;
        }
        let first = &pairs[run.start];
        let last = &pairs[run.end - 1];
        let before = run.start.checked_sub(1).map(|p| &pairs[p]);
        let after = pairs.get(run.end);
        let far = |a: Option<&Pair>, gap: &dyn Fn(&Pair) -> (usize, usize)| {
            a.is_none_or(|p| {
                let (bg, hg) = gap(p);
                bg >= 3 || hg >= 3
            })
        };
        let lonely_before = far(before, &|p| {
            (first.b.start - p.b.end, first.h.start - p.h.end)
        });
        let lonely_after = far(after, &|p| (p.b.start - last.b.end, p.h.start - last.h.end));
        if lonely_before && lonely_after && (before.is_some() || after.is_some()) {
            run.clone().for_each(|k| keep[k] = false);
        }
    }
    pairs
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(p))
        .collect()
}

fn touching(a: &Pair, b: &Pair) -> bool {
    a.b.end == b.b.start && a.h.end == b.h.start
}

/// Maximal runs of touching pairs.
fn runs(pairs: &[Pair]) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start = 0;
    for k in 1..=pairs.len() {
        if k == pairs.len() || !touching(&pairs[k - 1], &pairs[k]) {
            if start < k {
                runs.push(start..k);
            }
            start = k;
        }
    }
    runs
}

// ---------------------------------------------------------------- timing and report

/// Whether `letters` could have been read in `ms`: neither much faster than fast speech nor much
/// slower than a slow narrator with a pause.
fn plausible(ms: u64, letters: u64) -> bool {
    ms + 1_000 >= letters * 30 && ms <= letters * 150 + 2_000
}

/// Gives unread words a time when they sit between timed words and the time between them is
/// plausible for their letters; short runs at a chapter's edges are placed next to their
/// neighbour.
fn fill_between(timings: &mut [Timing], letters: &[u64]) {
    let mut k = 0;
    while k < timings.len() {
        if timings[k] != Timing::Unread {
            k += 1;
            continue;
        }
        let start = k;
        while k < timings.len() && timings[k] == Timing::Unread {
            k += 1;
        }
        let run = start..k;
        let total: u64 = letters[run.clone()].iter().sum();
        let before = start.checked_sub(1).and_then(|p| timings[p].span());
        let after = timings.get(k).and_then(Timing::span);
        let span = match (before, after) {
            (Some((_, from)), Some((to, _))) if to >= from && plausible(to - from, total) => {
                Some((from, to))
            }
            (None, Some((to, _))) if run.len() <= 3 => {
                Some((to.saturating_sub(total * 70 + 100), to))
            }
            (Some((_, from)), None) if run.len() <= 3 => Some((from, from + total * 70 + 100)),
            _ => None,
        };
        if let Some((from, to)) = span {
            for (i, (s, e)) in run.clone().zip(share(from, to, &letters[run])) {
                timings[i] = Timing::Estimated {
                    start_ms: s,
                    end_ms: e,
                };
            }
        }
    }
}

fn report(
    timings: &[Vec<Timing>],
    heard: &[HeardWord],
    toks: &[HeardTok],
    placed: &[bool],
) -> Report {
    let chapters: Vec<ChapterReport> = timings
        .iter()
        .map(|chapter| {
            let spans: Vec<(u64, u64)> = chapter.iter().filter_map(Timing::span).collect();
            ChapterReport {
                words: chapter.len(),
                matched_words: chapter
                    .iter()
                    .filter(|t| matches!(t, Timing::Heard { .. }))
                    .count(),
                timed_words: spans.len(),
                span: spans.first().zip(spans.last()).map(|(f, l)| (f.0, l.1)),
            }
        })
        .collect();
    // A heard word counts when any of its tokens was placed in the text.
    let mut word_placed: Vec<Option<bool>> = vec![None; heard.len()];
    for (tok, &p) in toks.iter().zip(placed) {
        let state = word_placed[tok.word].get_or_insert(false);
        *state |= p;
    }
    let mut unmatched_audio = Vec::new();
    let mut run: Vec<usize> = Vec::new();
    let mut flush = |run: &mut Vec<usize>| {
        if let (Some(&first), Some(&last)) = (run.first(), run.last()) {
            if heard[last].end_ms.saturating_sub(heard[first].start_ms) >= GAP_REPORT_MS {
                unmatched_audio.push(AudioGap {
                    start_ms: heard[first].start_ms,
                    end_ms: heard[last].end_ms,
                    excerpt: run
                        .iter()
                        .take(EXCERPT_WORDS)
                        .map(|&w| heard[w].text.trim())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
            }
        }
        run.clear();
    };
    for (w, state) in word_placed.iter().enumerate() {
        match state {
            Some(false) => run.push(w),
            Some(true) => flush(&mut run),
            None => {}
        }
    }
    flush(&mut run);
    Report {
        words: chapters.iter().map(|c| c.words).sum(),
        matched_words: chapters.iter().map(|c| c.matched_words).sum(),
        timed_words: chapters.iter().map(|c| c.timed_words).sum(),
        heard_words: word_placed.iter().filter(|s| s.is_some()).count(),
        matched_heard_words: word_placed.iter().filter(|s| **s == Some(true)).count(),
        chapters,
        unmatched_audio,
    }
}

// ---------------------------------------------------------------- helpers

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = (above + 1)
                .min(row[j] + 1)
                .min(diagonal + usize::from(ca != cb));
            diagonal = above;
        }
    }
    row[b.len()]
}

const NUMBER_WORDS: &[&str] = &[
    "zero",
    "oh",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
    "thirty",
    "forty",
    "fifty",
    "sixty",
    "seventy",
    "eighty",
    "ninety",
    "hundred",
    "thousand",
    "million",
    "billion",
    "and",
    "point",
    "first",
    "second",
    "third",
    "fourth",
    "fifth",
    "sixth",
    "seventh",
    "eighth",
    "ninth",
    "tenth",
    "eleventh",
    "twelfth",
    "thirteenth",
    "fourteenth",
    "fifteenth",
    "sixteenth",
    "seventeenth",
    "eighteenth",
    "nineteenth",
    "twentieth",
    "thirtieth",
    "fortieth",
    "fiftieth",
    "sixtieth",
    "seventieth",
    "eightieth",
    "ninetieth",
    "hundredth",
    "thousandth",
];
