//! Schema v11: feeds, their entries, and the shelf an item is on.
use aoide_library::{BookId, EntryRow, ItemKind, Library, NewBook, NewEntry};

fn article(lib: &Library, id: &str) -> BookId {
    let id = BookId(id.into());
    lib.upsert_book(
        &NewBook {
            id: id.clone(),
            kind: ItemKind::Article,
            path: format!("articles/{}/snapshot.html", id.0).into(),
            title: Some(format!("Article {}", id.0)),
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

fn entry(guid: &str) -> NewEntry {
    NewEntry {
        guid: guid.into(),
        url: Some(format!("https://harbour.example/{guid}")),
        title: Some(format!("Entry {guid}")),
        author: None,
        published_at: Some(1_000),
        summary: Some("The ferry arrives in spring.".into()),
        content_html: None,
        words: None,
    }
}

fn ids(books: Vec<aoide_library::BookRecord>) -> Vec<String> {
    books.into_iter().map(|b| b.id.0).collect()
}

#[test]
fn a_version_10_library_keeps_every_item_on_the_library_shelf() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let lib = Library::open(&path).unwrap();
    article(&lib, "a");
    drop(lib);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "DROP TABLE IF EXISTS sites; DROP TABLE feed_entries; DROP TABLE feeds; ALTER TABLE books DROP COLUMN shelf;
         PRAGMA user_version = 10;",
    )
    .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    assert_eq!(ids(lib.books().unwrap()), ["a"]);
    assert_eq!(
        lib.book(&BookId("a".into())).unwrap().unwrap().shelf,
        "library"
    );
    assert!(lib.feeds().unwrap().is_empty());
}

#[test]
fn items_on_the_feed_shelf_stay_out_of_the_library() {
    let lib = Library::open_in_memory().unwrap();
    let (a, b) = (article(&lib, "a"), article(&lib, "b"));
    lib.set_shelf(&b, "feed").unwrap();
    assert_eq!(ids(lib.books().unwrap()), ["a"]);
    assert_eq!(ids(lib.feed_shelf_books().unwrap()), ["b"]);
    assert_eq!(lib.book(&b).unwrap().unwrap().shelf, "feed");
    // Saving it later is moving it back; storing it again keeps its shelf.
    article(&lib, "b");
    assert_eq!(lib.book(&b).unwrap().unwrap().shelf, "feed");
    lib.set_shelf(&b, "library").unwrap();
    assert_eq!(ids(lib.books().unwrap()).len(), 2);
    assert!(lib.set_shelf(&a, "attic").is_err(), "only known shelves");
}

#[test]
fn a_feed_is_kept_once_and_its_entries_once_each() {
    let lib = Library::open_in_memory().unwrap();
    let harbour = lib
        .add_feed("https://harbour.example/feed.xml", 10)
        .unwrap();
    assert_eq!(
        lib.add_feed("https://harbour.example/feed.xml", 20)
            .unwrap(),
        harbour
    );
    let tides = lib.add_feed("https://tides.example/feed.json", 30).unwrap();
    assert_eq!(lib.feeds().unwrap().len(), 2);

    let first = lib.insert_entry(harbour, &entry("g1"), 100).unwrap();
    assert!(first.is_some());
    assert_eq!(lib.insert_entry(harbour, &entry("g1"), 200).unwrap(), None);
    assert!(lib
        .insert_entry(tides, &entry("g1"), 200)
        .unwrap()
        .is_some());

    let stored: EntryRow = lib.entry(first.unwrap()).unwrap().unwrap();
    assert_eq!(stored.feed_id, harbour);
    assert_eq!(stored.guid, "g1");
    assert_eq!(stored.seen_at, 100);
    assert_eq!(stored.read_at, None);
    assert_eq!(stored.book_id, None);

    // Removing the feed removes its entries.
    lib.remove_feed(harbour).unwrap();
    assert_eq!(lib.entry(first.unwrap()).unwrap(), None);
    assert_eq!(lib.feeds().unwrap().len(), 1);
}

#[test]
fn an_entry_outlives_the_item_it_was_opened_as() {
    let lib = Library::open_in_memory().unwrap();
    let feed = lib
        .add_feed("https://harbour.example/feed.xml", 10)
        .unwrap();
    let id = lib.insert_entry(feed, &entry("g1"), 100).unwrap().unwrap();
    let item = article(&lib, "a");
    lib.link_entry(id, &item).unwrap();
    assert_eq!(lib.entry(id).unwrap().unwrap().book_id, Some(item.clone()));
    lib.remove_book(&item).unwrap();
    let left = lib.entry(id).unwrap().unwrap();
    assert_eq!(left.book_id, None);
    assert_eq!(left.title.as_deref(), Some("Entry g1"));
}

#[test]
fn unread_entries_are_counted_and_a_new_feed_keeps_only_its_newest_unread() {
    let lib = Library::open_in_memory().unwrap();
    let feed = lib
        .add_feed("https://harbour.example/feed.xml", 10)
        .unwrap();
    for day in 0..12 {
        let mut e = entry(&format!("g{day}"));
        e.published_at = Some(day * 86_400_000);
        lib.insert_entry(feed, &e, 100).unwrap();
    }
    assert_eq!(lib.unread_counts().unwrap().get(&feed), Some(&12));
    lib.mark_read_except_newest(feed, 10, 200).unwrap();
    assert_eq!(lib.unread_counts().unwrap().get(&feed), Some(&10));
    let row = |guid: &str| {
        let id = lib.insert_entry(feed, &entry(guid), 300).unwrap();
        assert_eq!(id, None);
        lib.entries_of(feed)
            .unwrap()
            .into_iter()
            .find(|e| e.guid == guid)
            .unwrap()
    };
    assert_eq!(row("g0").read_at, Some(200));
    assert_eq!(row("g11").read_at, None);

    // A failing feed keeps when its failures began.
    let mut stored = lib.feed(feed).unwrap().unwrap();
    stored.failures = 2;
    stored.failing_since = Some(500);
    lib.update_feed(&stored).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap(), stored);
}
