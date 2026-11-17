//! Schema v23: feed folders, one level, existing while a feed is in one.
use aoide_library::{Library, NewEntry};

fn entry(guid: &str) -> NewEntry {
    NewEntry {
        guid: guid.into(),
        url: None,
        title: Some(format!("Entry {guid}")),
        author: None,
        published_at: None,
        summary: None,
        content_html: None,
        words: None,
    }
}

fn folders(lib: &Library) -> Vec<Option<String>> {
    lib.feeds().unwrap().into_iter().map(|f| f.folder).collect()
}

#[test]
fn a_feed_is_put_in_a_folder_and_a_check_leaves_it_there() {
    let lib = Library::open_in_memory().unwrap();
    let feed = lib.add_feed("https://quay.example/feed.xml", 1).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().folder, None);

    lib.set_feed_folder(feed, Some("Harbour")).unwrap();
    assert_eq!(
        lib.feed(feed).unwrap().unwrap().folder.as_deref(),
        Some("Harbour")
    );
    // A check stores the feed again without changing its folder.
    let mut row = lib.feed(feed).unwrap().unwrap();
    row.folder = None;
    row.title = Some("Quay Weekly".into());
    lib.update_feed(&row).unwrap();
    assert_eq!(folders(&lib), [Some("Harbour".into())]);

    lib.set_feed_folder(feed, None).unwrap();
    assert_eq!(folders(&lib), [None]);
}

#[test]
fn renaming_a_folder_moves_every_feed_in_it() {
    let lib = Library::open_in_memory().unwrap();
    let a = lib.add_feed("https://a.example/feed", 1).unwrap();
    let b = lib.add_feed("https://b.example/feed", 1).unwrap();
    let c = lib.add_feed("https://c.example/feed", 1).unwrap();
    lib.set_feed_folder(a, Some("News")).unwrap();
    lib.set_feed_folder(b, Some("News")).unwrap();
    lib.set_feed_folder(c, Some("Weather")).unwrap();

    lib.rename_feed_folder("News", "Daily news").unwrap();
    assert_eq!(
        folders(&lib),
        [
            Some("Daily news".into()),
            Some("Daily news".into()),
            Some("Weather".into())
        ]
    );
    // Renamed onto another folder, the two become one.
    lib.rename_feed_folder("Weather", "Daily news").unwrap();
    assert_eq!(folders(&lib), vec![Some("Daily news".to_string()); 3]);
}

#[test]
fn a_folder_is_listed_and_marked_read_as_one_stream() {
    let lib = Library::open_in_memory().unwrap();
    let a = lib.add_feed("https://a.example/feed", 1).unwrap();
    let b = lib.add_feed("https://b.example/feed", 1).unwrap();
    let c = lib.add_feed("https://c.example/feed", 1).unwrap();
    lib.set_feed_folder(a, Some("News")).unwrap();
    lib.set_feed_folder(b, Some("News")).unwrap();
    lib.insert_entry(a, &entry("a1"), 10).unwrap();
    lib.insert_entry(b, &entry("b1"), 20).unwrap();
    lib.insert_entry(c, &entry("c1"), 30).unwrap();

    let guids = |folder: Option<&str>, filter: &str| -> Vec<String> {
        lib.list_entries(None, folder, filter, None, 50)
            .unwrap()
            .into_iter()
            .map(|e| e.entry.guid)
            .collect()
    };
    assert_eq!(guids(Some("News"), "all"), ["b1", "a1"]);
    assert_eq!(guids(None, "all"), ["c1", "b1", "a1"]);

    lib.mark_all_read(None, Some("News"), 40).unwrap();
    assert_eq!(guids(Some("News"), "unread"), Vec::<String>::new());
    assert_eq!(guids(None, "unread"), ["c1"]);
}

#[test]
fn a_version_22_library_opens_with_no_folders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    drop(Library::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch("ALTER TABLE feeds DROP COLUMN folder; PRAGMA user_version = 22;")
        .unwrap();
    drop(conn);

    let lib = Library::open(&path).unwrap();
    let feed = lib.add_feed("https://quay.example/feed.xml", 1).unwrap();
    assert_eq!(lib.feed(feed).unwrap().unwrap().folder, None);
    let version: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 26);
}
