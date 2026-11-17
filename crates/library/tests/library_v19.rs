//! Schema v19: highlights, the user's passages of a book, kept when the book is removed.
use aoide_library::{BookId, ItemKind, Library, NewBook, NewHighlight};

fn book(lib: &Library, id: &str) -> BookId {
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

fn passage(chapter: u32, start: u32, end: u32, text: &str) -> NewHighlight {
    NewHighlight {
        chapter,
        start,
        end,
        text: text.into(),
        note: String::new(),
    }
}

fn places(lib: &Library, book: &BookId) -> Vec<(u32, u32, u32, String)> {
    lib.highlights(book)
        .unwrap()
        .into_iter()
        .map(|h| (h.chapter, h.start, h.end, h.text))
        .collect()
}

#[test]
fn highlights_are_listed_in_reading_order_and_edited_by_id() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let later = lib
        .add_highlight(&a, &passage(1, 4, 9, "tide"), &[], 10)
        .unwrap();
    let first = lib
        .add_highlight(&a, &passage(0, 20, 30, "the nets"), &[], 11)
        .unwrap();
    lib.add_highlight(&a, &passage(0, 2, 8, "Gulls"), &[], 12)
        .unwrap();
    assert_eq!(
        places(&lib, &a),
        [
            (0, 2, 8, "Gulls".into()),
            (0, 20, 30, "the nets".into()),
            (1, 4, 9, "tide".into())
        ]
    );

    assert!(lib.update_highlight_note(first, "a note").unwrap());
    let found = lib.highlight(first).unwrap().unwrap();
    assert_eq!((found.note.as_str(), &found.book_id), ("a note", &a));
    assert_eq!(found.created_at, 11);

    assert!(lib.move_highlight(later, 6, 11).unwrap());
    assert_eq!(lib.highlight(later).unwrap().unwrap().start, 6);

    assert!(lib.delete_highlight(later).unwrap());
    assert!(!lib.delete_highlight(later).unwrap());
    assert!(lib.highlight(later).unwrap().is_none());
    assert!(!lib.update_highlight_note(later, "gone").unwrap());
}

#[test]
fn a_highlight_can_take_the_place_of_the_ones_it_joins() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let one = lib
        .add_highlight(&a, &passage(0, 0, 5, "Gulls"), &[], 1)
        .unwrap();
    let two = lib
        .add_highlight(&a, &passage(0, 6, 13, "argued"), &[], 2)
        .unwrap();
    let mut joined = passage(0, 0, 13, "Gulls argued");
    joined.note = "kept".into();
    lib.add_highlight(&a, &joined, &[one, two], 3).unwrap();
    let all = lib.highlights(&a).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!((all[0].end, all[0].note.as_str()), (13, "kept"));
}

#[test]
fn a_highlight_survives_removing_its_book_and_returns_with_it() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.add_highlight(&a, &passage(0, 0, 5, "Gulls"), &[], 1)
        .unwrap();
    lib.remove_book(&a).unwrap();
    assert_eq!(places(&lib, &a).len(), 1, "kept without the book");
    let a = book(&lib, "a");
    assert_eq!(places(&lib, &a), [(0, 0, 5, "Gulls".into())]);
}

#[test]
fn a_version_18_library_opens_with_no_highlights() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE highlights; PRAGMA user_version = 18;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let a = book(&lib, "a");
    assert!(places(&lib, &a).is_empty());
    lib.add_highlight(&a, &passage(0, 0, 5, "Gulls"), &[], 1)
        .unwrap();
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 21);
}
