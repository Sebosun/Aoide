use aoide_sync::tokenize;

fn norms(text: &str) -> Vec<String> {
    tokenize(text).into_iter().map(|t| t.norm).collect()
}

#[test]
fn words_are_lower_case_without_surrounding_punctuation() {
    assert_eq!(
        norms("“Well,” said Ana. (Fine!)"),
        ["well", "said", "ana", "fine"]
    );
}

#[test]
fn ranges_are_character_offsets_into_the_text() {
    let tokens = tokenize("Żółw — ma “dom”.");
    let ranges: Vec<_> = tokens.iter().map(|t| t.range.clone()).collect();
    assert_eq!(ranges, [0..4, 7..9, 11..14]);
}

#[test]
fn compounds_split_at_hyphens_dashes_and_slashes() {
    assert_eq!(
        norms("a well-known tale—and/or not"),
        ["a", "well", "known", "tale", "and", "or", "not"]
    );
}

#[test]
fn apostrophes_inside_words_are_kept_as_one_kind() {
    assert_eq!(
        norms("Don’t ’tis the boys’ rock'n'roll"),
        ["don't", "tis", "the", "boys", "rock'n'roll"]
    );
}

#[test]
fn accents_are_folded() {
    assert_eq!(norms("Café naïve ÉLAN"), ["cafe", "naive", "elan"]);
}

#[test]
fn note_markers_and_marks_without_letters_are_dropped() {
    assert_eq!(norms("the end.[12] * * * Next"), ["the", "end", "next"]);
}

#[test]
fn numbers_keep_decimal_points_lose_thousands_separators_and_are_marked() {
    let tokens = tokenize("In 1865, 3.5 and 1,000 on the 21st.");
    let pairs: Vec<_> = tokens
        .iter()
        .map(|t| (t.norm.as_str(), t.numeric))
        .collect();
    assert_eq!(
        pairs,
        [
            ("in", false),
            ("1865", true),
            ("3.5", true),
            ("and", false),
            ("1000", true),
            ("on", false),
            ("the", false),
            ("21st", true),
        ]
    );
}
