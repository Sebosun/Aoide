//! Schema v5: collections, user-made shelves; a book can be on several.
use aoide_library::{BookId, Collection, Library, NewBook};

fn book(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            path: format!("{}.epub", id.0).into(),
            title: Some("Synthetic book".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        10,
    )
    .unwrap();
    id
}

fn names(lib: &Library) -> Vec<(String, u64)> {
    lib.collections()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.books))
        .collect()
}

#[test]
fn collections_are_created_listed_by_name_renamed_and_deleted() {
    let lib = Library::open_in_memory().unwrap();
    let night = lib.create_collection("  Night reading ", 5).unwrap();
    let classics = lib.create_collection("classics", 6).unwrap();
    assert_eq!(
        lib.collections().unwrap(),
        vec![
            Collection {
                id: classics,
                name: "classics".into(),
                created_at: 6,
                books: 0
            },
            Collection {
                id: night,
                name: "Night reading".into(),
                created_at: 5,
                books: 0
            },
        ]
    );
    assert!(
        lib.create_collection("Classics", 7).is_err(),
        "names are unique, ignoring case"
    );
    assert!(
        lib.create_collection("   ", 7).is_err(),
        "names are not empty"
    );

    lib.rename_collection(classics, "Classics").unwrap();
    assert!(lib.rename_collection(classics, "night reading").is_err());
    lib.delete_collection(night).unwrap();
    assert_eq!(names(&lib), vec![("Classics".into(), 0)]);
}

#[test]
fn books_are_put_on_several_collections_and_leave_them_when_removed() {
    let lib = Library::open_in_memory().unwrap();
    let (a, b) = (book(&lib, "a"), book(&lib, "b"));
    let night = lib.create_collection("Night", 1).unwrap();
    let short = lib.create_collection("Short", 2).unwrap();
    lib.set_book_collections(&a, &[night, short, night])
        .unwrap();
    lib.set_book_collections(&b, &[night]).unwrap();
    assert_eq!(names(&lib), vec![("Night".into(), 2), ("Short".into(), 1)]);
    let shelves = lib.book_collections().unwrap();
    assert_eq!(shelves[&a], vec![night, short]);
    assert_eq!(shelves[&b], vec![night]);

    lib.set_book_collections(&a, &[short]).unwrap();
    assert_eq!(lib.book_collections().unwrap()[&a], vec![short]);
    assert!(
        lib.set_book_collections(&a, &[999]).is_err(),
        "unknown collection"
    );

    lib.remove_book(&b).unwrap();
    assert_eq!(names(&lib), vec![("Night".into(), 0), ("Short".into(), 1)]);
    lib.delete_collection(short).unwrap();
    assert!(!lib.book_collections().unwrap().contains_key(&a));
}

#[test]
fn version_four_databases_gain_collections_and_keep_their_books() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("v4.db");
    {
        let lib = Library::open(&path).unwrap();
        book(&lib, "kept");
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE IF EXISTS sync_maps; DROP TABLE IF EXISTS recording_blocks; DROP TABLE IF EXISTS recordings; DROP TABLE IF EXISTS book_collections; DROP TABLE IF EXISTS collections; PRAGMA user_version = 4;")
        .unwrap();
    drop(conn);
    let lib = Library::open(&path).unwrap();
    assert_eq!(lib.books().unwrap().len(), 1);
    let id = lib.create_collection("New", 1).unwrap();
    lib.set_book_collections(&BookId("kept".into()), &[id])
        .unwrap();
    assert_eq!(names(&lib), vec![("New".into(), 1)]);
}
