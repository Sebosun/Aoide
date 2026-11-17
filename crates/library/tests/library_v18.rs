//! Schema v18: the pronunciation lists, one for each book and one for all books.
use aoide_library::{BookId, ItemKind, Library, NewBook};

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

fn said(lib: &Library, book: Option<&BookId>) -> Vec<(String, String, bool)> {
    lib.pronunciations(book)
        .unwrap()
        .into_iter()
        .map(|p| (p.written, p.spoken, p.match_case))
        .collect()
}

#[test]
fn each_list_keeps_one_entry_per_written_word() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let first = lib
        .save_pronunciation(Some(&a), "Xu", "shoe", false, 10)
        .unwrap();
    lib.save_pronunciation(None, "SQL", "sequel", true, 11)
        .unwrap();
    lib.save_pronunciation(None, "Xu", "zoo", false, 12)
        .unwrap();
    // Saving the same written word again changes that entry.
    let changed = lib
        .save_pronunciation(Some(&a), "Xu", "shoo", true, 13)
        .unwrap();
    assert_eq!(changed.id, first.id);
    assert_eq!(changed.created_at, 10);
    lib.save_pronunciation(None, "SQL", "squeal", false, 14)
        .unwrap();

    assert_eq!(said(&lib, Some(&a)), [("Xu".into(), "shoo".into(), true)]);
    assert_eq!(
        said(&lib, None),
        [
            ("SQL".into(), "squeal".into(), false),
            ("Xu".into(), "zoo".into(), false)
        ]
    );
    // A book's own entries come before the ones for all books.
    let rules: Vec<String> = lib
        .pronunciations_for(&a)
        .unwrap()
        .into_iter()
        .map(|p| p.spoken)
        .collect();
    assert_eq!(rules, ["shoo", "squeal", "zoo"]);

    assert_eq!(lib.pronunciation(first.id).unwrap(), Some(changed.clone()));
    assert!(lib.delete_pronunciation(first.id).unwrap());
    assert!(!lib.delete_pronunciation(first.id).unwrap());
    assert!(said(&lib, Some(&a)).is_empty());
}

#[test]
fn removing_a_book_removes_its_list_only() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.save_pronunciation(Some(&a), "Xu", "shoe", false, 1)
        .unwrap();
    lib.save_pronunciation(None, "Xu", "zoo", false, 1).unwrap();
    lib.remove_book(&a).unwrap();
    let a = book(&lib, "a");
    assert!(said(&lib, Some(&a)).is_empty());
    assert_eq!(said(&lib, None).len(), 1);
}

#[test]
fn a_version_17_library_opens_with_empty_lists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE pronunciations; DROP TABLE IF EXISTS highlights; PRAGMA user_version = 17;",
    )
    .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    assert!(said(&lib, None).is_empty());
    lib.save_pronunciation(None, "Xu", "shoe", false, 1)
        .unwrap();
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 24);
}
