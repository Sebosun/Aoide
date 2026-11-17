//! Schema v24: a book's series and its place in it.
use aoide_library::{BookId, ItemKind, Library, NewBook};

fn book(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Book,
            path: format!("{}.epub", id.0).into(),
            title: Some("Synthetic book".into()),
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

#[test]
fn a_book_keeps_its_series_when_it_is_stored_again() {
    let lib = Library::open_in_memory().unwrap();
    let id = book(&lib, "a");
    let record = lib.book(&id).unwrap().unwrap();
    assert_eq!((record.series, record.series_position), (None, None));

    lib.set_series(&id, Some(("The Harbour Trilogy", Some(2.5))))
        .unwrap();
    // A scan stores the record again; the series stays.
    book(&lib, "a");
    let record = lib.book(&id).unwrap().unwrap();
    assert_eq!(record.series.as_deref(), Some("The Harbour Trilogy"));
    assert_eq!(record.series_position, Some(2.5));
    assert_eq!(lib.books().unwrap()[0].series_position, Some(2.5));

    lib.set_series(&id, None).unwrap();
    let record = lib.book(&id).unwrap().unwrap();
    assert_eq!((record.series, record.series_position), (None, None));
}

#[test]
fn a_version_23_library_opens_with_no_series() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "ALTER TABLE books DROP COLUMN series; ALTER TABLE books DROP COLUMN series_position;
         PRAGMA user_version = 23;",
    )
    .unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    let id = book(&lib, "a");
    assert_eq!(lib.book(&id).unwrap().unwrap().series, None);
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 25);
}
