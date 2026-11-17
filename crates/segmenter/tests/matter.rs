//! Front and back matter proposed by chapter title, for leaving out of the narration. Only a
//! proposal: nothing is left out until the user agrees.
use aoide_text::front_and_back_matter;

fn proposed(titles: &[&str]) -> Vec<usize> {
    front_and_back_matter(titles, Some("en"))
}

#[test]
fn the_usual_front_and_back_matter_is_proposed() {
    let titles = [
        "Copyright",
        "Contents",
        "Chapter 1",
        "Chapter 2",
        "Acknowledgements",
        "Notes",
        "Bibliography",
        "Index",
        "About the Author",
        "Also by Mira Vale",
    ];
    assert_eq!(proposed(&titles), [0, 1, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn case_punctuation_and_spelling_do_not_matter() {
    let titles = [
        "COPYRIGHT ©",
        "Table of Contents",
        "  acknowledgments. ",
        "Endnotes:",
        "About the authors",
        "ALSO BY THE SAME AUTHOR",
        "Copyright page",
        "Praise for The Quay",
    ];
    assert_eq!(proposed(&titles), [0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn chapters_that_only_begin_with_a_listed_word_are_kept() {
    let titles = [
        "Notes from the Lighthouse",
        "Index of Tides",
        "Contents of the Chest",
        "The Copyright Thief",
    ];
    assert!(proposed(&titles).is_empty());
}

#[test]
fn nothing_is_proposed_for_a_novel_whose_chapters_are_all_numbered() {
    let titles = ["One", "Two", "Three", "Chapter 4", "5", "XII", ""];
    assert!(proposed(&titles).is_empty());
}

#[test]
fn without_a_language_the_english_titles_are_still_known() {
    assert_eq!(front_and_back_matter(&["Contents", "One"], None), [0]);
    assert_eq!(
        front_and_back_matter(&["Contents", "One"], Some("en-GB")),
        [0]
    );
}

#[test]
fn a_few_other_languages_have_their_own_titles() {
    assert_eq!(
        front_and_back_matter(
            &["Inhaltsverzeichnis", "Erstes Kapitel", "Danksagung"],
            Some("de")
        ),
        [0, 2]
    );
    assert_eq!(
        front_and_back_matter(
            &["Table des matières", "Un", "Remerciements"],
            Some("fr-FR")
        ),
        [0, 2]
    );
    assert_eq!(
        front_and_back_matter(&["Índice", "Uno", "Agradecimientos"], Some("es")),
        [0, 2]
    );
}
