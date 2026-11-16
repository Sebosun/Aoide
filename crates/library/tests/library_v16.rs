//! Schema v16: tags on items.
use aoide_library::{BookId, ItemKind, Library, NewBook, TagSource};

#[test]
fn a_version_15_library_opens_with_no_tags_and_takes_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE book_tags; DROP TABLE tags; DROP TABLE IF EXISTS segment_takes; PRAGMA user_version = 15;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    assert!(lib.tags().unwrap().is_empty());
    let id = BookId("a".into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Book,
            path: "a.epub".into(),
            title: Some("Synthetic item".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        1,
    )
    .unwrap();
    lib.add_tags(&id, &["Sea".into()], TagSource::Metadata)
        .unwrap();
    assert_eq!(lib.book_tags().unwrap()[&id], ["Sea"]);
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 17);
}
