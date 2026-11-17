//! The pronunciation list: written words said another way. The page keeps the book's spelling;
//! only the text sent to the speech engine changes, and each word of it is traced back to the
//! written word it stands for, so word timings can be folded back onto the page's words.

/// Say `written` as `spoken`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rule {
    /// One or more words, matched as whole words (whitespace between them matches any).
    pub written: String,
    /// Plain respelling, sent to the engine as it is.
    pub spoken: String,
    /// Upper and lower case must match exactly; otherwise case is ignored.
    pub match_case: bool,
}

/// Speech text with the rules applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spoken {
    pub text: String,
    /// For each whitespace-separated word of `text`, the index of the word of the input it
    /// stands for.
    pub word_of: Vec<usize>,
}

impl Spoken {
    /// Spans of the spoken words (as an aligner or an engine gives them) folded back onto the
    /// `written` words of the input: one span from the first start to the last end of the
    /// spoken words standing for each, `None` for a written word no spoken word was placed for.
    pub fn fold(&self, spans: &[Option<(u32, u32)>], written: usize) -> Vec<Option<(u32, u32)>> {
        let mut folded: Vec<Option<(u32, u32)>> = vec![None; written];
        for (span, of) in spans.iter().zip(&self.word_of) {
            let (Some((start, end)), Some(slot)) = (span, folded.get_mut(*of)) else {
                continue;
            };
            *slot = Some(slot.map_or((*start, *end), |(s, e)| (s.min(*start), e.max(*end))));
        }
        folded
    }
}

/// Says `speech` with `rules` applied: whole words only (a word boundary is where an
/// alphanumeric character meets one that is not), the longest `written` first, then in list
/// order (the caller puts a book's rules before the rules for all books). A plural or
/// possessive ending ("Xu's", "Xus") is kept after the spoken form; no other inflection is
/// matched. What a rule says is not said differently again. Without a match the text is
/// returned as it is. Deterministic: the same input and rules give the same output.
pub fn pronounce(speech: &str, rules: &[Rule]) -> Spoken {
    let chars: Vec<char> = speech.chars().collect();
    let mut word_index = Vec::with_capacity(chars.len());
    let mut words = 0;
    for (k, c) in chars.iter().enumerate() {
        if c.is_whitespace() {
            word_index.push(None);
        } else {
            if k == 0 || chars[k - 1].is_whitespace() {
                words += 1;
            }
            word_index.push(Some(words - 1));
        }
    }
    let mut ordered: Vec<(Vec<char>, Vec<&str>, bool)> = rules
        .iter()
        .filter_map(|rule| {
            let written: Vec<char> = rule
                .written
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .collect();
            written.iter().any(|c| c.is_alphanumeric()).then(|| {
                (
                    written,
                    rule.spoken.split_whitespace().collect(),
                    rule.match_case,
                )
            })
        })
        .collect();
    ordered.sort_by_key(|(written, _, _)| std::cmp::Reverse(written.len()));

    // Each character said, with the written word it stands for (`None` for whitespace).
    let mut said: Vec<(char, Option<usize>)> = Vec::with_capacity(chars.len());
    let mut matched = false;
    let mut i = 0;
    while i < chars.len() {
        let found = ordered.iter().find_map(|(written, spoken, match_case)| {
            match_at(&chars, i, written, *match_case).map(|end| (end, spoken))
        });
        let Some((end, spoken)) = found else {
            said.push((chars[i], word_index[i]));
            i += 1;
            continue;
        };
        matched = true;
        let mut covered: Vec<usize> = word_index[i..end].iter().flatten().copied().collect();
        covered.dedup();
        for (k, word) in spoken.iter().enumerate() {
            if k > 0 {
                said.push((' ', None));
            }
            let of = covered[k * covered.len() / spoken.len()];
            said.extend(word.chars().map(|c| (c, Some(of))));
        }
        i = end;
    }
    if !matched {
        return Spoken {
            text: speech.to_owned(),
            word_of: (0..words).collect(),
        };
    }
    let mut text = String::with_capacity(said.len());
    let mut word_of = Vec::new();
    let mut previous_space = true;
    for (c, of) in said {
        match of {
            None => previous_space = true,
            Some(of) => {
                if previous_space {
                    if !word_of.is_empty() {
                        text.push(' ');
                    }
                    word_of.push(of);
                    previous_space = false;
                }
                text.push(c);
            }
        }
    }
    Spoken { text, word_of }
}

/// Where a match of `written` starting at `start` ends, if there is one with word boundaries
/// on both sides. A plural `s` after it may follow; it stays in the text.
fn match_at(chars: &[char], start: usize, written: &[char], match_case: bool) -> Option<usize> {
    if !boundary(chars, start) {
        return None;
    }
    let mut at = start;
    for w in written {
        if *w == ' ' {
            let spaces = chars[at..].iter().take_while(|c| c.is_whitespace()).count();
            if spaces == 0 {
                return None;
            }
            at += spaces;
        } else {
            let c = *chars.get(at)?;
            if !(c == *w || !match_case && c.to_lowercase().eq(w.to_lowercase())) {
                return None;
            }
            at += 1;
        }
    }
    let plural = matches!(chars.get(at), Some('s' | 'S'));
    (boundary(chars, at) || plural && boundary(chars, at + 1)).then_some(at)
}

/// Whether a word boundary is at `at`: not between two alphanumeric characters.
fn boundary(chars: &[char], at: usize) -> bool {
    at == 0
        || at >= chars.len()
        || !(chars[at - 1].is_alphanumeric() && chars[at].is_alphanumeric())
}
