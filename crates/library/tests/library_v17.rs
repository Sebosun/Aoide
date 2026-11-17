//! Schema v17: takes of sentences said again.
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

#[test]
fn a_take_is_counted_up_per_sentence() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    assert_eq!(lib.takes(&a).unwrap().take("0:0:0", "The tide came in."), 0);

    assert_eq!(lib.count_take(&a, "0:0:0", "The tide came in.").unwrap(), 1);
    assert_eq!(lib.count_take(&a, "0:0:0", "The tide came in.").unwrap(), 2);
    assert_eq!(lib.count_take(&a, "0:0:1", "Gulls rose.").unwrap(), 1);
    assert_eq!(lib.count_take(&b, "0:0:0", "The tide came in.").unwrap(), 1);

    let takes = lib.takes(&a).unwrap();
    assert_eq!(takes.take("0:0:0", "The tide came in."), 2);
    assert_eq!(takes.take("0:0:1", "Gulls rose."), 1);
    assert_eq!(takes.take("0:0:2", "Unasked."), 0);
}

#[test]
fn a_take_of_other_text_is_ignored() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.count_take(&a, "0:0:0", "The tide came in.").unwrap();
    lib.count_take(&a, "0:0:0", "The tide came in.").unwrap();
    // The sentence now says something else (a new segmenter, a pronunciation).
    assert_eq!(
        lib.takes(&a).unwrap().take("0:0:0", "The tide went out."),
        0
    );
    assert_eq!(
        lib.count_take(&a, "0:0:0", "The tide went out.").unwrap(),
        1
    );
    assert_eq!(lib.takes(&a).unwrap().take("0:0:0", "The tide came in."), 0);
}

#[test]
fn a_version_16_library_opens_with_no_takes_and_counts_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE segment_takes; DROP TABLE pronunciations; DROP TABLE IF EXISTS highlights; PRAGMA user_version = 16;",
    )
    .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let a = book(&lib, "a");
    assert_eq!(lib.takes(&a).unwrap().take("0:0:0", "Hello."), 0);
    assert_eq!(lib.count_take(&a, "0:0:0", "Hello.").unwrap(), 1);
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 25);
}

#[test]
fn removing_a_book_removes_its_takes() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.count_take(&a, "0:0:0", "Hello.").unwrap();
    lib.remove_book(&a).unwrap();
    let a = book(&lib, "a");
    assert_eq!(lib.takes(&a).unwrap().take("0:0:0", "Hello."), 0);
}
