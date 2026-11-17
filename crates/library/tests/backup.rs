//! Copies for a backup: consistent while the app writes, and inspected without being changed.
use aoide_library::{BookId, ItemKind, Library, NewBook};

fn book(lib: &Library, id: &str) {
    lib.upsert_book(
        &NewBook {
            id: BookId(id.into()),
            kind: ItemKind::Book,
            path: format!("/home/old/Books/{id}.epub").into(),
            title: Some("Synthetic book".into()),
            authors: vec![],
            language: None,
            cover_path: None,
            error: None,
        },
        1,
    )
    .unwrap();
}

#[test]
fn a_copy_from_a_second_connection_is_consistent_while_the_first_writes() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("library.sqlite3");
    let lib = Library::open(&db).unwrap();
    book(&lib, "a");
    // The app's connection is part way through a write when the copy is made.
    let writer = rusqlite::Connection::open(&db).unwrap();
    writer
        .execute_batch("BEGIN IMMEDIATE; DELETE FROM books;")
        .unwrap();
    let copy = dir.path().join("copy.sqlite3");
    Library::copy_consistent(&db, &copy).unwrap();
    writer.execute_batch("COMMIT").unwrap();

    let copied = Library::open(&copy).unwrap();
    assert_eq!(
        copied.books().unwrap().len(),
        1,
        "the copy is the committed state"
    );
    assert!(lib.books().unwrap().is_empty());
    // An existing file is not overwritten.
    assert!(Library::copy_consistent(&db, &copy).is_err());
}

#[test]
fn a_backups_schema_is_read_without_changing_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("old.sqlite3");
    drop(Library::open(&db).unwrap());
    let set = |version: i64| {
        rusqlite::Connection::open(&db)
            .unwrap()
            .pragma_update(None, "user_version", version)
            .unwrap()
    };
    set(Library::schema_version() + 1);
    assert_eq!(
        Library::schema_version_of(&db).unwrap(),
        Library::schema_version() + 1
    );
    // Newer than this Aoide: refused on open, and still newer after.
    assert!(Library::open(&db).is_err());
    assert_eq!(
        Library::schema_version_of(&db).unwrap(),
        Library::schema_version() + 1
    );
    set(Library::schema_version());
    assert_eq!(
        Library::schema_version_of(&db).unwrap(),
        Library::schema_version()
    );
    assert!(Library::schema_version_of(&dir.path().join("none.sqlite3")).is_err());
}

#[test]
fn paths_under_another_home_are_moved_with_recordings_and_the_scan_cache_cleared() {
    use aoide_library::{RecordingRow, ScannedFile};
    let lib = Library::open_in_memory().unwrap();
    book(&lib, "a");
    let files = r#"["/home/old/Audio/a 1.mp3","/media/disk/a 2.mp3","/home/older/x.mp3"]"#;
    lib.save_recording(&RecordingRow {
        book_id: BookId("a".into()),
        files: files.into(),
        total_ms: 1,
        stage: "ready".into(),
        error: None,
        accepted: false,
        added_at: 1,
    })
    .unwrap();
    lib.store_scanned_files(&[ScannedFile {
        path: "/home/old/Books/a.epub".into(),
        id: BookId("a".into()),
        size: 1,
        modified_ns: 1,
    }])
    .unwrap();
    lib.move_paths("/home/old".as_ref(), "/home/new".as_ref())
        .unwrap();
    let record = lib.book(&BookId("a".into())).unwrap().unwrap();
    assert_eq!(record.path, std::path::Path::new("/home/new/Books/a.epub"));
    let moved = lib.recording(&BookId("a".into())).unwrap().unwrap().files;
    assert_eq!(
        moved,
        r#"["/home/new/Audio/a 1.mp3","/media/disk/a 2.mp3","/home/older/x.mp3"]"#
    );
    lib.clear_scanned_files().unwrap();
    assert!(lib.scanned_files().unwrap().is_empty());
}
