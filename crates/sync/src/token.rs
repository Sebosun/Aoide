use std::ops::Range;

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// One word of text in the form used for matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// Character range of the word in the text it came from.
    pub range: Range<usize>,
    /// Lower case, accents folded, one kind of apostrophe, no surrounding punctuation.
    pub norm: String,
    /// Contains a digit ("1865", "3.5", "21st").
    pub numeric: bool,
}

/// Splits text into words for matching. Punctuation separates words (so compounds split at
/// hyphens, dashes and slashes), apostrophes inside a word are kept, `[12]` note markers and
/// anything without a letter or digit are dropped.
pub fn tokenize(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = note_marker(&chars, i) {
            i = end;
            continue;
        }
        if !is_word(chars[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i + 1;
        loop {
            if end < chars.len() && is_word(chars[end]) {
                end += 1;
            } else if joins(&chars, end) {
                end += 2;
            } else {
                break;
            }
        }
        tokens.push(token(&chars, start..end));
        i = end;
    }
    tokens
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || is_combining_mark(c)
}

fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '’' | 'ʼ')
}

/// Whether the character at `at` continues the word it follows: an apostrophe between letters,
/// or a decimal point or thousands separator between digits.
fn joins(chars: &[char], at: usize) -> bool {
    let (Some(&before), Some(&c), Some(&after)) = (
        at.checked_sub(1).and_then(|b| chars.get(b)),
        chars.get(at),
        chars.get(at + 1),
    ) else {
        return false;
    };
    (is_apostrophe(c) && is_word(before) && is_word(after))
        || (matches!(c, '.' | ',') && before.is_ascii_digit() && after.is_ascii_digit())
}

/// End of a `[digits]` note marker starting at `at`.
fn note_marker(chars: &[char], at: usize) -> Option<usize> {
    if chars[at] != '[' {
        return None;
    }
    let digits = chars[at + 1..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count();
    (digits > 0 && chars.get(at + 1 + digits) == Some(&']')).then_some(at + digits + 2)
}

fn token(chars: &[char], range: Range<usize>) -> Token {
    let lower: String = chars[range.clone()]
        .iter()
        .filter(|c| **c != ',')
        .map(|c| if is_apostrophe(*c) { '\'' } else { *c })
        .flat_map(char::to_lowercase)
        .collect();
    let norm: String = lower.nfkd().filter(|c| !is_combining_mark(*c)).collect();
    let numeric = norm.chars().any(|c| c.is_ascii_digit());
    Token {
        range,
        norm,
        numeric,
    }
}
