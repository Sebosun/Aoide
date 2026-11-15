//! Schema v8: when a book was finished, how many words it has, and reading time per day.
use aoide_library::{BookId, Library, NewBook, Progress};

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
fn a_version_7_library_keeps_its_books_progress_and_listening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    {
        let lib = Library::open(&path).unwrap();
        let a = book(&lib, "a");
        lib.save_progress(
            &a,
            &Progress {
                chapter: 2,
                char_offset: 40,
                segment_id: Some("2:1:0".into()),
                updated_at: 5,
                position_ms: None,
            },
        )
        .unwrap();
        lib.record_listening(&a, "2026-11-10", 60_000).unwrap();
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE IF EXISTS articles; ALTER TABLE books DROP COLUMN kind; ALTER TABLE progress DROP COLUMN position_ms; ALTER TABLE bookmarks DROP COLUMN position_ms;
         DROP TABLE reading;
         ALTER TABLE books DROP COLUMN finished_at;
         ALTER TABLE books DROP COLUMN word_count;
         PRAGMA user_version = 7;",
    )
    .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let a = BookId("a".into());
    let record = lib.book(&a).unwrap().unwrap();
    assert_eq!((record.finished_at, record.word_count), (None, None));
    assert_eq!(lib.progress(&a).unwrap().unwrap().chapter, 2);
    let stats = lib.activity_stats().unwrap();
    assert_eq!(stats.listening_ms, 60_000);
    assert_eq!(stats.reading_ms, 0);
}

#[test]
fn finishing_counting_words_and_forgetting_progress() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    lib.set_finished_at(&a, Some(99)).unwrap();
    lib.set_word_count(&a, 1234).unwrap();
    let record = lib.book(&a).unwrap().unwrap();
    assert_eq!(
        (record.finished_at, record.word_count),
        (Some(99), Some(1234))
    );
    // Importing the file again keeps both.
    book(&lib, "a");
    let record = lib.book(&a).unwrap().unwrap();
    assert_eq!(
        (record.finished_at, record.word_count),
        (Some(99), Some(1234))
    );
    lib.set_finished_at(&a, None).unwrap();
    assert_eq!(lib.book(&a).unwrap().unwrap().finished_at, None);

    lib.save_progress(
        &a,
        &Progress {
            chapter: 0,
            char_offset: 0,
            segment_id: None,
            updated_at: 1,
            position_ms: None,
        },
    )
    .unwrap();
    lib.delete_progress(&a).unwrap();
    assert_eq!(lib.progress(&a).unwrap(), None);
}

#[test]
fn reading_and_listening_add_up_per_day_and_per_book() {
    let lib = Library::open_in_memory().unwrap();
    let a = book(&lib, "a");
    let b = book(&lib, "b");
    lib.record_reading(&a, "2026-11-10", 1_000).unwrap();
    lib.record_reading(&a, "2026-11-10", 2_000).unwrap();
    lib.record_reading(&b, "2026-11-11", 4_000).unwrap();
    lib.record_listening(&a, "2026-11-11", 10_000).unwrap();

    let stats = lib.activity_stats().unwrap();
    assert_eq!((stats.reading_ms, stats.listening_ms), (7_000, 10_000));
    let days: Vec<_> = stats
        .by_day
        .iter()
        .map(|d| (d.day.as_str(), d.reading_ms, d.listening_ms))
        .collect();
    assert_eq!(
        days,
        [("2026-11-10", 3_000, 0), ("2026-11-11", 4_000, 10_000)]
    );
    let books: Vec<_> = stats
        .by_book
        .iter()
        .map(|b| (b.book_id.0.as_str(), b.reading_ms, b.listening_ms))
        .collect();
    assert_eq!(books, [("a", 3_000, 10_000), ("b", 4_000, 0)]);

    lib.remove_book(&a).unwrap();
    assert_eq!(lib.activity_stats().unwrap().reading_ms, 4_000);
}
