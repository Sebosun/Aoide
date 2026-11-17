//! Schema v20: the archive. An archived item stays in the library, with when it was archived.
use aoide_library::{BookId, ItemKind, Library, NewBook};

fn book(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Article,
            path: format!("{}.html", id.0).into(),
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

fn archived(lib: &Library, id: &BookId) -> Option<i64> {
    lib.book(id).unwrap().unwrap().archived_at
}

#[test]
fn an_item_is_archived_and_taken_out_of_the_archive() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    assert_eq!(archived(&lib, &a), None);

    assert!(lib.set_archived(&a, Some(5)).unwrap());
    assert_eq!(archived(&lib, &a), Some(5));
    assert_eq!(archived(&lib, &b), None);
    // Archiving it again keeps when it was first archived.
    assert!(!lib.set_archived(&a, Some(9)).unwrap());
    assert_eq!(archived(&lib, &a), Some(5));
    // Still in the library's items.
    assert_eq!(lib.books().unwrap().len(), 2);

    assert!(lib.set_archived(&a, None).unwrap());
    assert!(!lib.set_archived(&a, None).unwrap());
    assert_eq!(archived(&lib, &a), None);
}

#[test]
fn a_version_19_library_opens_with_nothing_archived() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("ALTER TABLE books DROP COLUMN archived_at; PRAGMA user_version = 19;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let a = book(&lib, "a");
    assert_eq!(archived(&lib, &a), None);
    assert!(lib.set_archived(&a, Some(3)).unwrap());
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 26);
}

#[test]
fn a_collection_counts_only_what_is_not_archived() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    let shelf = lib.create_collection("Later", 1).unwrap();
    lib.set_book_collections(&a, &[shelf]).unwrap();
    lib.set_book_collections(&b, &[shelf]).unwrap();
    assert_eq!(lib.collections().unwrap()[0].books, 2);
    lib.set_archived(&a, Some(2)).unwrap();
    assert_eq!(lib.collections().unwrap()[0].books, 1);
    // Still on it.
    assert_eq!(lib.book_collections().unwrap()[&a], [shelf]);
}
