//! Tags on items: the user's own, and those that came with an item (its EPUB subjects, an
//! audiobook's genre, a classifier's guess). One the user removes stays removed.
use aoide_library::{BookId, ItemKind, Library, NewBook, TagCount, TagSource};

fn item(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Book,
            path: format!("{}.epub", id.0).into(),
            title: Some("Synthetic item".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        1,
    )
    .unwrap();
    id
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn tags_are_added_listed_with_counts_and_alike_ignoring_case() {
    let lib = Library::open_in_memory().unwrap();
    let a = item(&lib, "a");
    let b = item(&lib, "b");
    lib.add_tags(&a, &names(&["Fantasy", "Sea"]), TagSource::Metadata)
        .unwrap();
    lib.add_tags(&b, &names(&["fantasy", "History"]), TagSource::Metadata)
        .unwrap();
    let tags = lib.book_tags().unwrap();
    assert_eq!(tags[&a], names(&["Fantasy", "Sea"]));
    // The name first used is kept.
    assert_eq!(tags[&b], names(&["Fantasy", "History"]));
    assert_eq!(
        lib.tags().unwrap(),
        [
            TagCount {
                name: "Fantasy".into(),
                books: 2
            },
            TagCount {
                name: "History".into(),
                books: 1
            },
            TagCount {
                name: "Sea".into(),
                books: 1
            },
        ]
    );
}

#[test]
fn the_users_tags_replace_the_shown_ones_and_a_removed_tag_does_not_come_back() {
    let lib = Library::open_in_memory().unwrap();
    let a = item(&lib, "a");
    lib.add_tags(&a, &names(&["Fiction", "General"]), TagSource::Metadata)
        .unwrap();
    lib.set_tags(&a, &names(&["Fiction", "  To read "]))
        .unwrap();
    assert_eq!(lib.book_tags().unwrap()[&a], names(&["Fiction", "To read"]));
    // Read again from its EPUB, or guessed: the removed tag stays away.
    lib.add_tags(&a, &names(&["General", "Sea"]), TagSource::Classifier)
        .unwrap();
    assert_eq!(
        lib.book_tags().unwrap()[&a],
        names(&["Fiction", "Sea", "To read"])
    );
    // The user may put it back.
    lib.set_tags(&a, &names(&["General"])).unwrap();
    assert_eq!(lib.book_tags().unwrap()[&a], names(&["General"]));
    // Tags on no item are not listed.
    assert_eq!(
        lib.tags().unwrap(),
        [TagCount {
            name: "General".into(),
            books: 1
        }]
    );
}

#[test]
fn a_removed_item_takes_its_tags_with_it() {
    let lib = Library::open_in_memory().unwrap();
    let a = item(&lib, "a");
    lib.add_tags(&a, &names(&["Sea"]), TagSource::User).unwrap();
    lib.remove_book(&a).unwrap();
    assert!(lib.book_tags().unwrap().is_empty());
    assert!(lib.tags().unwrap().is_empty());
}

#[test]
fn subject_headings_become_a_few_plain_tags() {
    let raw = names(&[
        "Fiction / Science Fiction / General",
        "FICTION",
        "History -- 20th century.",
        "  Sea   stories ",
        "General",
        "Juvenile fiction; Ships",
        "nyt:combined-print-and-e-book-fiction=2014-01-05",
        "Audiobook",
    ]);
    assert_eq!(
        aoide_library::tidy_tags(&raw),
        names(&[
            "Fiction",
            "Science Fiction",
            "History",
            "20th century",
            "Sea stories",
            "Juvenile fiction",
            "Ships"
        ])
    );
    let many: Vec<String> = (0..20).map(|i| format!("Subject {i}")).collect();
    assert_eq!(aoide_library::tidy_tags(&many).len(), 8);
}

#[test]
fn merging_an_audiobook_into_its_book_brings_its_tags() {
    let lib = Library::open_in_memory().unwrap();
    let book = item(&lib, "book");
    let audio = item(&lib, "audio");
    lib.add_tags(&book, &names(&["Sea"]), TagSource::Metadata)
        .unwrap();
    lib.add_tags(&audio, &names(&["Audiobook", "sea"]), TagSource::Metadata)
        .unwrap();
    lib.merge_audiobook(&audio, &book).unwrap();
    assert_eq!(
        lib.book_tags().unwrap()[&book],
        names(&["Audiobook", "Sea"])
    );
}
