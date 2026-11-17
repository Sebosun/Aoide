//! The pronunciation list: written words said another way, with each spoken word traced back to
//! the written word it stands for.
use aoide_text::{pronounce, Rule};

fn rule(written: &str, spoken: &str) -> Rule {
    Rule {
        written: written.into(),
        spoken: spoken.into(),
        match_case: false,
    }
}

fn said(speech: &str, rules: &[Rule]) -> String {
    pronounce(speech, rules).text
}

#[test]
fn a_rule_changes_whole_words_only() {
    let rules = [rule("art", "ART")];
    assert_eq!(
        said("The art of the start.", &rules),
        "The ART of the start."
    );
    assert_eq!(said("Art, art; (art)", &rules), "ART, ART; (ART)");
    assert_eq!(said("Nothing here.", &rules), "Nothing here.");
}

#[test]
fn a_rule_may_be_several_words_and_keeps_the_punctuation_around_it() {
    let rules = [rule("Dr. Xu", "doctor shoe")];
    assert_eq!(
        said("\"Dr. Xu,\" she said.", &rules),
        "\"doctor shoe,\" she said."
    );
    assert_eq!(said("Dr. Xus", &rules), "doctor shoes");
    // The words must follow each other.
    assert_eq!(said("Dr. and Xu.", &rules), "Dr. and Xu.");
}

#[test]
fn the_longest_rule_wins_then_the_earlier_one() {
    let rules = [
        rule("Xu", "shoe"),
        rule("Dr. Xu", "doctor shoe"),
        rule("Dr.", "doctor"),
        rule("xu", "zoo"),
    ];
    assert_eq!(
        said("Dr. Xu met Xu and Dr. Li.", &rules),
        "doctor shoe met shoe and doctor Li."
    );
}

#[test]
fn case_is_ignored_unless_the_rule_says_otherwise() {
    assert_eq!(
        said("sql and SQL", &[rule("SQL", "sequel")]),
        "sequel and sequel"
    );
    let exact = Rule {
        match_case: true,
        ..rule("US", "you ess")
    };
    assert_eq!(said("US and us", &[exact]), "you ess and us");
}

#[test]
fn a_possessive_or_plural_keeps_its_ending() {
    let rules = [rule("Hermione", "her-MY-oh-nee")];
    assert_eq!(said("Hermione's wand.", &rules), "her-MY-oh-nee's wand.");
    assert_eq!(said("Hermione’s wand.", &rules), "her-MY-oh-nee’s wand.");
    assert_eq!(said("two Hermiones", &rules), "two her-MY-oh-nees");
    // No other ending.
    assert_eq!(said("Hermioned", &rules), "Hermioned");
}

#[test]
fn each_spoken_word_names_the_written_word_it_stands_for() {
    let spoken = pronounce("So Hermione said hi.", &[rule("Hermione", "her my oh nee")]);
    assert_eq!(spoken.text, "So her my oh nee said hi.");
    assert_eq!(spoken.word_of, [0, 1, 1, 1, 1, 2, 3]);

    // Several written words share out the spoken ones in order.
    let spoken = pronounce("Ask Dr. Xu now.", &[rule("Dr. Xu", "doctor shoe")]);
    assert_eq!(spoken.word_of, [0, 1, 2, 3]);
    let spoken = pronounce("Ask Dr. Xu now.", &[rule("Dr. Xu", "Xu")]);
    assert_eq!(
        spoken.word_of,
        [0, 1, 3],
        "the written Xu has no spoken word"
    );
}

#[test]
fn without_a_match_the_text_is_the_same_and_each_word_stands_for_itself() {
    let spoken = pronounce("One  two three.", &[rule("four", "for")]);
    assert_eq!(spoken.text, "One  two three.");
    assert_eq!(spoken.word_of, [0, 1, 2]);
    let spoken = pronounce("One two.", &[rule("  ", "nothing"), rule("", "x")]);
    assert_eq!(spoken.text, "One two.");
}

#[test]
fn a_spoken_form_is_not_said_differently_again() {
    let rules = [rule("Xu", "shoe"), rule("shoe", "boot")];
    assert_eq!(said("Xu's shoe", &rules), "shoe's boot");
}
