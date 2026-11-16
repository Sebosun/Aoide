//! Schema v9: what kind each item is, and places on an audiobook's timeline.
use aoide_library::{BookId, ItemKind, Library, NewBook, Progress};

fn item(lib: &Library, id: &str, kind: ItemKind) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind,
            path: format!("{}.m4b", id.0).into(),
            title: Some("Synthetic item".into()),
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

#[test]
fn a_version_8_library_opens_with_every_item_a_book() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    {
        let lib = Library::open(&path).unwrap();
        let a = item(&lib, "a", ItemKind::Book);
        lib.save_progress(
            &a,
            &Progress {
                chapter: 1,
                char_offset: 12,
                segment_id: Some("1:0:0".into()),
                updated_at: 5,
                position_ms: None,
            },
        )
        .unwrap();
        lib.add_bookmark(&a, 1, 12, Some("1:0:0"), "Excerpt", "", 6)
            .unwrap();
    }
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE IF EXISTS sites; DROP TABLE IF EXISTS feed_entries; DROP TABLE IF EXISTS feeds; ALTER TABLE books DROP COLUMN shelf; DROP TABLE IF EXISTS articles; ALTER TABLE books DROP COLUMN kind;
         ALTER TABLE progress DROP COLUMN position_ms;
         ALTER TABLE bookmarks DROP COLUMN position_ms;
         DROP TABLE IF EXISTS book_tags; DROP TABLE IF EXISTS tags; PRAGMA user_version = 8;",
    )
    .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let a = BookId("a".into());
    assert_eq!(lib.book(&a).unwrap().unwrap().kind, ItemKind::Book);
    let progress = lib.progress(&a).unwrap().unwrap();
    assert_eq!((progress.char_offset, progress.position_ms), (12, None));
    assert_eq!(lib.bookmarks(&a).unwrap()[0].position_ms, None);
}

#[test]
fn an_audiobook_and_its_places_on_the_timeline_round_trip() {
    let lib = Library::open_in_memory().unwrap();
    let a = item(&lib, "a", ItemKind::Audiobook);
    assert_eq!(lib.book(&a).unwrap().unwrap().kind, ItemKind::Audiobook);
    assert_eq!(lib.books().unwrap()[0].kind, ItemKind::Audiobook);

    let place = Progress {
        chapter: 6,
        char_offset: 0,
        segment_id: None,
        updated_at: 7,
        position_ms: Some(4_000_000_123),
    };
    lib.save_progress(&a, &place).unwrap();
    assert_eq!(lib.progress(&a).unwrap(), Some(place));

    lib.add_time_bookmark(&a, 6, 1_122_000, "Part 7 · 18:42", 8)
        .unwrap();
    lib.add_time_bookmark(&a, 2, 300_000, "Part 3 · 5:00", 9)
        .unwrap();
    let marks = lib.bookmarks(&a).unwrap();
    let places: Vec<_> = marks
        .iter()
        .map(|m| (m.chapter, m.position_ms, m.segment_id.as_deref()))
        .collect();
    assert_eq!(
        places,
        [(2, Some(300_000), None), (6, Some(1_122_000), None)]
    );
}

#[test]
fn a_rescan_of_the_library_folder_leaves_other_kinds_alone() {
    use aoide_library::BookStatus;
    let lib = Library::open_in_memory().unwrap();
    let book = item(&lib, "book", ItemKind::Book);
    let audio = item(&lib, "audio", ItemKind::Audiobook);
    assert_eq!(
        lib.mark_missing_except(&[]).unwrap(),
        std::slice::from_ref(&book)
    );
    assert_eq!(
        lib.book(&audio).unwrap().unwrap().status,
        BookStatus::Present
    );

    assert!(lib.set_missing(&audio, true).unwrap());
    assert!(!lib.set_missing(&audio, true).unwrap(), "already missing");
    assert_eq!(
        lib.book(&audio).unwrap().unwrap().status,
        BookStatus::Missing
    );
    assert!(lib.set_missing(&audio, false).unwrap());
    assert_eq!(
        lib.book(&audio).unwrap().unwrap().status,
        BookStatus::Present
    );
}
