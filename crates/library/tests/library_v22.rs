//! Schema v22: the queue, items lined up to listen to next.
use aoide_library::{BookId, ItemKind, Library, NewBook};

fn item(lib: &Library, id: &str, kind: ItemKind) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind,
            path: format!("{}.item", id.0).into(),
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

fn ids(lib: &Library) -> Vec<String> {
    lib.queue().unwrap().into_iter().map(|b| b.0).collect()
}

#[test]
fn items_are_queued_next_or_last_moved_and_removed() {
    let lib = Library::open_in_memory().unwrap();
    for id in ["a", "b", "c", "d"] {
        item(&lib, id, ItemKind::Article);
    }
    assert!(ids(&lib).is_empty());

    lib.queue_put(&BookId("a".into()), usize::MAX).unwrap();
    lib.queue_put(&BookId("b".into()), usize::MAX).unwrap();
    lib.queue_put(&BookId("c".into()), 0).unwrap();
    assert_eq!(ids(&lib), ["c", "a", "b"]);

    // Queued again, an item moves.
    lib.queue_put(&BookId("c".into()), usize::MAX).unwrap();
    assert_eq!(ids(&lib), ["a", "b", "c"]);
    lib.queue_put(&BookId("c".into()), 1).unwrap();
    assert_eq!(ids(&lib), ["a", "c", "b"]);

    assert!(lib.queue_remove(&BookId("c".into())).unwrap());
    assert!(!lib.queue_remove(&BookId("c".into())).unwrap());
    assert_eq!(ids(&lib), ["a", "b"]);

    lib.queue_put(&BookId("d".into()), 1).unwrap();
    assert_eq!(ids(&lib), ["a", "d", "b"]);
    lib.queue_clear().unwrap();
    assert!(ids(&lib).is_empty());
}

#[test]
fn an_item_removed_from_the_library_leaves_the_queue() {
    let lib = Library::open_in_memory().unwrap();
    let a = item(&lib, "a", ItemKind::Article);
    let b = item(&lib, "b", ItemKind::Book);
    lib.queue_put(&a, usize::MAX).unwrap();
    lib.queue_put(&b, usize::MAX).unwrap();
    lib.remove_book(&a).unwrap();
    assert_eq!(ids(&lib), ["b"]);
}

#[test]
fn a_merged_audiobook_hands_its_place_in_the_queue_to_the_book() {
    let lib = Library::open_in_memory().unwrap();
    let first = item(&lib, "first", ItemKind::Article);
    let audio = item(&lib, "audio", ItemKind::Audiobook);
    let book = item(&lib, "book", ItemKind::Book);
    lib.queue_put(&first, usize::MAX).unwrap();
    lib.queue_put(&audio, usize::MAX).unwrap();
    lib.merge_audiobook(&audio, &book).unwrap();
    assert_eq!(ids(&lib), ["first", "book"]);

    // A book queued already keeps its own place; the audiobook's goes with it.
    let audio = item(&lib, "audio", ItemKind::Audiobook);
    lib.queue_put(&audio, 0).unwrap();
    lib.merge_audiobook(&audio, &book).unwrap();
    assert_eq!(ids(&lib), ["first", "book"]);
}

#[test]
fn a_version_21_library_opens_with_an_empty_queue() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("DROP TABLE queue; PRAGMA user_version = 21;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    assert!(ids(&lib).is_empty());
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 22);
}
