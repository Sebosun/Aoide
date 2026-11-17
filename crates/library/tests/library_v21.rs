//! Schema v21: a feed's collection, which the entries saved from it go on.
use aoide_library::{Library, LibraryError};

#[test]
fn a_feed_keeps_the_collection_its_entries_are_saved_into() {
    let lib = Library::open_in_memory().unwrap();
    let feed = lib.add_feed("https://quay.example/feed.xml", 1).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().collection_id, None);
    let shelf = lib.create_collection("Harbour", 2).unwrap();

    lib.set_feed_collection(feed, Some(shelf)).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().collection_id, Some(shelf));
    // A check stores the feed again without changing it.
    let mut row = lib.feed(feed).unwrap().unwrap();
    row.title = Some("Quay Weekly".into());
    lib.update_feed(&row).unwrap();
    assert_eq!(lib.feeds().unwrap()[0].collection_id, Some(shelf));

    assert!(matches!(
        lib.set_feed_collection(feed, Some(shelf + 99)),
        Err(LibraryError::Invalid(_))
    ));
    // Deleting the collection leaves the feed with none.
    lib.delete_collection(shelf).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().collection_id, None);

    let other = lib.create_collection("Later", 3).unwrap();
    lib.set_feed_collection(feed, Some(other)).unwrap();
    lib.set_feed_collection(feed, None).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().collection_id, None);
}

#[test]
fn a_version_20_library_opens_with_no_feed_collections() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("ALTER TABLE feeds DROP COLUMN collection_id; PRAGMA user_version = 20;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let feed = lib.add_feed("https://quay.example/feed.xml", 1).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().collection_id, None);
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 21);
}
