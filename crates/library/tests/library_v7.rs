//! Schema v7: per-book values that go with their book (`book_values`), and typed keys for every
//! stored value.
use aoide_library::{BookId, BookKey, GlobalKey, Library, LibraryError, NewBook};

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
            kind: aoide_library::ItemKind::Book,
        },
        10,
    )
    .unwrap();
    id
}

#[test]
fn a_version_6_library_keeps_every_stored_value_readable_through_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    {
        let lib = Library::open(&path).unwrap();
        book(&lib, "a");
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        r#"DROP TABLE IF EXISTS sites; DROP TABLE IF EXISTS feed_entries; DROP TABLE IF EXISTS feeds; ALTER TABLE books DROP COLUMN shelf; DROP TABLE IF EXISTS articles; ALTER TABLE books DROP COLUMN kind; ALTER TABLE progress DROP COLUMN position_ms; ALTER TABLE bookmarks DROP COLUMN position_ms; DROP TABLE IF EXISTS reading; ALTER TABLE books DROP COLUMN finished_at; ALTER TABLE books DROP COLUMN word_count; DROP TABLE book_values; DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; DROP TABLE IF EXISTS segment_takes; DROP TABLE IF EXISTS pronunciations; PRAGMA user_version = 6;
        INSERT INTO settings (key, value) VALUES
            ('book.a.settings', '{"voice":"Wren"}'),
            ('progress.fraction.a', '0.25'),
            ('book.a.prepared', '12'),
            ('book.gone.prepared', '3'),
            ('library.ignored', '["b"]'),
            ('prepare.queue', '["a"]'),
            ('prepare.paused', '[]'),
            ('prepare.chapters', '{}'),
            ('engine.profile', '{"id":"old"}'),
            ('player.speed', '1.5'),
            ('app.settings', '{}');"#,
    )
    .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let a = BookId("a".into());
    let json = |text: &str| serde_json::from_str::<serde_json::Value>(text).unwrap();
    assert_eq!(
        lib.book_value(&a, BookKey::Settings).unwrap(),
        Some(json(r#"{"voice":"Wren"}"#))
    );
    assert_eq!(
        lib.book_value(&a, BookKey::ProgressFraction).unwrap(),
        Some(0.25f64)
    );
    assert_eq!(
        lib.book_value(&a, BookKey::Prepared).unwrap(),
        Some(12usize)
    );
    assert_eq!(
        lib.value(GlobalKey::IgnoredBooks).unwrap(),
        Some(vec!["b".to_owned()])
    );
    assert_eq!(
        lib.value(GlobalKey::PrepareQueue).unwrap(),
        Some(vec!["a".to_owned()])
    );
    assert_eq!(
        lib.value(GlobalKey::PreparePaused).unwrap(),
        Some(Vec::<String>::new())
    );
    assert_eq!(
        lib.value(GlobalKey::PrepareChapters).unwrap(),
        Some(json("{}"))
    );
    assert_eq!(
        lib.value(GlobalKey::OldEngineProfile).unwrap(),
        Some(json(r#"{"id":"old"}"#))
    );
    assert_eq!(lib.value(GlobalKey::OldPlayerSpeed).unwrap(), Some(1.5f32));
    assert_eq!(
        lib.value(GlobalKey::OldAppSettings).unwrap(),
        Some(json("{}"))
    );
    assert_eq!(
        lib.book_value::<usize>(&BookId("gone".into()), BookKey::Prepared)
            .unwrap(),
        None,
        "a mark for a book no longer in the library is dropped"
    );
}

#[test]
fn removing_a_book_removes_its_prepared_mark_and_keeps_its_voice_and_place() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.set_book_value(&a, BookKey::Settings, &serde_json::json!({"voice": "Wren"}))
        .unwrap();
    lib.set_book_value(&a, BookKey::ProgressFraction, &0.5f64)
        .unwrap();
    lib.set_book_value(&a, BookKey::Prepared, &7usize).unwrap();

    lib.remove_book(&a).unwrap();
    let a = book(&lib, "a");

    assert_eq!(
        lib.book_value::<usize>(&a, BookKey::Prepared).unwrap(),
        None
    );
    assert_eq!(
        lib.book_value(&a, BookKey::ProgressFraction).unwrap(),
        Some(0.5f64)
    );
    assert!(lib
        .book_value::<serde_json::Value>(&a, BookKey::Settings)
        .unwrap()
        .is_some());
}

#[test]
fn values_are_set_replaced_and_removed_and_a_value_of_another_type_is_an_error() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.set_book_value(&a, BookKey::Prepared, &3usize).unwrap();
    lib.set_book_value(&a, BookKey::Prepared, &4usize).unwrap();
    assert_eq!(lib.book_value(&a, BookKey::Prepared).unwrap(), Some(4usize));
    assert!(matches!(
        lib.book_value::<Vec<String>>(&a, BookKey::Prepared),
        Err(LibraryError::Corrupt(_))
    ));
    lib.remove_book_value(&a, BookKey::Prepared).unwrap();
    assert_eq!(
        lib.book_value::<usize>(&a, BookKey::Prepared).unwrap(),
        None
    );

    lib.set_value(GlobalKey::IgnoredBooks, &["x"]).unwrap();
    assert_eq!(
        lib.value(GlobalKey::IgnoredBooks).unwrap(),
        Some(vec!["x".to_owned()])
    );
    lib.remove_value(GlobalKey::IgnoredBooks).unwrap();
    assert_eq!(
        lib.value::<Vec<String>>(GlobalKey::IgnoredBooks).unwrap(),
        None
    );
}
